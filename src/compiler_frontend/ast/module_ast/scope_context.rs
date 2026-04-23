//! Shared parser/lowering context for one active AST scope.
//!
//! WHAT: `ScopeContext` carries all state needed to parse/lower a single scope — declarations,
//! visibility gates, type expectations, and optional path-resolution capability.
//!
//! WHY: passing it as one struct avoids large parameter lists across recursive parsing calls,
//! and makes clone-to-child easy while keeping shared mutation through `Rc<RefCell<>>`.
//!
//! ## Relationship to `AstBuildState`
//!
//! `AstBuildState` owns the module-wide accumulation across passes (output vectors, type tables,
//! the manifest). `ScopeContext` is created fresh for each function/template body in pass 6
//! ([`pass_emit_nodes`](crate::compiler_frontend::ast::module_ast::pass_emit_nodes)) and owns
//! only local scope growth (`local_declarations`, `loop_depth`, type expectations).
//!
//! `ScopeContext` receives cloned/copied state from `AstBuildState` (e.g.
//! `Rc<TopLevelDeclarationIndex>` for top-level symbols, `Rc<ReceiverMethodCatalog>` for method
//! lookup, `HostRegistry` clone) so body parsing is self-contained without referencing the mutable
//! build state directly.

use crate::compiler_frontend::FrontendBuildProfile;
use crate::compiler_frontend::ast::ast_nodes::Declaration;
use crate::compiler_frontend::ast::statements::functions::FunctionSignature;
use crate::compiler_frontend::ast::templates::template_folding::TemplateFoldContext;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::compiler_warnings::CompilerWarning;
use crate::compiler_frontend::datatypes::{DataType, ReceiverKey};
use crate::compiler_frontend::host_functions::HostRegistry;
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

pub(super) static CONTROL_FLOW_SCOPE_COUNTER: AtomicUsize = AtomicUsize::new(0);

pub struct TopLevelDeclarationIndex {
    declarations: Box<[Declaration]>,
    by_name: FxHashMap<StringId, DeclarationBucket>,
}

enum DeclarationBucket {
    One(u32),
    Many(Box<[u32]>),
}

impl TopLevelDeclarationIndex {
    pub fn new(declarations: Vec<Declaration>) -> Self {
        let declarations: Box<[Declaration]> = declarations.into_boxed_slice();
        let mut temp: FxHashMap<StringId, Vec<u32>> = FxHashMap::default();

        for (index, declaration) in declarations.iter().enumerate() {
            let Some(name) = declaration.id.name() else {
                continue;
            };

            // Receiver methods already have their own catalog.
            if matches!(
                &declaration.value.data_type,
                DataType::Function(receiver, _) if receiver.as_ref().is_some()
            ) {
                continue;
            }

            temp.entry(name).or_default().push(index as u32);
        }

        let by_name = temp
            .into_iter()
            .map(|(name, indices)| {
                let bucket = match indices.as_slice() {
                    [one] => DeclarationBucket::One(*one),
                    _ => DeclarationBucket::Many(indices.into_boxed_slice()),
                };
                (name, bucket)
            })
            .collect();

        Self {
            declarations,
            by_name,
        }
    }

    pub fn declarations(&self) -> &[Declaration] {
        &self.declarations
    }

    pub fn get_visible(
        &self,
        name: StringId,
        visible: Option<&FxHashSet<InternedPath>>,
    ) -> Option<&Declaration> {
        let is_visible = |declaration: &Declaration| match visible {
            Some(visible) => visible.contains(&declaration.id),
            None => true,
        };

        match self.by_name.get(&name)? {
            DeclarationBucket::One(index) => {
                let declaration = &self.declarations[*index as usize];
                is_visible(declaration).then_some(declaration)
            }
            DeclarationBucket::Many(indices) => indices
                .iter()
                .rev()
                .map(|index| &self.declarations[*index as usize])
                .find(|declaration| is_visible(declaration)),
        }
    }
}

#[derive(Clone)]
/// Shared parser/lowering context for one active AST scope.
pub struct ScopeContext {
    // --- Core scope identity ---
    pub kind: ContextKind,
    pub scope: InternedPath,

    // --- Declaration tables ---
    // Shared module-wide top-level declaration store + name index.
    pub(crate) top_level_declarations: Rc<TopLevelDeclarationIndex>,
    // Per-scope locals: function parameters + body-declared variables only.
    // Grows incrementally in source order via add_var(); never carries module-wide data.
    pub local_declarations: Vec<Declaration>,
    // Optional file-local visibility gate over declarations.
    // When present, references must be in this set, which enforces import boundaries.
    pub visible_declaration_ids: Option<FxHashSet<InternedPath>>,

    // --- Type expectations ---
    pub expected_result_types: Vec<DataType>,
    pub expected_error_type: Option<DataType>,

    // --- External registries ---
    pub host_registry: HostRegistry,
    pub style_directives: StyleDirectiveRegistry,
    pub build_profile: FrontendBuildProfile,

    // --- Control flow state ---
    pub loop_depth: usize,

    // --- Side-channels (Rc-shared across clones) ---
    pub(crate) emitted_warnings: Rc<RefCell<Vec<CompilerWarning>>>,
    pub(crate) rendered_path_usages: Rc<RefCell<Vec<RenderedPathUsage>>>,

    // --- Path resolution (optional) ---
    /// Project-aware path resolver for compile-time path validation.
    pub(crate) project_path_resolver: Option<ProjectPathResolver>,
    /// The real filesystem source file that this context originated from.
    /// For const templates, `scope` is a synthetic path like `#page.bst/#const_template0`,
    /// so this field carries the actual source file path for path resolution.
    pub(crate) source_file_scope: Option<InternedPath>,
    /// Path formatting config for `#origin`-aware path string coercion.
    pub(crate) path_format_config: PathStringFormatConfig,

    // --- Method catalog ---
    pub(crate) receiver_methods: Rc<ReceiverMethodCatalog>,
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

impl ScopeContext {
    pub fn new(
        kind: ContextKind,
        scope: InternedPath,
        top_level_declarations: Rc<TopLevelDeclarationIndex>,
        host_registry: HostRegistry,
        expected_result_types: Vec<DataType>,
    ) -> ScopeContext {
        ScopeContext {
            kind,
            scope,
            top_level_declarations,
            local_declarations: Vec::new(),
            visible_declaration_ids: None,
            expected_result_types,
            expected_error_type: None,
            host_registry,
            style_directives: StyleDirectiveRegistry::built_ins(),
            loop_depth: 0,
            build_profile: FrontendBuildProfile::Dev,
            emitted_warnings: Rc::new(RefCell::new(Vec::new())),
            project_path_resolver: None,
            source_file_scope: None,
            path_format_config: PathStringFormatConfig::default(),
            rendered_path_usages: Rc::new(RefCell::new(Vec::new())),
            receiver_methods: Rc::new(ReceiverMethodCatalog::default()),
        }
    }

    pub fn new_child_control_flow(
        &self,
        kind: ContextKind,
        string_table: &mut StringTable,
    ) -> ScopeContext {
        let mut new_context = self.to_owned();
        new_context.kind = kind;
        if matches!(new_context.kind, ContextKind::Loop) {
            new_context.loop_depth += 1;
        }

        let scope_id = CONTROL_FLOW_SCOPE_COUNTER.fetch_add(1, Ordering::Relaxed);
        new_context.scope = self
            .scope
            .join_str(&format!("__scope_{scope_id}"), string_table);

        new_context
    }

    pub fn new_child_function(
        &self,
        id: StringId,
        signature: FunctionSignature,
        _string_table: &mut StringTable,
    ) -> ScopeContext {
        let mut new_context = self.to_owned();
        new_context.kind = ContextKind::Function;
        new_context.expected_result_types = signature.return_data_types();
        new_context.expected_error_type = signature
            .error_return()
            .map(|ret| ret.data_type().to_owned());

        // Create a new scope path by joining the current scope with the function name
        new_context.scope = self.scope.append(id);
        new_context.loop_depth = 0;

        // Share the top-level declaration table (cheap Rc clone); reset locals to params only.
        new_context.top_level_declarations = Rc::clone(&self.top_level_declarations);
        new_context.local_declarations = signature.parameters;

        new_context
    }

    pub fn new_child_expression(&self, expected_result_types: Vec<DataType>) -> ScopeContext {
        let mut new_context = self.to_owned();
        new_context.kind = ContextKind::Expression;
        new_context.expected_result_types = expected_result_types;
        new_context
    }

    /// Build the context used while parsing template expressions.
    ///
    /// Constant contexts stay constant so template-head captures can inline
    /// compile-time values. All other contexts parse templates as runtime-capable.
    pub fn new_template_parsing_context(&self) -> ScopeContext {
        let template_kind = if self.kind.is_constant_context() {
            self.kind.clone()
        } else {
            ContextKind::Template
        };

        ScopeContext {
            kind: template_kind,
            scope: self.scope.clone(),
            top_level_declarations: Rc::clone(&self.top_level_declarations),
            local_declarations: self.local_declarations.clone(),
            visible_declaration_ids: self.visible_declaration_ids.clone(),
            expected_result_types: vec![],
            expected_error_type: self.expected_error_type.clone(),
            host_registry: self.host_registry.clone(),
            style_directives: self.style_directives.clone(),
            loop_depth: self.loop_depth,
            build_profile: self.build_profile,
            emitted_warnings: self.emitted_warnings.clone(),
            project_path_resolver: self.project_path_resolver.clone(),
            source_file_scope: self.source_file_scope.clone(),
            path_format_config: self.path_format_config.clone(),
            rendered_path_usages: self.rendered_path_usages.clone(),
            receiver_methods: self.receiver_methods.clone(),
        }
    }

    /// Builds a constant child context that preserves project-aware folding/path state.
    ///
    /// WHAT: clones the parent visibility/declaration environment and forces
    ///       resolver + source file scope propagation into constant parsing paths.
    /// WHY: resolver-less constant contexts are invalid for template folding and
    ///      template-head path coercion.
    pub fn new_constant(scope: InternedPath, parent: &ScopeContext) -> ScopeContext {
        ScopeContext {
            kind: ContextKind::Constant,
            scope,
            top_level_declarations: Rc::clone(&parent.top_level_declarations),
            local_declarations: parent.local_declarations.clone(),
            visible_declaration_ids: parent.visible_declaration_ids.clone(),
            expected_result_types: Vec::new(),
            expected_error_type: parent.expected_error_type.clone(),
            host_registry: parent.host_registry.clone(),
            style_directives: parent.style_directives.clone(),
            loop_depth: parent.loop_depth,
            build_profile: parent.build_profile,
            emitted_warnings: parent.emitted_warnings.clone(),
            project_path_resolver: parent.project_path_resolver.clone(),
            source_file_scope: parent.source_file_scope.clone(),
            path_format_config: parent.path_format_config.clone(),
            rendered_path_usages: parent.rendered_path_usages.clone(),
            receiver_methods: parent.receiver_methods.clone(),
        }
    }

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
        let Some(source_scope) = self.source_file_scope.as_ref() else {
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

    pub fn with_build_profile(mut self, profile: FrontendBuildProfile) -> ScopeContext {
        self.build_profile = profile;
        self
    }

    pub fn with_visible_declarations(mut self, visible: FxHashSet<InternedPath>) -> ScopeContext {
        // A context without this gate can resolve any declaration in the module.
        // File/start contexts set this to enforce import semantics.
        self.visible_declaration_ids = Some(visible);
        self
    }

    pub fn with_style_directives(
        mut self,
        style_directives: &StyleDirectiveRegistry,
    ) -> ScopeContext {
        self.style_directives = style_directives.clone();
        self
    }

    pub(crate) fn with_project_path_resolver(
        mut self,
        resolver: Option<ProjectPathResolver>,
    ) -> ScopeContext {
        self.project_path_resolver = resolver;
        self
    }

    pub fn with_source_file_scope(mut self, source_file: InternedPath) -> ScopeContext {
        self.source_file_scope = Some(source_file);
        self
    }

    pub fn with_path_format_config(mut self, config: PathStringFormatConfig) -> ScopeContext {
        self.path_format_config = config;
        self
    }

    pub fn with_rendered_path_usage_sink(
        mut self,
        sink: Rc<RefCell<Vec<RenderedPathUsage>>>,
    ) -> ScopeContext {
        self.rendered_path_usages = sink;
        self
    }

    pub(crate) fn with_receiver_methods(
        mut self,
        receiver_methods: Rc<ReceiverMethodCatalog>,
    ) -> ScopeContext {
        self.receiver_methods = receiver_methods;
        self
    }

    pub(crate) fn set_local_declarations(&mut self, declarations: Vec<Declaration>) {
        self.local_declarations = declarations;
    }

    pub(crate) fn get_reference(&self, name: &StringId) -> Option<&Declaration> {
        self.local_declarations
            .iter()
            .rfind(|declaration| declaration.id.name() == Some(*name))
            .or_else(|| {
                self.top_level_declarations
                    .get_visible(*name, self.visible_declaration_ids.as_ref())
            })
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

    pub fn add_var(&mut self, arg: Declaration) {
        if let Some(visible_declarations) = self.visible_declaration_ids.as_mut() {
            visible_declarations.insert(arg.id.clone());
        }
        self.local_declarations.push(arg);
    }

    pub fn is_inside_loop(&self) -> bool {
        self.loop_depth > 0
    }

    pub fn emit_warning(&self, warning: CompilerWarning) {
        self.emitted_warnings.borrow_mut().push(warning);
    }

    pub fn take_emitted_warnings(&self) -> Vec<CompilerWarning> {
        std::mem::take(&mut *self.emitted_warnings.borrow_mut())
    }

    pub fn record_rendered_path_usages(&self, usages: Vec<RenderedPathUsage>) {
        self.rendered_path_usages.borrow_mut().extend(usages);
    }

    pub fn take_rendered_path_usages(&self) -> Vec<RenderedPathUsage> {
        std::mem::take(&mut *self.rendered_path_usages.borrow_mut())
    }
}
