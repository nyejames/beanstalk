//! Shared parser/lowering context for one active AST scope.
//!
//! WHAT: `ScopeContext` carries all state needed to parse/lower a single scope — declarations,
//! visibility gates, type expectations, and optional path-resolution capability.
//!
//! WHY: passing it as one struct avoids large parameter lists across recursive parsing calls,
//! and makes clone-to-child cheap without rebuilding immutable semantic lookup tables.
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
//! Semantic lookups are immutable after environment construction. Interior-mutable shared state is
//! limited to emission side channels: diagnostics/warnings, rendered path usages, and generic
//! function instantiation requests consumed by `AstEmitter`.
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
use crate::compiler_frontend::ast::generic_functions::{
    GenericFunctionInstanceKey, GenericFunctionInstantiationRequest,
};
use crate::compiler_frontend::ast::instrumentation::{
    AstCounter, add_ast_counter, increment_ast_counter,
};
use crate::compiler_frontend::ast::module_ast::environment::{
    AstModuleLookups, TopLevelDeclarationTable,
};
use crate::compiler_frontend::ast::statements::functions::FunctionSignature;
use crate::compiler_frontend::ast::templates::template_folding::TemplateFoldContext;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::compiler_messages::CompilerDiagnostic;
use crate::compiler_frontend::datatypes::definitions::TypeDefinition;
use crate::compiler_frontend::datatypes::environment::TypeEnvironment;
use crate::compiler_frontend::datatypes::generic_parameters::ActiveGenericTypeContext;
use crate::compiler_frontend::datatypes::ids::TypeId;
use crate::compiler_frontend::datatypes::{DataType, ReceiverKey};
use crate::compiler_frontend::declaration_syntax::choice::ChoiceVariant;
use crate::compiler_frontend::external_packages::{
    ExternalAbiType, ExternalConstantDef, ExternalConstantId, ExternalFunctionDef,
    ExternalFunctionId, ExternalPackageRegistry, ExternalSignatureType, ExternalSymbolId,
    ExternalTypeDef, ExternalTypeId,
};
use crate::compiler_frontend::headers::import_environment::{
    FileVisibility, HeaderImportEnvironment,
};
use crate::compiler_frontend::headers::module_symbols::{
    GenericDeclarationMetadata, ModuleSymbols,
};
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::paths::path_format::PathStringFormatConfig;
use crate::compiler_frontend::paths::path_resolution::ProjectPathResolver;
use crate::compiler_frontend::paths::rendered_path_usage::RenderedPathUsage;
use crate::compiler_frontend::style_directives::StyleDirectiveRegistry;
use crate::compiler_frontend::symbols::string_interning::{StringId, StringTable};
use crate::return_compiler_error;

use rustc_hash::{FxHashMap, FxHashSet};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};

pub(crate) use crate::compiler_frontend::ast::receiver_methods::{
    ReceiverMethodCatalog, ReceiverMethodEntry,
};

mod builders;
mod diagnostic_sinks;
mod local_declarations;
mod lookup;
mod required_services;

/// Checks whether an `ExternalSignatureType` is compatible with a frontend `TypeId`
/// for receiver method dispatch.
///
/// WHAT: boundary check between external package signature types and canonical frontend
///       type identity. Exact `External(type_id)` requires the same canonical external
///       type; `Abi(Handle)` preserves the old broad "any external type" matching.
/// WHY: receiver dispatch should not rebuild parse-only `DataType` just to
///      decide whether a visible external method applies, and package-scoped opaque
///      types must not collapse into indistinguishable handles.
fn external_signature_type_matches_type_id(
    signature_type: &ExternalSignatureType,
    type_id: TypeId,
    type_environment: &TypeEnvironment,
) -> bool {
    match signature_type {
        ExternalSignatureType::Abi(abi_type) => match abi_type {
            ExternalAbiType::Inferred => true,
            ExternalAbiType::I32 => type_id == type_environment.builtins().int,
            ExternalAbiType::F64 => type_id == type_environment.builtins().float,
            ExternalAbiType::Bool => type_id == type_environment.builtins().bool,
            ExternalAbiType::Utf8Str => type_id == type_environment.builtins().string,
            ExternalAbiType::Char => type_id == type_environment.builtins().char,
            ExternalAbiType::Handle => matches!(
                type_environment.get(type_id),
                Some(TypeDefinition::External(..))
            ),
            ExternalAbiType::Void => false,
        },
        ExternalSignatureType::BuiltinError => {
            // BuiltinError is not a valid receiver type.
            false
        }
        ExternalSignatureType::External(expected_external_id) => matches!(
            type_environment.get(type_id),
            Some(TypeDefinition::External(def)) if def.type_id == *expected_external_id
        ),
    }
}

/// Global counter for generating unique synthetic scope paths in child control-flow contexts.
pub(super) static CONTROL_FLOW_SCOPE_COUNTER: AtomicUsize = AtomicUsize::new(0);

/// Immutable shared state common to a scope and all its cloned children.
///
/// WHAT: bundles all state that is identical across child scopes so cloning a
/// `ScopeContext` only copies per-scope mutable fields and one `Rc` pointer.
/// WHY: eliminates deep cloning of visibility maps, registries, and lookup tables
/// every time a child control-flow or expression scope is created.
#[derive(Clone)]
pub struct ScopeShared {
    // Immutable semantic lookup tables.
    pub(crate) lookups: Rc<AstModuleLookups>,
    pub(crate) top_level_declarations: Rc<TopLevelDeclarationTable>,
    pub(crate) nominal_type_ids_by_path: Rc<FxHashMap<InternedPath, TypeId>>,

    // External package and frontend services.
    pub(crate) external_package_registry: ExternalPackageRegistry,
    pub(crate) style_directives: StyleDirectiveRegistry,
    pub(crate) build_profile: FrontendBuildProfile,

    // File-local visibility and resolved declarations.
    pub(crate) file_visibility: Option<Rc<FileVisibility>>,
    pub(crate) resolved_type_aliases: Option<Rc<FxHashMap<InternedPath, DataType>>>,
    pub(crate) generic_declarations_by_path:
        Option<Rc<FxHashMap<InternedPath, GenericDeclarationMetadata>>>,
    pub(crate) resolved_struct_fields_by_path:
        Option<Rc<FxHashMap<InternedPath, Vec<Declaration>>>>,
    pub(crate) choice_variant_shells_by_path:
        Option<Rc<FxHashMap<InternedPath, Vec<ChoiceVariant>>>>,

    // Emission side channels (diagnostics, path usages, generic instantiation requests).
    pub(crate) emitted_warnings: Rc<RefCell<Vec<CompilerDiagnostic>>>,
    pub(crate) rendered_path_usages: Rc<RefCell<Vec<RenderedPathUsage>>>,
    pub(crate) generic_function_instantiation_requests:
        Rc<RefCell<Vec<GenericFunctionInstantiationRequest>>>,

    // Path resolution and source identity.
    pub(crate) project_path_resolver: Option<ProjectPathResolver>,
    pub(crate) source_file_scope: Option<InternedPath>,
    pub(crate) path_format_config: PathStringFormatConfig,

    // Receiver method catalog for dispatch.
    pub(crate) receiver_methods: Rc<ReceiverMethodCatalog>,
}

/// Shared parser/lowering context for one active AST scope.
#[derive(Clone)]
pub struct ScopeContext {
    // Core scope identity.
    pub kind: ContextKind,
    pub scope: InternedPath,

    // Immutable shared services are cheap to clone into child scopes.
    pub(crate) shared: Rc<ScopeShared>,

    // Per-scope locals: function parameters + body-declared variables only.
    // Grows incrementally in source order via add_var(); never carries module-wide data.
    pub local_declarations: Vec<Declaration>,

    // Indexed local lookup: name → ordered indices into local_declarations.
    // Preserves "latest visible local wins" without reverse scanning the full vec.
    local_declarations_by_name: FxHashMap<StringId, Vec<u32>>,

    // Assignment targets are readable on the success side of an assignment expression, but not from
    // catch recovery subtrees attached to that assignment. The pending set is activated only when
    // the parser enters a `catch` handler body.
    unavailable_assignment_targets: FxHashSet<StringId>,
    pending_catch_assignment_targets: FxHashSet<StringId>,

    // Optional file-local visibility gate over declarations.
    // When present, references must be in this set, which enforces import boundaries.
    // Kept directly on ScopeContext (not in ScopeShared) because add_var mutates it.
    pub visible_declaration_ids: Option<FxHashSet<InternedPath>>,

    // Type expectations.
    pub expected_result_type_ids: Vec<TypeId>,
    pub expected_error_type: Option<TypeId>,

    /// Success return slots for the nearest enclosing function-like body.
    ///
    /// WHAT: unlike `expected_result_type_ids`, this remains stable through
    /// expression-local expected-type contexts such as call arguments.
    /// WHY: postfix option propagation returns from the current function, so it
    /// must validate against the function return contract rather than the
    /// immediate expression receiver.
    pub current_function_return_type_ids: Vec<TypeId>,

    /// Active value-production target for `then` statements in the current scope.
    ///
    /// WHAT: when present, `then` statements must produce values matching these types.
    /// WHY: one target shape lets catch, value `if`, and value match handlers
    /// share arity/coercion validation and HIR result-local lowering.
    pub active_value_target: Option<
        crate::compiler_frontend::ast::statements::value_production::ActiveValueProductionTarget,
    >,

    active_generic_type_context: Option<ActiveGenericTypeContext>,
    pub(crate) generic_template_validation: bool,
    pub(crate) generic_function_instantiation_stack: Vec<GenericFunctionInstanceKey>,

    // Control flow state.
    pub loop_depth: usize,
}

impl std::ops::Deref for ScopeContext {
    type Target = ScopeShared;

    fn deref(&self) -> &Self::Target {
        &self.shared
    }
}

/// High-level scope categories used by parser/lowering rules.
#[derive(Debug, PartialEq, Clone)]
pub enum ContextKind {
    /// The top-level scope of each file in the module.
    Module,

    Expression,

    /// An expression enforced to be evaluated at compile time;
    /// cannot contain non-constant references.
    Constant,

    /// Top-level compile-time constant declaration context (`name #= ...`).
    ConstantHeader,

    Function,

    /// For loops and if statements.
    Condition,

    Loop,
    Block,
    Branch,
    CatchHandler,

    /// Statement body of one `<pattern> =>` or `else =>` arm in a match block.
    MatchArm,

    Template,
}

impl ContextKind {
    pub fn is_constant_context(&self) -> bool {
        matches!(self, ContextKind::Constant | ContextKind::ConstantHeader)
    }

    pub fn allows_const_record_coercion(&self) -> bool {
        self.is_constant_context()
    }
}

impl ScopeContext {
    pub(crate) fn record_generic_function_instantiation_request(
        &self,
        request: GenericFunctionInstantiationRequest,
    ) {
        self.shared
            .generic_function_instantiation_requests
            .borrow_mut()
            .push(request);
    }

    pub(crate) fn is_generic_function_instantiation_active(
        &self,
        key: &GenericFunctionInstanceKey,
    ) -> bool {
        self.generic_function_instantiation_stack
            .iter()
            .any(|active_key| active_key == key)
    }

    pub(crate) fn active_generic_type_context(&self) -> Option<&ActiveGenericTypeContext> {
        self.active_generic_type_context.as_ref()
    }
}

/// Build an index mapping local declaration names to their positions in `local_declarations`.
///
/// WHAT: enables O(1) lookup of all locals with a given name, with the last
///       registered index representing the currently visible binding.
/// WHY: avoids reverse-scanning the full declaration vec on every name resolution.
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
    /// Build a context with the minimum synthetic lookup package needed before
    /// the completed AST environment is available.
    ///
    /// WHAT: seeds shared services with empty/default lookup tables plus the
    /// provided declaration table and external package registry.
    /// WHY: constant-header parsing runs while the environment is still being
    /// built, so it supplies visibility, aliases, and nominal type maps through
    /// builder setters. Body emission must replace these synthetic lookups with
    /// `with_lookups` before parsing function/start/template bodies.
    pub(crate) fn new(
        kind: ContextKind,
        scope: InternedPath,
        top_level_declarations: Rc<TopLevelDeclarationTable>,
        external_package_registry: ExternalPackageRegistry,
        expected_result_type_ids: Vec<TypeId>,
    ) -> ScopeContext {
        increment_ast_counter(AstCounter::ScopeContextsCreated);

        let lookups = Rc::new(AstModuleLookups {
            module_symbols: ModuleSymbols::empty(),
            import_environment: HeaderImportEnvironment::default(),
            warnings: Vec::new(),
            declaration_table: top_level_declarations,
            module_constants: Vec::new(),
            rendered_path_usages: Rc::new(RefCell::new(Vec::new())),
            builtin_struct_ast_nodes: Vec::new(),
            resolved_struct_fields_by_path: Rc::new(FxHashMap::default()),
            resolved_function_signatures_by_path: Rc::new(FxHashMap::default()),
            generic_function_templates_by_path: Rc::new(FxHashMap::default()),
            resolved_type_aliases_by_path: Rc::new(FxHashMap::default()),
            choice_variant_shells_by_path: Rc::new(FxHashMap::default()),
            receiver_methods: Rc::new(ReceiverMethodCatalog::default()),
            generic_declarations_by_path: Rc::new(FxHashMap::default()),
            nominal_type_ids_by_path: Rc::new(FxHashMap::default()),
            external_package_registry,
            style_directives: StyleDirectiveRegistry::built_ins(),
            build_profile: FrontendBuildProfile::Dev,
            project_path_resolver: None,
            path_format_config: PathStringFormatConfig::default(),
        });

        let shared = Rc::new(ScopeShared {
            lookups: Rc::clone(&lookups),
            top_level_declarations: Rc::clone(&lookups.declaration_table),
            external_package_registry: lookups.external_package_registry.clone(),
            style_directives: lookups.style_directives.clone(),
            build_profile: lookups.build_profile,
            file_visibility: None,
            resolved_type_aliases: None,
            generic_declarations_by_path: None,
            resolved_struct_fields_by_path: None,
            choice_variant_shells_by_path: None,
            emitted_warnings: Rc::new(RefCell::new(Vec::new())),
            rendered_path_usages: Rc::clone(&lookups.rendered_path_usages),
            generic_function_instantiation_requests: Rc::new(RefCell::new(Vec::new())),
            project_path_resolver: lookups.project_path_resolver.clone(),
            source_file_scope: None,
            path_format_config: lookups.path_format_config.clone(),
            receiver_methods: Rc::clone(&lookups.receiver_methods),
            nominal_type_ids_by_path: Rc::clone(&lookups.nominal_type_ids_by_path),
        });

        ScopeContext {
            kind,
            scope,
            shared,
            local_declarations: Vec::new(),
            local_declarations_by_name: FxHashMap::default(),
            unavailable_assignment_targets: FxHashSet::default(),
            pending_catch_assignment_targets: FxHashSet::default(),
            visible_declaration_ids: None,
            expected_result_type_ids,
            expected_error_type: None,
            current_function_return_type_ids: Vec::new(),
            active_value_target: None,
            active_generic_type_context: None,
            generic_template_validation: false,
            generic_function_instantiation_stack: Vec::new(),
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
        let active_value_target = if matches!(kind, ContextKind::Branch | ContextKind::MatchArm) {
            self.active_value_target.clone()
        } else {
            None
        };

        ScopeContext {
            kind,
            scope,
            shared: Rc::clone(&self.shared),
            local_declarations: self.local_declarations.clone(),
            local_declarations_by_name: self.local_declarations_by_name.clone(),
            unavailable_assignment_targets: self.unavailable_assignment_targets.clone(),
            pending_catch_assignment_targets: self.pending_catch_assignment_targets.clone(),
            visible_declaration_ids: self.visible_declaration_ids.clone(),
            expected_result_type_ids: self.expected_result_type_ids.clone(),
            expected_error_type: self.expected_error_type,
            current_function_return_type_ids: self.current_function_return_type_ids.clone(),
            // Branch-like child scopes inherit value production so ordinary nested
            // `if`/match paths can produce for the nearest active value block.
            // Barriers such as loops, functions, conditions, and templates keep
            // clearing the target by constructing non-branch child contexts.
            active_value_target,
            active_generic_type_context: self.active_generic_type_context.clone(),
            generic_template_validation: self.generic_template_validation,
            generic_function_instantiation_stack: self.generic_function_instantiation_stack.clone(),
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
        let expected_result_type_ids = signature.success_return_type_ids();
        let expected_error_type = signature.error_return_type_id();
        new_context.expected_result_type_ids = expected_result_type_ids;
        new_context.expected_error_type = expected_error_type;
        new_context.current_function_return_type_ids = new_context.expected_result_type_ids.clone();
        new_context.active_value_target = None;
        new_context.active_generic_type_context = None;
        new_context.generic_template_validation = false;

        // Create a new scope path by joining the current scope with the function name.
        new_context.scope = self.scope.append(function_name);
        new_context.loop_depth = 0;

        // Share the top-level declaration table (cheap Rc clone); reset locals to params only.
        new_context.set_local_declarations(signature.parameters);

        new_context
    }

    pub fn new_child_expression(&self, expected_result_type_ids: Vec<TypeId>) -> ScopeContext {
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
            unavailable_assignment_targets: self.unavailable_assignment_targets.clone(),
            pending_catch_assignment_targets: self.pending_catch_assignment_targets.clone(),
            visible_declaration_ids: self.visible_declaration_ids.clone(),
            expected_result_type_ids,
            expected_error_type: self.expected_error_type,
            current_function_return_type_ids: self.current_function_return_type_ids.clone(),
            active_value_target: None,
            active_generic_type_context: self.active_generic_type_context.clone(),
            generic_template_validation: self.generic_template_validation,
            generic_function_instantiation_stack: self.generic_function_instantiation_stack.clone(),
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
            unavailable_assignment_targets: self.unavailable_assignment_targets.clone(),
            pending_catch_assignment_targets: self.pending_catch_assignment_targets.clone(),
            visible_declaration_ids: self.visible_declaration_ids.clone(),
            expected_result_type_ids: vec![],
            expected_error_type: self.expected_error_type,
            current_function_return_type_ids: self.current_function_return_type_ids.clone(),
            active_value_target: None,
            active_generic_type_context: self.active_generic_type_context.clone(),
            generic_template_validation: self.generic_template_validation,
            generic_function_instantiation_stack: self.generic_function_instantiation_stack.clone(),
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
            unavailable_assignment_targets: parent.unavailable_assignment_targets.clone(),
            pending_catch_assignment_targets: parent.pending_catch_assignment_targets.clone(),
            visible_declaration_ids: parent.visible_declaration_ids.clone(),
            expected_result_type_ids: Vec::new(),
            expected_error_type: parent.expected_error_type,
            current_function_return_type_ids: parent.current_function_return_type_ids.clone(),
            active_value_target: None,
            active_generic_type_context: parent.active_generic_type_context.clone(),
            generic_template_validation: parent.generic_template_validation,
            generic_function_instantiation_stack: parent
                .generic_function_instantiation_stack
                .clone(),
            loop_depth: parent.loop_depth,
        }
    }
}
