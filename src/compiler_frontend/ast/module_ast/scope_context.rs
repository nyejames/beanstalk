//! Shared parser/lowering context for one active AST scope.
//!
//! WHAT: `ScopeContext` carries all state needed to parse/lower a single scope — declarations,
//! visibility gates, type expectations, and optional path-resolution capability.
//!
//! WHY: passing it as one struct avoids large parameter lists across recursive parsing calls,
//! and makes clone-to-child easy while keeping shared mutation through `Rc<RefCell<>>`.
//!
//! ## Relationship to AST emission
//!
//! `AstEmitter` creates `ScopeContext` fresh for each function/template body after the semantic
//! environment is complete. `ScopeContext` owns only local scope growth (`local_declarations`,
//! `loop_depth`, type expectations).
//!
//! `ScopeContext` receives shared state from the completed environment (for example
//! `Rc<TopLevelDeclarationTable>` for top-level symbols and `Rc<ReceiverMethodCatalog>` for
//! method lookup) so body parsing is self-contained without referencing the mutable environment
//! builder directly.
//!
//! ## External symbol visibility
//!
//! File-local visibility originates from the header-built `FileVisibility` struct and is
//! applied to each `ScopeContext` via `with_file_visibility`. This includes same-file
//! declarations, imported source symbols, type aliases, and external package symbols.
//!
//! `visible_external_symbols` stores source-visible names mapped to already-resolved
//! `ExternalSymbolId` values. Expression and type resolution must use these IDs directly;
//! they must never re-resolve names globally through the registry.

use crate::compiler_frontend::FrontendBuildProfile;
use crate::compiler_frontend::ast::ast_nodes::Declaration;
use crate::compiler_frontend::ast::instrumentation::{
    AstCounter, add_ast_counter, increment_ast_counter,
};
use crate::compiler_frontend::ast::module_ast::environment::{
    AstModuleEnvironment, TopLevelDeclarationTable,
};
use crate::compiler_frontend::ast::statements::functions::FunctionSignature;
use crate::compiler_frontend::ast::templates::template_folding::TemplateFoldContext;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::compiler_warnings::CompilerWarning;
use crate::compiler_frontend::datatypes::generics::GenericNominalInstantiationCache;
use crate::compiler_frontend::datatypes::{DataType, ReceiverKey};
use crate::compiler_frontend::external_packages::{
    ExternalAbiType, ExternalFunctionDef, ExternalFunctionId, ExternalPackageRegistry,
    ExternalSymbolId, ExternalTypeId,
};
use crate::compiler_frontend::headers::import_environment::{
    FileVisibility, HeaderImportEnvironment,
};
use crate::compiler_frontend::headers::module_symbols::{
    GenericDeclarationMetadata, ModuleSymbols,
};
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::style_directives::StyleDirectiveRegistry;
use crate::compiler_frontend::symbols::string_interning::{StringId, StringTable};
use crate::return_compiler_error;
use rustc_hash::{FxHashMap, FxHashSet};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::compiler_frontend::paths::path_format::PathStringFormatConfig;
use crate::compiler_frontend::paths::path_resolution::ProjectPathResolver;
use crate::compiler_frontend::paths::rendered_path_usage::RenderedPathUsage;

pub(crate) use crate::compiler_frontend::ast::receiver_methods::{
    ReceiverMethodCatalog, ReceiverMethodEntry,
};

/// Checks whether an `ExternalAbiType` is compatible with a frontend `DataType`
/// for receiver method dispatch.
fn external_abi_matches_datatype(abi_type: &ExternalAbiType, data_type: &DataType) -> bool {
    matches!(
        (abi_type, data_type),
        (ExternalAbiType::Inferred, _)
            | (ExternalAbiType::I32, DataType::Int)
            | (ExternalAbiType::F64, DataType::Float)
            | (ExternalAbiType::Bool, DataType::Bool)
            | (ExternalAbiType::Utf8Str, DataType::StringSlice)
            | (ExternalAbiType::Handle, DataType::External { .. })
    )
}

pub(super) static CONTROL_FLOW_SCOPE_COUNTER: AtomicUsize = AtomicUsize::new(0);

/// Immutable shared state common to a scope and all its cloned children.
///
/// WHAT: bundles all state that is identical across child scopes so cloning a
/// `ScopeContext` only copies per-scope mutable fields and one `Rc` pointer.
/// WHY: eliminates deep cloning of visibility maps, registries, and lookup tables
/// every time a child control-flow or expression scope is created.
#[derive(Clone)]
pub struct ScopeShared {
    pub(crate) environment: Rc<AstModuleEnvironment>,
    pub(crate) top_level_declarations: Rc<TopLevelDeclarationTable>,
    pub(crate) external_package_registry: ExternalPackageRegistry,
    pub(crate) style_directives: StyleDirectiveRegistry,
    pub(crate) build_profile: FrontendBuildProfile,
    pub(crate) file_visibility: Option<Rc<FileVisibility>>,
    pub(crate) resolved_type_aliases: Option<Rc<FxHashMap<InternedPath, DataType>>>,
    pub(crate) generic_declarations_by_path:
        Option<Rc<FxHashMap<InternedPath, GenericDeclarationMetadata>>>,
    pub(crate) resolved_struct_fields_by_path:
        Option<Rc<FxHashMap<InternedPath, Vec<Declaration>>>>,
    pub(crate) generic_nominal_instantiations: Option<Rc<GenericNominalInstantiationCache>>,
    pub(crate) emitted_warnings: Rc<RefCell<Vec<CompilerWarning>>>,
    pub(crate) rendered_path_usages: Rc<RefCell<Vec<RenderedPathUsage>>>,
    pub(crate) project_path_resolver: Option<ProjectPathResolver>,
    pub(crate) source_file_scope: Option<InternedPath>,
    pub(crate) path_format_config: PathStringFormatConfig,
    pub(crate) receiver_methods: Rc<ReceiverMethodCatalog>,
}

#[derive(Clone)]
/// Shared parser/lowering context for one active AST scope.
pub struct ScopeContext {
    // --- Core scope identity ---
    pub kind: ContextKind,
    pub scope: InternedPath,

    // --- Immutable shared state (cheap Rc clone for children) ---
    pub(crate) shared: Rc<ScopeShared>,

    // --- Per-scope mutable state ---
    // Per-scope locals: function parameters + body-declared variables only.
    // Grows incrementally in source order via add_var(); never carries module-wide data.
    pub local_declarations: Vec<Declaration>,
    // Indexed local lookup: name → ordered indices into local_declarations.
    // Preserves "latest visible local wins" without reverse scanning the full vec.
    local_declarations_by_name: FxHashMap<StringId, Vec<u32>>,
    // Optional file-local visibility gate over declarations.
    // When present, references must be in this set, which enforces import boundaries.
    // Kept directly on ScopeContext (not in ScopeShared) because add_var mutates it.
    pub visible_declaration_ids: Option<FxHashSet<InternedPath>>,

    // --- Type expectations ---
    pub expected_result_types: Vec<DataType>,
    pub expected_error_type: Option<DataType>,

    // --- Control flow state ---
    pub loop_depth: usize,
}

impl std::ops::Deref for ScopeContext {
    type Target = ScopeShared;
    fn deref(&self) -> &Self::Target {
        &self.shared
    }
}

#[derive(Debug, PartialEq, Clone)]
/// High-level scope categories used by parser/lowering rules.
pub enum ContextKind {
    Module, // The top-level scope of each file in the module
    Expression,
    Constant, // An expression that is enforced to be evaluated at compile time and can't contain non-constant references
    ConstantHeader, // Top-level exported constant declaration context (#name = ...)
    Function,
    Condition, // For loops and if statements
    Loop,
    Block,
    Branch,
    MatchArm, // Statement body of one `case ... =>` or `else =>` arm in a match block
    Template,
}

impl ContextKind {
    pub fn is_constant_context(&self) -> bool {
        matches!(self, ContextKind::Constant | ContextKind::ConstantHeader)
    }

    pub fn allows_const_record_coercion(&self) -> bool {
        matches!(self, ContextKind::ConstantHeader)
    }
}

fn build_local_declarations_index(declarations: &[Declaration]) -> FxHashMap<StringId, Vec<u32>> {
    let mut index: FxHashMap<StringId, Vec<u32>> = FxHashMap::default();
    for (i, declaration) in declarations.iter().enumerate() {
        if let Some(name) = declaration.id.name() {
            index.entry(name).or_default().push(i as u32);
        }
    }
    index
}

// --------------------------
//  Constructors
// --------------------------

impl ScopeContext {
    pub(crate) fn new(
        kind: ContextKind,
        scope: InternedPath,
        top_level_declarations: Rc<TopLevelDeclarationTable>,
        external_package_registry: ExternalPackageRegistry,
        expected_result_types: Vec<DataType>,
    ) -> ScopeContext {
        increment_ast_counter(AstCounter::ScopeContextsCreated);

        let environment = Rc::new(AstModuleEnvironment {
            module_symbols: ModuleSymbols::empty(),
            import_environment: HeaderImportEnvironment::default(),
            warnings: Vec::new(),
            declaration_table: top_level_declarations,
            module_constants: Vec::new(),
            rendered_path_usages: Rc::new(RefCell::new(Vec::new())),
            builtin_struct_ast_nodes: Vec::new(),
            resolved_struct_fields_by_path: Rc::new(FxHashMap::default()),
            resolved_function_signatures_by_path: Rc::new(FxHashMap::default()),
            resolved_type_aliases_by_path: Rc::new(FxHashMap::default()),
            receiver_methods: Rc::new(ReceiverMethodCatalog::default()),
            generic_nominal_instantiations: Rc::new(GenericNominalInstantiationCache::new()),
            generic_declarations_by_path: Rc::new(FxHashMap::default()),
            external_package_registry,
            style_directives: StyleDirectiveRegistry::built_ins(),
            build_profile: FrontendBuildProfile::Dev,
            project_path_resolver: None,
            path_format_config: PathStringFormatConfig::default(),
        });

        let shared = Rc::new(ScopeShared {
            environment: Rc::clone(&environment),
            top_level_declarations: Rc::clone(&environment.declaration_table),
            external_package_registry: environment.external_package_registry.clone(),
            style_directives: environment.style_directives.clone(),
            build_profile: environment.build_profile,
            file_visibility: None,
            resolved_type_aliases: None,
            generic_declarations_by_path: None,
            resolved_struct_fields_by_path: None,
            generic_nominal_instantiations: None,
            emitted_warnings: Rc::new(RefCell::new(Vec::new())),
            rendered_path_usages: Rc::clone(&environment.rendered_path_usages),
            project_path_resolver: environment.project_path_resolver.clone(),
            source_file_scope: None,
            path_format_config: environment.path_format_config.clone(),
            receiver_methods: Rc::clone(&environment.receiver_methods),
        });

        ScopeContext {
            kind,
            scope,
            shared,
            local_declarations: Vec::new(),
            local_declarations_by_name: FxHashMap::default(),
            visible_declaration_ids: None,
            expected_result_types,
            expected_error_type: None,
            loop_depth: 0,
        }
    }

    pub fn new_child_control_flow(
        &self,
        kind: ContextKind,
        string_table: &mut StringTable,
    ) -> ScopeContext {
        increment_ast_counter(AstCounter::ScopeContextsCreated);
        add_ast_counter(
            AstCounter::ScopeLocalDeclarationsClonedTotal,
            self.local_declarations.len(),
        );

        let loop_depth = if matches!(kind, ContextKind::Loop) {
            self.loop_depth + 1
        } else {
            self.loop_depth
        };

        let scope_id = CONTROL_FLOW_SCOPE_COUNTER.fetch_add(1, Ordering::Relaxed);
        let scope = self
            .scope
            .join_str(&format!("__scope_{scope_id}"), string_table);

        ScopeContext {
            kind,
            scope,
            shared: Rc::clone(&self.shared),
            local_declarations: self.local_declarations.clone(),
            local_declarations_by_name: self.local_declarations_by_name.clone(),
            visible_declaration_ids: self.visible_declaration_ids.clone(),
            expected_result_types: self.expected_result_types.clone(),
            expected_error_type: self.expected_error_type.clone(),
            loop_depth,
        }
    }

    pub fn new_child_function(
        &self,
        function_name: StringId,
        signature: FunctionSignature,
        _string_table: &mut StringTable,
    ) -> ScopeContext {
        increment_ast_counter(AstCounter::ScopeContextsCreated);
        add_ast_counter(
            AstCounter::ScopeLocalDeclarationsClonedTotal,
            self.local_declarations.len(),
        );

        let mut new_context = self.to_owned();
        new_context.kind = ContextKind::Function;
        new_context.expected_result_types = signature.return_data_types();
        new_context.expected_error_type = signature
            .error_return()
            .map(|ret| ret.data_type().to_owned());

        // Create a new scope path by joining the current scope with the function name
        new_context.scope = self.scope.append(function_name);
        new_context.loop_depth = 0;

        // Share the top-level declaration table (cheap Rc clone); reset locals to params only.
        new_context.set_local_declarations(signature.parameters);

        new_context
    }

    pub fn new_child_expression(&self, expected_result_types: Vec<DataType>) -> ScopeContext {
        increment_ast_counter(AstCounter::ScopeContextsCreated);
        add_ast_counter(
            AstCounter::ScopeLocalDeclarationsClonedTotal,
            self.local_declarations.len(),
        );

        ScopeContext {
            kind: ContextKind::Expression,
            scope: self.scope.clone(),
            shared: Rc::clone(&self.shared),
            local_declarations: self.local_declarations.clone(),
            local_declarations_by_name: self.local_declarations_by_name.clone(),
            visible_declaration_ids: self.visible_declaration_ids.clone(),
            expected_result_types,
            expected_error_type: self.expected_error_type.clone(),
            loop_depth: self.loop_depth,
        }
    }

    /// Build the context used while parsing template expressions.
    ///
    /// Constant contexts stay constant so template-head captures can inline
    /// compile-time values. All other contexts parse templates as runtime-capable.
    pub fn new_template_parsing_context(&self) -> ScopeContext {
        increment_ast_counter(AstCounter::ScopeContextsCreated);
        add_ast_counter(
            AstCounter::ScopeLocalDeclarationsClonedTotal,
            self.local_declarations.len(),
        );

        let template_kind = if self.kind.is_constant_context() {
            self.kind.clone()
        } else {
            ContextKind::Template
        };

        ScopeContext {
            kind: template_kind,
            scope: self.scope.clone(),
            shared: Rc::clone(&self.shared),
            local_declarations: self.local_declarations.clone(),
            local_declarations_by_name: self.local_declarations_by_name.clone(),
            visible_declaration_ids: self.visible_declaration_ids.clone(),
            expected_result_types: vec![],
            expected_error_type: self.expected_error_type.clone(),
            loop_depth: self.loop_depth,
        }
    }

    /// Builds a constant child context that preserves project-aware folding/path state.
    ///
    /// WHAT: clones the parent visibility/declaration environment and forces
    ///       resolver + source file scope propagation into constant parsing paths.
    /// WHY: resolver-less constant contexts are invalid for template folding and
    ///      template-head path coercion.
    pub fn new_constant(scope: InternedPath, parent: &ScopeContext) -> ScopeContext {
        increment_ast_counter(AstCounter::ScopeContextsCreated);
        add_ast_counter(
            AstCounter::ScopeLocalDeclarationsClonedTotal,
            parent.local_declarations.len(),
        );

        ScopeContext {
            kind: ContextKind::Constant,
            scope,
            shared: Rc::clone(&parent.shared),
            local_declarations: parent.local_declarations.clone(),
            local_declarations_by_name: parent.local_declarations_by_name.clone(),
            visible_declaration_ids: parent.visible_declaration_ids.clone(),
            expected_result_types: Vec::new(),
            expected_error_type: parent.expected_error_type.clone(),
            loop_depth: parent.loop_depth,
        }
    }

    // --------------------------
    //  Required services
    // --------------------------

    pub(crate) fn required_project_path_resolver(
        &self,
        operation: &str,
    ) -> Result<&ProjectPathResolver, CompilerError> {
        let Some(resolver) = self.project_path_resolver.as_ref() else {
            return_compiler_error!(
                "Missing project path resolver during '{}'. Context scope: '{}'. This is a compiler setup bug.",
                operation,
                format!("{:?}", self.scope)
            );
        };
        Ok(resolver)
    }

    pub(crate) fn required_source_file_scope(
        &self,
        operation: &str,
    ) -> Result<&InternedPath, CompilerError> {
        let Some(source_scope) = self.shared.source_file_scope.as_ref() else {
            return_compiler_error!(
                "Missing source file scope during '{}'. Context scope: '{}'. This is a compiler setup bug.",
                operation,
                format!("{:?}", self.scope)
            );
        };
        Ok(source_scope)
    }

    pub fn new_template_fold_context<'b>(
        &'b self,
        string_table: &'b mut StringTable,
        operation: &str,
    ) -> Result<TemplateFoldContext<'b>, CompilerError> {
        let resolver = self.required_project_path_resolver(operation)?;
        let source_file_scope = self.required_source_file_scope(operation)?;
        Ok(TemplateFoldContext {
            string_table,
            project_path_resolver: resolver,
            path_format_config: &self.path_format_config,
            source_file_scope,
        })
    }

    // --------------------------
    //  Builder setters
    // --------------------------

    pub fn with_build_profile(mut self, profile: FrontendBuildProfile) -> ScopeContext {
        Rc::make_mut(&mut self.shared).build_profile = profile;
        self
    }

    pub fn with_visible_declarations(mut self, visible: FxHashSet<InternedPath>) -> ScopeContext {
        // A context without this gate can resolve any declaration in the module.
        // File/start contexts set this to enforce import semantics.
        self.visible_declaration_ids = Some(visible);
        self
    }

    pub fn with_visible_external_symbols(
        mut self,
        visible: FxHashMap<StringId, ExternalSymbolId>,
    ) -> ScopeContext {
        let shared = Rc::make_mut(&mut self.shared);
        let mut file_visibility = shared
            .file_visibility
            .as_ref()
            .map(|f| (**f).clone())
            .unwrap_or_default();
        file_visibility.visible_external_symbols = visible;
        shared.file_visibility = Some(Rc::new(file_visibility));
        self
    }

    pub fn with_visible_source_bindings(
        mut self,
        bindings: FxHashMap<StringId, InternedPath>,
    ) -> ScopeContext {
        let shared = Rc::make_mut(&mut self.shared);
        let mut file_visibility = shared
            .file_visibility
            .as_ref()
            .map(|f| (**f).clone())
            .unwrap_or_default();
        file_visibility.visible_source_names = bindings;
        shared.file_visibility = Some(Rc::new(file_visibility));
        self
    }

    pub fn with_visible_type_aliases(
        mut self,
        aliases: FxHashMap<StringId, InternedPath>,
    ) -> ScopeContext {
        let shared = Rc::make_mut(&mut self.shared);
        let mut file_visibility = shared
            .file_visibility
            .as_ref()
            .map(|f| (**f).clone())
            .unwrap_or_default();
        file_visibility.visible_type_alias_names = aliases;
        shared.file_visibility = Some(Rc::new(file_visibility));
        self
    }

    /// Apply a header-built `FileVisibility` to this scope context.
    ///
    /// WHAT: copies all visibility maps from the prepared header environment.
    /// WHY: AST emission should consume header-built visibility directly instead of
    /// reconstructing import bindings or manually setting each field.
    pub(crate) fn with_file_visibility(
        mut self,
        visibility: Rc<crate::compiler_frontend::headers::import_environment::FileVisibility>,
    ) -> ScopeContext {
        self.visible_declaration_ids = Some(visibility.visible_declaration_paths.clone());
        Rc::make_mut(&mut self.shared).file_visibility = Some(visibility);
        self
    }

    pub fn with_resolved_type_aliases(
        mut self,
        aliases: Rc<FxHashMap<InternedPath, DataType>>,
    ) -> ScopeContext {
        Rc::make_mut(&mut self.shared).resolved_type_aliases = Some(aliases);
        self
    }

    pub(crate) fn with_generic_declarations(
        mut self,
        declarations: Rc<FxHashMap<InternedPath, GenericDeclarationMetadata>>,
    ) -> ScopeContext {
        Rc::make_mut(&mut self.shared).generic_declarations_by_path = Some(declarations);
        self
    }

    pub(crate) fn with_resolved_struct_fields_by_path(
        mut self,
        fields: Rc<FxHashMap<InternedPath, Vec<Declaration>>>,
    ) -> ScopeContext {
        Rc::make_mut(&mut self.shared).resolved_struct_fields_by_path = Some(fields);
        self
    }

    pub(crate) fn with_generic_nominal_instantiations(
        mut self,
        cache: Rc<GenericNominalInstantiationCache>,
    ) -> ScopeContext {
        Rc::make_mut(&mut self.shared).generic_nominal_instantiations = Some(cache);
        self
    }

    pub fn with_style_directives(
        mut self,
        style_directives: &StyleDirectiveRegistry,
    ) -> ScopeContext {
        Rc::make_mut(&mut self.shared).style_directives = style_directives.clone();
        self
    }

    pub(crate) fn with_project_path_resolver(
        mut self,
        resolver: Option<ProjectPathResolver>,
    ) -> ScopeContext {
        Rc::make_mut(&mut self.shared).project_path_resolver = resolver;
        self
    }

    pub fn with_source_file_scope(mut self, source_file: InternedPath) -> ScopeContext {
        Rc::make_mut(&mut self.shared).source_file_scope = Some(source_file);
        self
    }

    pub fn with_path_format_config(mut self, config: PathStringFormatConfig) -> ScopeContext {
        Rc::make_mut(&mut self.shared).path_format_config = config;
        self
    }

    pub fn with_rendered_path_usage_sink(
        mut self,
        sink: Rc<RefCell<Vec<RenderedPathUsage>>>,
    ) -> ScopeContext {
        Rc::make_mut(&mut self.shared).rendered_path_usages = sink;
        self
    }

    pub(crate) fn with_receiver_methods(
        mut self,
        receiver_methods: Rc<ReceiverMethodCatalog>,
    ) -> ScopeContext {
        Rc::make_mut(&mut self.shared).receiver_methods = receiver_methods;
        self
    }

    // --------------------------
    //  Local mutation
    // --------------------------

    pub(crate) fn set_local_declarations(&mut self, declarations: Vec<Declaration>) {
        self.local_declarations_by_name = build_local_declarations_index(&declarations);
        self.local_declarations = declarations;
    }

    // --------------------------
    //  Symbol lookup
    // --------------------------

    pub(crate) fn get_reference(&self, name: &StringId) -> Option<&Declaration> {
        // 1. Locals (latest visible local wins)
        if let Some(indices) = self.local_declarations_by_name.get(name) {
            return indices
                .iter()
                .rev()
                .map(|index| &self.local_declarations[*index as usize])
                .next();
        }

        // 2. Source-visible names → canonical declaration path.
        // Includes same-file declarations and imported source symbols (aliased or not).
        // When file_visibility is populated (production contexts), this is the
        // *only* path for cross-file name lookup. The fallback below is only for test
        // contexts that do not set file_visibility.
        // Skip receiver methods: they must be called via receiver syntax, and the
        // receiver method catalog handles their lookup.
        if let Some(file_visibility) = &self.file_visibility {
            if let Some(canonical_path) = file_visibility.visible_source_names.get(name)
                && let Some(declaration) = self
                    .shared
                    .environment
                    .declaration_table
                    .get_by_path(canonical_path)
                && !matches!(
                    &declaration.value.data_type,
                    DataType::Function(receiver, _) if receiver.as_ref().is_some()
                )
            {
                return Some(declaration);
            }
            // file_visibility is set but name not found — do not fall back.
            // This ensures import aliases hide the original name.
            return None;
        }

        // 3. Fallback for contexts that do not set file_visibility
        // (e.g. synthetic evaluation contexts and some unit-test helpers).
        self.shared
            .environment
            .declaration_table
            .get_visible_non_receiver_by_name(*name, self.visible_declaration_ids.as_ref())
    }

    pub(crate) fn lookup_receiver_method(
        &self,
        receiver: &ReceiverKey,
        method_name: StringId,
    ) -> Option<&ReceiverMethodEntry> {
        let entry = self
            .receiver_methods
            .by_receiver_and_name
            .get(&(receiver.to_owned(), method_name))?;

        let current_source_file = self.source_file_scope.as_ref()?;
        if &entry.source_file == current_source_file || entry.exported {
            Some(entry)
        } else {
            None
        }
    }

    pub(crate) fn lookup_visible_receiver_method_by_name(
        &self,
        method_name: StringId,
    ) -> Option<&ReceiverMethodEntry> {
        let current_source_file = self.source_file_scope.as_ref()?;
        let entries = self.receiver_methods.by_method_name.get(&method_name)?;

        entries
            .iter()
            .find(|entry| &entry.source_file == current_source_file)
            .or_else(|| entries.iter().find(|entry| entry.exported))
    }

    /// Look up a visible external function by its source-level name.
    pub(crate) fn lookup_visible_external_function(
        &self,
        name: StringId,
    ) -> Option<(
        ExternalFunctionId,
        &crate::compiler_frontend::external_packages::ExternalFunctionDef,
    )> {
        let fv = self.file_visibility.as_ref()?;
        let symbol_id = *fv.visible_external_symbols.get(&name)?;
        let ExternalSymbolId::Function(func_id) = symbol_id else {
            return None;
        };
        self.external_package_registry
            .get_function_by_id(func_id)
            .map(|def| (func_id, def))
    }

    /// Look up a visible external type by its source-level name.
    pub(crate) fn lookup_visible_external_type(
        &self,
        name: StringId,
    ) -> Option<(
        ExternalTypeId,
        &crate::compiler_frontend::external_packages::ExternalTypeDef,
    )> {
        let fv = self.file_visibility.as_ref()?;
        let symbol_id = *fv.visible_external_symbols.get(&name)?;
        let ExternalSymbolId::Type(type_id) = symbol_id else {
            return None;
        };
        self.external_package_registry
            .get_type_by_id(type_id)
            .map(|def| (type_id, def))
    }

    /// Look up a visible external constant by its source-level name.
    pub(crate) fn lookup_visible_external_constant(
        &self,
        name: StringId,
    ) -> Option<(
        crate::compiler_frontend::external_packages::ExternalConstantId,
        &crate::compiler_frontend::external_packages::ExternalConstantDef,
    )> {
        let fv = self.file_visibility.as_ref()?;
        let symbol_id = *fv.visible_external_symbols.get(&name)?;
        let ExternalSymbolId::Constant(const_id) = symbol_id else {
            return None;
        };
        self.external_package_registry
            .get_constant_by_id(const_id)
            .map(|def| (const_id, def))
    }

    /// Look up a visible external receiver method by receiver type and method name.
    ///
    /// WHAT: only considers external functions in `file_visibility.visible_external_symbols`; checks
    ///       receiver compatibility against the def's `receiver_type`.
    /// WHY: package-scoped external symbols must respect file-local visibility.
    pub(crate) fn lookup_visible_external_method(
        &self,
        receiver_type: &DataType,
        method_name: StringId,
    ) -> Option<(ExternalFunctionId, &ExternalFunctionDef)> {
        let fv = self.file_visibility.as_ref()?;
        let symbol_id = *fv.visible_external_symbols.get(&method_name)?;
        let ExternalSymbolId::Function(func_id) = symbol_id else {
            return None;
        };
        let def = self.external_package_registry.get_function_by_id(func_id)?;
        let expected_abi = def.receiver_type.as_ref()?;
        if external_abi_matches_datatype(expected_abi, receiver_type) {
            Some((func_id, def))
        } else {
            None
        }
    }

    /// Check whether a name is a visible type alias, regardless of whether its target
    /// has been resolved yet.
    ///
    /// WHAT: used by expression parsing to give a precise diagnostic when a type alias
    /// is mistakenly used in value position.
    pub(crate) fn is_visible_type_alias_name(&self, name: StringId) -> bool {
        self.shared
            .file_visibility
            .as_ref()
            .is_some_and(|fv| fv.visible_type_alias_names.contains_key(&name))
    }

    // --------------------------
    //  Warning / path tracking
    // --------------------------

    pub fn add_var(&mut self, declaration: Declaration) {
        if let Some(visible_declarations) = self.visible_declaration_ids.as_mut() {
            visible_declarations.insert(declaration.id.clone());
        }
        if let Some(name) = declaration.id.name() {
            let index = self.local_declarations.len() as u32;
            self.local_declarations_by_name
                .entry(name)
                .or_default()
                .push(index);
        }
        self.local_declarations.push(declaration);
    }

    pub fn is_inside_loop(&self) -> bool {
        self.loop_depth > 0
    }

    pub fn emit_warning(&self, warning: CompilerWarning) {
        self.shared.emitted_warnings.borrow_mut().push(warning);
    }

    pub fn take_emitted_warnings(&self) -> Vec<CompilerWarning> {
        std::mem::take(&mut *self.shared.emitted_warnings.borrow_mut())
    }

    pub fn record_rendered_path_usages(&self, usages: Vec<RenderedPathUsage>) {
        self.rendered_path_usages.borrow_mut().extend(usages);
    }

    pub fn take_rendered_path_usages(&self) -> Vec<RenderedPathUsage> {
        std::mem::take(&mut *self.rendered_path_usages.borrow_mut())
    }
}
