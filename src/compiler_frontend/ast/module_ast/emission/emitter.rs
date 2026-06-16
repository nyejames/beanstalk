//! AST node emission.
//!
//! WHAT: iterates sorted headers with full context (resolved signatures, receiver catalog,
//! per-file visibility) and lowers each header into typed AST nodes.
//! WHY: emission is the only AST phase that parses executable bodies (function bodies, template
//! bodies, start body). Earlier phases consume header shells without body parsing.
//! Top-level declaration shell reparsing does NOT happen here — shells were fully parsed
//! by the header stage and resolved by environment construction.
//!
//! Constants and choices are handled in earlier passes; they do not emit nodes here.
//! Struct node emission reads the resolved field table produced by environment construction.

use crate::compiler_frontend::ast::ast_nodes::{AstNode, NodeKind, SourceLocation};
use crate::compiler_frontend::ast::function_body_to_ast;
use crate::compiler_frontend::ast::generic_functions::{
    GenericFunctionBodyValidationInput, GenericFunctionInstance, GenericFunctionInstanceKey,
    GenericFunctionInstantiationRequest, GenericInstantiationDiagnosticContext,
    concrete_argument_mapping, recursive_generic_function_instantiation,
    substitute_function_signature,
    validate_generic_function_body as validate_generic_body_template,
    with_generic_instantiation_context,
};
use crate::compiler_frontend::ast::module_ast::build_context::AstPhaseContext;
use crate::compiler_frontend::ast::module_ast::environment::AstModuleEnvironment;
use crate::compiler_frontend::ast::module_ast::environment::TopLevelDeclarationTable;
use crate::compiler_frontend::ast::module_ast::scope_context::{ContextKind, ScopeContext};
use crate::compiler_frontend::ast::statements::functions::{
    FunctionReturn, FunctionSignature, ReturnChannel, ReturnSlot,
};
use crate::compiler_frontend::ast::statements::terminality::{
    terminality_policy_for_signature, validate_function_body_terminality,
};
use crate::compiler_frontend::ast::templates::error::TemplateError;
use crate::compiler_frontend::ast::templates::template::TemplateConstValueKind;
use crate::compiler_frontend::ast::templates::template_types::Template;
use crate::compiler_frontend::ast::type_interner::AstTypeInterner;
use crate::compiler_frontend::compiler_errors::{CompilerError, CompilerMessages, ErrorType};
use crate::compiler_frontend::compiler_messages::{
    CompilerDiagnostic, GenericSubstitutionDiagnostic, InvalidTemplateStructureReason,
};

use crate::compiler_frontend::ast::type_resolution::resolve_diagnostic_type_to_type_id_checked;
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::datatypes::definitions::TypeDefinition;
use crate::compiler_frontend::datatypes::generic_parameters::{
    ActiveGenericTypeContext, GenericParameterScope,
};
use crate::compiler_frontend::datatypes::ids::{
    GenericParameterId, GenericParameterListId, TypeId,
};
use crate::compiler_frontend::headers::import_environment::FileVisibility;
use crate::compiler_frontend::headers::parse_file_headers::{Header, HeaderKind};
use crate::compiler_frontend::symbols::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::{StringId, StringTable};
use crate::compiler_frontend::tokenizer::tokens::FileTokens;
use crate::compiler_frontend::type_coercion::compatibility::TypeCompatibilityCache;
use crate::projects::settings::{self, IMPLICIT_START_FUNC_NAME};
use rustc_hash::FxHashMap;
use std::cell::RefCell;
use std::rc::Rc;

#[cfg(feature = "detailed_timers")]
use crate::compiler_frontend::compiler_messages::compiler_dev_logging::{
    detailed_timer_output_enabled, log_aggregated_duration,
};
#[cfg(feature = "detailed_timers")]
use std::time::Duration;
#[cfg(feature = "detailed_timers")]
use std::time::Instant;

pub(in crate::compiler_frontend::ast) struct AstEmission {
    /// Typed AST nodes emitted for this module (functions, structs, generic instances).
    pub(in crate::compiler_frontend::ast) ast: Vec<AstNode>,
    /// Warnings accumulated during emission (unused variables, deprecated uses, etc.).
    pub(in crate::compiler_frontend::ast) warnings: Vec<CompilerDiagnostic>,
    /// Compile-time template fragments keyed by the source file that declared them.
    pub(in crate::compiler_frontend::ast) const_templates_by_path:
        FxHashMap<InternedPath, StringId>,
    /// Concrete generic function instances emitted while lowering visible calls.
    pub(in crate::compiler_frontend::ast) generic_instance_count: usize,
}

/// Shared input used by [`AstEmitter::build_base_scope_context`] to create a
/// [`ScopeContext`] that is identical across function, start, and const-template emission.
struct BaseScopeContextInput<'scope> {
    kind: ContextKind,
    scope: InternedPath,
    top_level_declarations: &'scope Rc<TopLevelDeclarationTable>,
    visibility: Rc<FileVisibility>,
    source_file_scope: InternedPath,
}

/// Rebase each parameter's [`InternedPath`] from a bare name to a fully qualified path
/// under the given function path.
///
/// WHAT: ensures parameter symbols are module-unique before body parsing.
/// WHY: AST symbol IDs are full [`InternedPath`] values, not local-scope names.
fn rebase_signature_parameters(signature: &mut FunctionSignature, function_path: &InternedPath) {
    for parameter in &mut signature.parameters {
        let Some(parameter_name) = parameter.id.name() else {
            continue;
        };

        let old_parameter_id = parameter.id.clone();
        parameter.id = function_path.append(parameter_name);

        if let Some(source) = &mut parameter.value.reactive_source
            && source.path == old_parameter_id
        {
            source.path = parameter.id.clone();
        }

        if let Some(metadata) = &mut parameter.value.reactive_template {
            for dependency in &mut metadata.template_value_parameters {
                if dependency.parameter == old_parameter_id {
                    dependency.parameter = parameter.id.clone();
                }
            }
        }
    }
}

pub(in crate::compiler_frontend::ast) struct AstEmitter<'context, 'services, 'environment> {
    context: &'context AstPhaseContext<'services>,
    environment: &'environment mut AstModuleEnvironment,
    ast: Vec<AstNode>,
    warnings: Vec<CompilerDiagnostic>,
    const_templates_by_path: FxHashMap<InternedPath, StringId>,
    compatibility_cache: TypeCompatibilityCache,
    generic_function_instantiation_requests: Rc<RefCell<Vec<GenericFunctionInstantiationRequest>>>,
    generic_function_instances_by_key:
        FxHashMap<GenericFunctionInstanceKey, GenericFunctionInstance>,
}

impl<'context, 'services, 'environment> AstEmitter<'context, 'services, 'environment> {
    pub(in crate::compiler_frontend::ast) fn new(
        context: &'context AstPhaseContext<'services>,
        environment: &'environment mut AstModuleEnvironment,
        header_count: usize,
    ) -> Self {
        let warnings = environment.lookups.warnings.clone();
        Self {
            context,
            environment,
            ast: Vec::with_capacity(header_count * settings::TOKEN_TO_NODE_RATIO),
            warnings,
            const_templates_by_path: FxHashMap::default(),
            compatibility_cache: TypeCompatibilityCache::new(),
            generic_function_instantiation_requests: Rc::new(RefCell::new(Vec::new())),
            generic_function_instances_by_key: FxHashMap::default(),
        }
    }

    /// Emits AST nodes for each header kind (functions, structs, templates).
    /// Build a base `ScopeContext` with all shared state that is identical across function,
    /// start, and const-template emission.
    ///
    /// WHAT: centralizes the repeated 11-method `ScopeContext` builder chain so each emission
    /// arm only adds emission-specific configuration (parameters for functions, etc.).
    /// WHY: avoids duplicating the same visibility/alias/field/setup sequence in three match arms.
    fn build_base_scope_context(&self, input: BaseScopeContextInput<'_>) -> ScopeContext {
        ScopeContext::new(
            input.kind,
            input.scope,
            Rc::clone(input.top_level_declarations),
            self.context.external_package_registry.clone(),
            Vec::<TypeId>::new(),
        )
        .with_style_directives(self.context.style_directives)
        .with_build_profile(self.context.build_profile)
        .with_file_visibility(input.visibility)
        .with_resolved_type_aliases(Rc::clone(
            &self.environment.lookups.resolved_type_aliases_by_path,
        ))
        .with_generic_declarations(Rc::clone(
            &self.environment.lookups.generic_declarations_by_path,
        ))
        .with_resolved_struct_fields_by_path(Rc::clone(
            &self.environment.lookups.resolved_struct_fields_by_path,
        ))
        .with_project_path_resolver(self.context.project_path_resolver.clone())
        .with_path_format_config(self.context.path_format_config.clone())
        .with_template_const_loop_iteration_limit(self.context.template_const_loop_iteration_limit)
        .with_rendered_path_usage_sink(Rc::clone(&self.environment.lookups.rendered_path_usages))
        .with_generic_function_instantiation_sink(Rc::clone(
            &self.generic_function_instantiation_requests,
        ))
        .with_receiver_methods(Rc::clone(&self.environment.lookups.receiver_methods))
        .with_lookups(Rc::clone(&self.environment.lookups))
        .with_source_file_scope(input.source_file_scope)
    }

    pub(in crate::compiler_frontend::ast) fn emit(
        mut self,
        sorted_headers: Vec<Header>,
        string_table: &mut StringTable,
    ) -> Result<AstEmission, CompilerMessages> {
        // The environment owns the single resolved declaration table. Body contexts clone only
        // the Rc pointer so declaration metadata is not rebuilt during emission.
        let top_level_declarations = Rc::clone(&self.environment.lookups.declaration_table);

        #[cfg(feature = "detailed_timers")]
        let mut total_function_body_parse_time = Duration::default();
        #[cfg(feature = "detailed_timers")]
        let mut total_start_body_parse_time = Duration::default();
        #[cfg(feature = "detailed_timers")]
        let mut total_const_template_parse_time = Duration::default();
        #[cfg(feature = "detailed_timers")]
        let mut total_const_template_fold_time = Duration::default();
        #[cfg(feature = "detailed_timers")]
        let mut function_headers_emitted = 0usize;
        #[cfg(feature = "detailed_timers")]
        let mut start_headers_emitted = 0usize;
        #[cfg(feature = "detailed_timers")]
        let mut struct_headers_emitted = 0usize;
        #[cfg(feature = "detailed_timers")]
        let mut const_templates_emitted = 0usize;

        for header in sorted_headers {
            let visibility = Rc::new(
                self.environment
                    .lookups
                    .import_environment
                    .visibility_for(&header.source_file)
                    .map_err(|error| self.error_messages(error, string_table))?
                    .clone(),
            );
            let source_file_scope = header.canonical_source_file(string_table);

            match &header.kind {
                HeaderKind::Function {
                    generic_parameters, ..
                } => {
                    if !generic_parameters.is_empty() {
                        self.validate_generic_function_body(
                            header,
                            visibility,
                            source_file_scope,
                            string_table,
                        )?;
                        continue;
                    }

                    #[cfg(feature = "detailed_timers")]
                    let start = Instant::now();
                    self.emit_function(header, visibility, source_file_scope, string_table)?;
                    #[cfg(feature = "detailed_timers")]
                    {
                        total_function_body_parse_time += start.elapsed();
                        function_headers_emitted += 1;
                    }
                }

                HeaderKind::StartFunction => {
                    #[cfg(feature = "detailed_timers")]
                    let start = Instant::now();
                    self.emit_start(header, visibility, source_file_scope, string_table)?;
                    #[cfg(feature = "detailed_timers")]
                    {
                        total_start_body_parse_time += start.elapsed();
                        start_headers_emitted += 1;
                    }
                }

                HeaderKind::Struct {
                    generic_parameters, ..
                } => {
                    if !generic_parameters.is_empty() {
                        continue;
                    }

                    #[cfg(feature = "detailed_timers")]
                    {
                        struct_headers_emitted += 1;
                    }
                    self.emit_struct(header, string_table)?;
                }

                // Constants and choices are fully handled during environment construction.
                HeaderKind::Constant { .. } | HeaderKind::Choice { .. } => {}

                HeaderKind::ConstTemplate { .. } => {
                    let mut template_tokens = header.tokens;
                    let context = self.build_base_scope_context(BaseScopeContextInput {
                        kind: ContextKind::Constant,
                        scope: template_tokens.src_path.to_owned(),
                        top_level_declarations: &top_level_declarations,
                        visibility,
                        source_file_scope,
                    });

                    #[cfg(feature = "detailed_timers")]
                    let const_template_parse_start = Instant::now();
                    let template =
                        self.parse_const_template(&mut template_tokens, &context, string_table)?;
                    #[cfg(feature = "detailed_timers")]
                    {
                        total_const_template_parse_time += const_template_parse_start.elapsed();
                    }
                    self.warnings.extend(context.take_emitted_warnings());

                    #[cfg(feature = "detailed_timers")]
                    let const_template_fold_start = Instant::now();
                    let html = self.fold_const_template(template, &context, string_table)?;
                    #[cfg(feature = "detailed_timers")]
                    {
                        total_const_template_fold_time += const_template_fold_start.elapsed();
                        const_templates_emitted += 1;
                    }

                    self.const_templates_by_path
                        .insert(template_tokens.src_path, html);
                }

                // --------------------------
                //  Type aliases (no runtime emission)
                // --------------------------
                HeaderKind::TypeAlias { .. } => {
                    // Type aliases are compile-time-only metadata; they do not emit runtime nodes.
                }

                HeaderKind::Trait { .. }
                | HeaderKind::TraitConformance { .. }
                | HeaderKind::TraitIncompatibility { .. } => {
                    // Trait metadata is compile-time-only. AST environment construction has
                    // already resolved trait identities, evidence, and incompatibility relations
                    // before body emission.
                }
            }
        }

        self.emit_requested_generic_function_instances(Vec::new(), string_table)?;

        #[cfg(feature = "detailed_timers")]
        {
            log_aggregated_duration(
                "AST/node emission/function bodies parsed in: ",
                total_function_body_parse_time,
            );
            log_aggregated_duration(
                "AST/node emission/start bodies parsed in: ",
                total_start_body_parse_time,
            );
            log_aggregated_duration(
                "AST/node emission/const templates parsed in: ",
                total_const_template_parse_time,
            );
            log_aggregated_duration(
                "AST/node emission/const templates folded in: ",
                total_const_template_fold_time,
            );
            if detailed_timer_output_enabled() {
                saying::say!(
                    "AST/node emission/headers emitted: \n functions = ", Dark Green function_headers_emitted,
                    Reset "\n starts = ", Dark Green start_headers_emitted,
                    Reset "\n structs = ", Dark Green struct_headers_emitted,
                    Reset "\n const templates = ", Dark Green const_templates_emitted
                );
            }
        }

        Ok(AstEmission {
            ast: self.ast,
            warnings: self.warnings,
            const_templates_by_path: self.const_templates_by_path,
            generic_instance_count: self.generic_function_instances_by_key.len(),
        })
    }

    // --------------------------
    //  Emit function bodies
    // --------------------------

    /// Drains pending generic-instantiation requests and emits each one.
    ///
    /// WHAT: repeated in a loop because emitting one instance may queue further nested instances.
    /// WHY: generic function bodies can call other generic functions, so one pass is insufficient.
    fn emit_requested_generic_function_instances(
        &mut self,
        active_stack: Vec<GenericFunctionInstanceKey>,
        string_table: &mut StringTable,
    ) -> Result<(), CompilerMessages> {
        loop {
            let requests = {
                let mut pending = self.generic_function_instantiation_requests.borrow_mut();
                if pending.is_empty() {
                    break;
                }

                std::mem::take(&mut *pending)
            };

            for request in requests {
                self.emit_generic_function_instance(request, &active_stack, string_table)?;
            }
        }

        Ok(())
    }

    /// Rebuilds the body-local generic type context from canonical type metadata.
    ///
    /// WHAT: exposes generic parameter names while parsing a generic function
    /// body and optionally supplies concrete substitutions for an emitted
    /// instance.
    /// WHY: signature resolution owns canonical parameter allocation; body
    /// parsing must consume that canonical identity instead of reconstructing
    /// parser-local parameter IDs.
    fn build_active_generic_type_context(
        &self,
        parameter_list_id: GenericParameterListId,
        substitutions: Option<FxHashMap<GenericParameterId, TypeId>>,
        source_parameter_by_rebased_path: FxHashMap<InternedPath, GenericParameterId>,
        string_table: &StringTable,
    ) -> Result<ActiveGenericTypeContext, CompilerMessages> {
        let Some(parameter_list) = self
            .environment
            .type_environment
            .generic_parameters(parameter_list_id)
        else {
            return Err(self.error_messages(
                CompilerError::compiler_error(
                    "Generic function body requested an unknown generic parameter list.",
                ),
                string_table,
            ));
        };

        Ok(ActiveGenericTypeContext {
            parameter_scope: GenericParameterScope::from_canonical_parameter_list(parameter_list),
            substitutions,
            source_parameter_by_rebased_path,
        })
    }

    fn source_parameter_origins_for_signature(
        &self,
        source_signature: &FunctionSignature,
        emitted_signature: &FunctionSignature,
    ) -> FxHashMap<InternedPath, GenericParameterId> {
        let mut origins = FxHashMap::default();

        for (source_parameter, emitted_parameter) in source_signature
            .parameters
            .iter()
            .zip(emitted_signature.parameters.iter())
        {
            let Some(TypeDefinition::GenericParameter(parameter)) = self
                .environment
                .type_environment
                .get(source_parameter.value.type_id)
            else {
                continue;
            };

            origins.insert(emitted_parameter.id.clone(), parameter.id);
        }

        origins
    }

    fn generic_substitution_diagnostics(
        &self,
        parameter_list_id: GenericParameterListId,
        type_arguments: &[TypeId],
    ) -> Vec<GenericSubstitutionDiagnostic> {
        let Some(parameter_list) = self
            .environment
            .type_environment
            .generic_parameters(parameter_list_id)
        else {
            return Vec::new();
        };

        parameter_list
            .parameters
            .iter()
            .zip(type_arguments.iter())
            .map(
                |(parameter, concrete_type_id)| GenericSubstitutionDiagnostic {
                    parameter_name: parameter.name,
                    concrete_type_id: *concrete_type_id,
                },
            )
            .collect()
    }

    fn emit_generic_function_instance(
        &mut self,
        request: GenericFunctionInstantiationRequest,
        active_stack: &[GenericFunctionInstanceKey],
        string_table: &mut StringTable,
    ) -> Result<(), CompilerMessages> {
        // --------------------------
        //  Deduplication and recursion guard
        // --------------------------
        if let Some(existing_instance) = self.generic_function_instances_by_key.get(&request.key) {
            debug_assert_eq!(existing_instance.key, request.key);
            debug_assert_eq!(existing_instance.instance_path, request.instance_path);
            return Ok(());
        }

        if active_stack
            .iter()
            .any(|active_key| active_key == &request.key)
        {
            return Err(self.diagnostic_messages(
                recursive_generic_function_instantiation(
                    request.key.function_path.name(),
                    request.call_location,
                ),
                string_table,
            ));
        }

        // --------------------------
        //  Resolve template and substitute signature
        // --------------------------
        let Some(template) = self
            .environment
            .lookups
            .generic_function_templates_by_path
            .get(&request.key.function_path)
            .cloned()
        else {
            return Err(self.error_messages(
                CompilerError::compiler_error(
                    "Generic function instance requested for an unknown template.",
                ),
                string_table,
            ));
        };

        let Some(mapping) = concrete_argument_mapping(
            template.generic_parameter_list_id,
            request.key.type_arguments.as_ref(),
            &self.environment.type_environment,
        ) else {
            return Err(self.error_messages(
                CompilerError::compiler_error(
                    "Generic function instance request did not match its template parameter list.",
                ),
                string_table,
            ));
        };
        let substitution_diagnostics = self.generic_substitution_diagnostics(
            template.generic_parameter_list_id,
            request.key.type_arguments.as_ref(),
        );

        let mut signature = substitute_function_signature(
            &template.signature,
            &mapping,
            &mut self.environment.type_environment,
        );
        rebase_signature_parameters(&mut signature, &request.instance_path);
        let generic_type_context = self.build_active_generic_type_context(
            template.generic_parameter_list_id,
            Some(mapping),
            self.source_parameter_origins_for_signature(&template.signature, &signature),
            string_table,
        )?;

        // --------------------------
        //  Build body parsing context
        // --------------------------
        let visibility = Rc::new(
            self.environment
                .lookups
                .import_environment
                .visibility_for(&template.source_file)
                .map_err(|error| self.error_messages(error, string_table))?
                .clone(),
        );
        let mut visible_declarations = visibility.visible_declaration_paths.clone();
        for parameter in &signature.parameters {
            visible_declarations.insert(parameter.id.to_owned());
        }

        let mut active_instance_stack = active_stack.to_vec();
        active_instance_stack.push(request.key.clone());

        let mut context = self
            .build_base_scope_context(BaseScopeContextInput {
                kind: ContextKind::Function,
                scope: request.instance_path.clone(),
                top_level_declarations: &Rc::clone(&self.environment.lookups.declaration_table),
                visibility,
                source_file_scope: template.source_file.clone(),
            })
            .with_visible_declarations(visible_declarations)
            .with_active_generic_type_context(generic_type_context)
            .with_generic_function_instantiation_stack(active_instance_stack.clone());
        context.expected_result_type_ids = signature.success_return_type_ids();
        context.expected_error_type = signature.error_return_type_id();
        context.current_function_return_type_ids = context.expected_result_type_ids.clone();
        context.set_local_declarations(signature.parameters.to_owned());

        // --------------------------
        //  Parse body and materialize nested instances
        // --------------------------
        let mut token_stream = template.body_tokens.to_owned();
        token_stream.src_path = request.instance_path.clone();
        let mut type_interner = AstTypeInterner::new(
            &mut self.environment.type_environment,
            &mut self.compatibility_cache,
        );
        let body = match function_body_to_ast(
            &mut token_stream,
            context,
            &mut type_interner,
            &mut self.warnings,
            string_table,
        ) {
            Ok(body) => body,
            Err(diagnostic) => {
                let diagnostic = with_generic_instantiation_context(
                    diagnostic,
                    GenericInstantiationDiagnosticContext {
                        call_location: request.call_location.clone(),
                        declaration_location: template.declaration_location.clone(),
                        substitutions: substitution_diagnostics,
                    },
                );
                return Err(self.diagnostic_messages(diagnostic, string_table));
            }
        };

        // Template validation already proved terminality, so a failure during concrete instance
        // reparse is an internal compiler invariant failure rather than a user-facing diagnostic.
        let policy = terminality_policy_for_signature(&signature, false);
        if let Some(diagnostic) =
            validate_function_body_terminality(&body, policy, template.declaration_location.clone())
        {
            return Err(self.error_messages(
                CompilerError::new(
                    format!(
                        "Generic function instance {} failed terminality validation after template validation succeeded",
                        request.instance_path.to_string(string_table)
                    ),
                    diagnostic.primary_location,
                    ErrorType::Compiler,
                ),
                string_table,
            ));
        }

        // Materialize nested instances before marking this one complete so direct or indirect
        // recursive generic instantiation is diagnosed while the active stack is still visible.
        self.emit_requested_generic_function_instances(active_instance_stack, string_table)?;

        // --------------------------
        //  Register instance and emit AST node
        // --------------------------
        self.generic_function_instances_by_key.insert(
            request.key.clone(),
            GenericFunctionInstance {
                instance_path: request.instance_path.clone(),
                key: request.key,
            },
        );
        self.ast.push(AstNode {
            kind: NodeKind::Function(request.instance_path.clone(), signature, body),
            location: template.declaration_location,
            scope: request.instance_path,
        });

        Ok(())
    }

    fn validate_generic_function_body(
        &mut self,
        header: Header,
        visibility: Rc<FileVisibility>,
        source_file_scope: InternedPath,
        string_table: &mut StringTable,
    ) -> Result<(), CompilerMessages> {
        // --------------------------
        //  Retrieve resolved signature and template
        // --------------------------
        let Some(resolved_signature) = self
            .environment
            .lookups
            .resolved_function_signatures_by_path
            .get(&header.tokens.src_path)
            .cloned()
        else {
            return Err(self.error_messages(
                CompilerError::compiler_error(
                    "Generic function signature was not resolved before body validation.",
                ),
                string_table,
            ));
        };

        let Some(template) = self
            .environment
            .lookups
            .generic_function_templates_by_path
            .get(&header.tokens.src_path)
            .cloned()
        else {
            return Err(self.error_messages(
                CompilerError::compiler_error(
                    "Generic function template was not stored before body validation.",
                ),
                string_table,
            ));
        };

        // --------------------------
        //  Build validation context and run check
        // --------------------------
        let mut visible_declarations = visibility.visible_declaration_paths.clone();
        for parameter in &resolved_signature.signature.parameters {
            visible_declarations.insert(parameter.id.to_owned());
        }

        let mut context = self
            .build_base_scope_context(BaseScopeContextInput {
                kind: ContextKind::Function,
                scope: header.tokens.src_path.to_owned(),
                top_level_declarations: &Rc::clone(&self.environment.lookups.declaration_table),
                visibility,
                source_file_scope,
            })
            .with_visible_declarations(visible_declarations);
        let generic_type_context = self.build_active_generic_type_context(
            template.generic_parameter_list_id,
            None,
            FxHashMap::default(),
            string_table,
        )?;
        context = context.with_active_generic_type_context(generic_type_context);
        context.expected_result_type_ids = resolved_signature.signature.success_return_type_ids();
        context.expected_error_type = resolved_signature.signature.error_return_type_id();
        context.current_function_return_type_ids = context.expected_result_type_ids.clone();
        context.set_local_declarations(resolved_signature.signature.parameters);

        let mut type_interner = AstTypeInterner::new(
            &mut self.environment.type_environment,
            &mut self.compatibility_cache,
        );
        validate_generic_body_template(GenericFunctionBodyValidationInput {
            template: &template,
            context,
            type_interner: &mut type_interner,
            warnings: &mut self.warnings,
            string_table,
        })
        .map_err(|diagnostic| self.diagnostic_messages(diagnostic, string_table))
    }

    fn emit_function(
        &mut self,
        header: Header,
        visibility: Rc<FileVisibility>,
        source_file_scope: InternedPath,
        string_table: &mut StringTable,
    ) -> Result<(), CompilerMessages> {
        // --------------------------
        //  Resolve signature
        // --------------------------
        let Some(resolved_signature) = self
            .environment
            .lookups
            .resolved_function_signatures_by_path
            .get(&header.tokens.src_path)
            .cloned()
        else {
            return Err(self.error_messages(
                CompilerError::compiler_error(
                    "Function signature was not resolved before AST emission.",
                ),
                string_table,
            ));
        };

        // --------------------------
        //  Build body parsing context
        // --------------------------
        let mut visible_declarations = visibility.visible_declaration_paths.clone();
        for parameter in &resolved_signature.signature.parameters {
            visible_declarations.insert(parameter.id.to_owned());
        }

        // Top-level declarations are shared via Rc (no data copy);
        // parameters live in local_declarations.
        let mut context = self
            .build_base_scope_context(BaseScopeContextInput {
                kind: ContextKind::Function,
                scope: header.tokens.src_path.to_owned(),
                top_level_declarations: &Rc::clone(&self.environment.lookups.declaration_table),
                visibility,
                source_file_scope,
            })
            .with_visible_declarations(visible_declarations);
        let expected_result_type_ids = resolved_signature.signature.success_return_type_ids();
        let expected_error_type = resolved_signature.signature.error_return_type_id();
        context.expected_result_type_ids = expected_result_type_ids;
        context.expected_error_type = expected_error_type;
        context.current_function_return_type_ids = context.expected_result_type_ids.clone();
        context.set_local_declarations(resolved_signature.signature.parameters.to_owned());

        // --------------------------
        //  Parse body and emit node
        // --------------------------
        let mut token_stream = header.tokens;
        let function_scope = context.scope.clone();

        let mut type_interner = AstTypeInterner::new(
            &mut self.environment.type_environment,
            &mut self.compatibility_cache,
        );
        let body_result = function_body_to_ast(
            &mut token_stream,
            context,
            &mut type_interner,
            &mut self.warnings,
            string_table,
        );

        let body = body_result.map_err(|error| self.diagnostic_messages(error, string_table))?;

        self.validate_body_terminality(
            &body,
            &resolved_signature.signature,
            false,
            header.name_location.clone(),
            string_table,
        )?;

        // AST symbol IDs are stored as full InternedPath values and are unique
        // module-wide, not only within a local scope.
        self.ast.push(AstNode {
            kind: NodeKind::Function(token_stream.src_path, resolved_signature.signature, body),
            location: header.name_location,
            scope: function_scope,
        });

        Ok(())
    }

    // --------------------------
    //  Emit start function bodies
    // --------------------------

    fn emit_start(
        &mut self,
        header: Header,
        visibility: Rc<FileVisibility>,
        source_file_scope: InternedPath,
        string_table: &mut StringTable,
    ) -> Result<(), CompilerMessages> {
        // --------------------------
        //  Build context and parse body
        // --------------------------
        let context = self.build_base_scope_context(BaseScopeContextInput {
            kind: ContextKind::Module,
            scope: header.tokens.src_path.to_owned(),
            top_level_declarations: &Rc::clone(&self.environment.lookups.declaration_table),
            visibility,
            source_file_scope,
        });

        let mut token_stream = header.tokens;
        let start_scope = context.scope.clone();

        let mut type_interner = AstTypeInterner::new(
            &mut self.environment.type_environment,
            &mut self.compatibility_cache,
        );
        let body_result = function_body_to_ast(
            &mut token_stream,
            context,
            &mut type_interner,
            &mut self.warnings,
            string_table,
        );

        let body = body_result.map_err(|error| self.diagnostic_messages(error, string_table))?;

        // --------------------------
        //  Synthesize implicit start signature and emit node
        // --------------------------
        let full_name = token_stream
            .src_path
            .join_str(IMPLICIT_START_FUNC_NAME, string_table);

        // WHAT: entry start() returns Collection(StringSlice, MutableOwned),
        //       which is the Beanstalk frontend type for Vec<String>.
        // WHY: compiler-design-overview.md describes the return type as Vec<String>;
        //      DataType::Collection(StringSlice) is the same contract
        //      expressed in frontend DataType terms. The HIR builder adds the implicit
        //      return of the accumulated fragment vec at function end.
        let start_return_type = DataType::collection(DataType::StringSlice);
        let start_return_type_id = resolve_diagnostic_type_to_type_id_checked(
            &start_return_type,
            &mut self.environment.type_environment,
            &header.name_location,
        )
        .map_err(|diagnostic| self.diagnostic_messages(*diagnostic, string_table))?;
        let start_signature = FunctionSignature {
            parameters: vec![],
            returns: vec![ReturnSlot {
                value: FunctionReturn::Value(start_return_type),
                type_id: Some(start_return_type_id),
                reactive_template: None,
                channel: ReturnChannel::Success,
            }],
        };

        self.validate_body_terminality(
            &body,
            &start_signature,
            true,
            header.name_location.clone(),
            string_table,
        )?;

        self.ast.push(AstNode {
            kind: NodeKind::Function(full_name, start_signature, body),
            location: header.name_location,
            scope: start_scope,
        });

        Ok(())
    }

    // --------------------------
    //  Emit struct definitions
    // --------------------------

    fn emit_struct(
        &mut self,
        header: Header,
        string_table: &mut StringTable,
    ) -> Result<(), CompilerMessages> {
        let fields = self
            .environment
            .lookups
            .resolved_struct_fields_by_path
            .get(&header.tokens.src_path)
            .cloned()
            .ok_or_else(|| {
                self.error_messages(
                    CompilerError::compiler_error(
                        "Struct fields were not resolved before AST emission.",
                    ),
                    string_table,
                )
            })?;

        self.ast.push(AstNode {
            kind: NodeKind::StructDefinition(header.tokens.src_path.to_owned(), fields),
            location: header.name_location,
            scope: header.tokens.src_path,
        });

        Ok(())
    }

    // --------------------------
    //  Const template helpers
    // --------------------------

    fn parse_const_template(
        &mut self,
        template_tokens: &mut FileTokens,
        context: &ScopeContext,
        string_table: &mut StringTable,
    ) -> Result<Template, CompilerMessages> {
        let mut type_interner = AstTypeInterner::new(
            &mut self.environment.type_environment,
            &mut self.compatibility_cache,
        );
        let template_result = Template::new_const_required_with_type_interner(
            template_tokens,
            context,
            &mut type_interner,
            vec![],
            string_table,
        );

        let template =
            template_result.map_err(|error| self.diagnostic_messages(error, string_table))?;

        match template.const_value_kind() {
            // WHAT: top-level const templates can be direct strings or wrapper
            // templates with optional, unfilled slots.
            // WHY: unfilled slots are rendered as empty strings at compile time.
            // Nested helper-owned contribution structure is legal while composing a
            // wrapper, but the final top-level const value itself cannot be a raw
            // `$insert(...)` helper artifact.
            TemplateConstValueKind::RenderableString | TemplateConstValueKind::WrapperTemplate => {}
            TemplateConstValueKind::SlotInsertHelper => {
                return Err(self.diagnostic_messages(
                    CompilerDiagnostic::invalid_template_structure(
                        InvalidTemplateStructureReason::HelperInConstTemplate,
                        template.location,
                    ),
                    string_table,
                ));
            }
            TemplateConstValueKind::NonConst => {
                return Err(self.diagnostic_messages(
                    CompilerDiagnostic::invalid_template_structure(
                        InvalidTemplateStructureReason::NonFoldableConstTemplate,
                        template.location,
                    ),
                    string_table,
                ));
            }
        }

        Ok(template)
    }

    fn fold_const_template(
        &mut self,
        template: Template,
        context: &ScopeContext,
        string_table: &mut StringTable,
    ) -> Result<StringId, CompilerMessages> {
        let mut fold_context = match context
            .new_template_fold_context(string_table, "top-level const template folding")
        {
            Ok(ctx) => ctx,
            Err(error) => {
                return Err(self.error_messages(error, string_table));
            }
        };

        template
            .fold_into_stringid(&mut fold_context)
            .map_err(|error| self.template_error_messages(error, string_table))
    }

    /// Wraps an internal [`CompilerError`] into [`CompilerMessages`], preserving current
    /// warnings and the module type environment for render-time type-name resolution.
    fn error_messages(&self, error: CompilerError, string_table: &StringTable) -> CompilerMessages {
        CompilerMessages::from_error_with_warnings(error, self.warnings.clone(), string_table)
            .with_type_context_for_all_diagnostics(self.environment.type_environment.clone())
    }

    /// Wraps a user-facing [`CompilerDiagnostic`] into [`CompilerMessages`], preserving current
    /// warnings and the module type environment for render-time type-name resolution.
    fn diagnostic_messages(
        &self,
        diagnostic: CompilerDiagnostic,
        string_table: &StringTable,
    ) -> CompilerMessages {
        CompilerMessages::from_diagnostic_with_warnings(
            diagnostic,
            self.warnings.clone(),
            string_table,
        )
        .with_type_context_for_all_diagnostics(self.environment.type_environment.clone())
    }

    /// Converts a [`TemplateError`] (which may be user-facing or infrastructure) into the
    /// appropriate [`CompilerMessages`] wrapper.
    fn template_error_messages(
        &self,
        error: TemplateError,
        string_table: &StringTable,
    ) -> CompilerMessages {
        match error {
            TemplateError::Diagnostic(diagnostic) => {
                self.diagnostic_messages(*diagnostic, string_table)
            }
            TemplateError::Infrastructure(error) => self.error_messages(*error, string_table),
        }
    }

    /// Runs AST-owned terminality validation for a parsed function body.
    ///
    /// WHAT: converts the optional `FunctionMayFallThrough` diagnostic into the standard
    /// `CompilerMessages` wrapper used by this emitter.
    /// WHY: body parsing is complete at this point; missing-return diagnostics belong to AST,
    /// not to HIR lowering.
    fn validate_body_terminality(
        &self,
        body: &[AstNode],
        signature: &FunctionSignature,
        is_entry_start: bool,
        location: SourceLocation,
        string_table: &StringTable,
    ) -> Result<(), CompilerMessages> {
        let policy = terminality_policy_for_signature(signature, is_entry_start);
        if let Some(diagnostic) = validate_function_body_terminality(body, policy, location) {
            return Err(self.diagnostic_messages(diagnostic, string_table));
        }

        Ok(())
    }
}
