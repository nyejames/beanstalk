//! Function signature resolution and receiver method catalog construction.
//!
//! WHAT: resolves function parameter/return types after nominal declarations are registered,
//! then builds an indexed receiver-method catalog from the resolved signatures.
//! WHY: late resolution lets signatures use named struct types and receiver syntax
//! without adding a second nominal-type system just for headers.

use super::builder::AstModuleEnvironmentBuilder;

use crate::compiler_frontend::ast::generic_functions::GenericFunctionTemplate;
use crate::compiler_frontend::ast::module_ast::scope_context::{
    ContextKind, ReceiverMethodCatalog, ScopeContext,
};
use crate::compiler_frontend::ast::receiver_methods::{
    BuildReceiverMethodCatalogInput, ReceiverMethodCatalogError, ReceiverMethodKind,
    build_receiver_method_catalog, receiver_type_is_visible_in_file,
};
use crate::compiler_frontend::ast::statements::functions::FunctionSignature;
use crate::compiler_frontend::ast::statements::functions::function_signature_from_syntax_with_unresolved_types;
use crate::compiler_frontend::ast::type_interner::AstTypeInterner;
use crate::compiler_frontend::ast::type_resolution::{
    GenericParameterScopeBuildInput, build_generic_parameter_scope,
    collect_type_parameter_ids_from_declarations, collect_type_parameter_ids_from_type,
    resolve_function_signature, validate_generic_parameters_used,
};
use crate::compiler_frontend::compiler_errors::{CompilerError, CompilerMessages};
use crate::compiler_frontend::compiler_messages::{
    CompilerDiagnostic, InvalidReceiverDeclarationReason,
};
use crate::compiler_frontend::datatypes::definitions::TypeDefinition;
use crate::compiler_frontend::datatypes::environment::TypeEnvironment;
use crate::compiler_frontend::datatypes::generic_parameters::{
    GenericParameterList, TypeParameterId,
};
use crate::compiler_frontend::datatypes::ids::{
    GenericParameterId, GenericParameterListId, TypeId,
};
use crate::compiler_frontend::datatypes::{DataType, ReceiverKey};
use crate::compiler_frontend::headers::import_environment::ReceiverMethodVisibility;
use crate::compiler_frontend::headers::parse_file_headers::{FileRole, Header, HeaderKind};
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::traits::environment::TraitEnvironment;
use crate::compiler_frontend::type_coercion::compatibility::TypeCompatibilityCache;

#[cfg(feature = "detailed_timers")]
use crate::compiler_frontend::compiler_messages::compiler_dev_logging::detailed_timer_output_enabled;

use rustc_hash::FxHashMap;
use std::rc::Rc;

impl<'context, 'services> AstModuleEnvironmentBuilder<'context, 'services> {
    /// Resolves function signatures after struct declarations are available.
    ///
    /// WHY: late resolution lets signatures use named struct types and receiver syntax
    /// without adding a second nominal-type system just for headers.
    pub(in crate::compiler_frontend::ast) fn resolve_function_signatures(
        &mut self,
        sorted_headers: &[Header],
        trait_environment: &TraitEnvironment,
        string_table: &mut StringTable,
    ) -> Result<(), CompilerMessages> {
        #[cfg(feature = "detailed_timers")]
        let mut resolved_function_count = 0usize;

        for header in sorted_headers {
            let HeaderKind::Function {
                generic_parameters,
                signature,
            } = &header.kind
            else {
                continue;
            };

            let visibility = self
                .import_environment
                .visibility_for(&header.source_file)
                .map_err(|error| self.error_messages(error, string_table))?
                .clone();

            let resolved_bounds_by_local = self.resolve_generic_parameter_bounds(
                generic_parameters,
                &visibility,
                trait_environment,
                string_table,
            )?;
            if header.file_role == FileRole::ModuleFacade {
                let function_name = header.tokens.src_path.name().ok_or_else(|| {
                    self.error_messages(
                        CompilerError::compiler_error(
                            "Public generic function header had no source-path name.",
                        ),
                        string_table,
                    )
                })?;
                self.validate_public_generic_bounds(
                    function_name,
                    generic_parameters,
                    &resolved_bounds_by_local,
                    trait_environment,
                    string_table,
                )?;
            }

            let registered_generic_parameters =
                if generic_parameters.is_empty() {
                    None
                } else {
                    Some(self.type_environment.register_generic_parameter_list(
                        generic_parameters,
                        &resolved_bounds_by_local,
                    ))
                };

            let generic_parameter_scope =
                build_generic_parameter_scope(GenericParameterScopeBuildInput {
                    generic_parameters,
                    canonical_by_local: registered_generic_parameters
                        .as_ref()
                        .map(|registered| &registered.canonical_by_local),
                    visible_source_bindings: &visibility.visible_source_names,
                    visible_type_aliases: &visibility.visible_type_alias_names,
                    visible_external_symbols: &visibility.visible_external_symbols,
                    declaration_table: self.declaration_table.as_ref(),
                    generic_declarations_by_path: &self.module_symbols.generic_declarations_by_path,
                    string_table,
                })
                .map_err(|diagnostic| self.diagnostic_messages(*diagnostic, string_table))?;

            // ---------------------------------
            //  Parse unresolved signature
            // ---------------------------------

            let unresolved_signature = {
                let source_file_scope = header.canonical_source_file(string_table);
                let signature_context = ScopeContext::new(
                    ContextKind::ConstantHeader,
                    header.tokens.src_path.to_owned(),
                    Rc::clone(&self.declaration_table),
                    self.context.external_package_registry.clone(),
                    vec![],
                )
                .with_style_directives(self.context.style_directives)
                .with_build_profile(self.context.build_profile)
                .with_project_path_resolver(self.context.project_path_resolver.clone())
                .with_path_format_config(self.context.path_format_config.clone())
                .with_template_const_loop_iteration_limit(
                    self.context.template_const_loop_iteration_limit,
                )
                .with_rendered_path_usage_sink(Rc::clone(&self.rendered_path_usages))
                .with_visible_declarations(visibility.visible_declaration_paths.clone())
                .with_visible_external_symbols(visibility.visible_external_symbols.clone())
                .with_visible_source_bindings(visibility.visible_source_names.clone())
                .with_visible_type_aliases(visibility.visible_type_alias_names.clone())
                .with_resolved_type_aliases(Rc::new(self.resolved_type_aliases_by_path.clone()))
                .with_generic_declarations(Rc::new(
                    self.module_symbols.generic_declarations_by_path.clone(),
                ))
                .with_resolved_struct_fields_by_path(Rc::new(
                    self.resolved_struct_fields_by_path.clone(),
                ))
                .with_nominal_type_ids_by_path(Rc::new(self.nominal_type_ids_by_path.clone()))
                .with_source_file_scope(source_file_scope);
                let mut compatibility_cache = TypeCompatibilityCache::new();
                let mut type_interner =
                    AstTypeInterner::new(&mut self.type_environment, &mut compatibility_cache);
                let signature = function_signature_from_syntax_with_unresolved_types(
                    signature,
                    &signature_context,
                    &mut type_interner,
                    string_table,
                )
                .map_err(|diagnostic| self.diagnostic_messages(diagnostic, string_table))?;
                self.warnings
                    .extend(signature_context.take_emitted_warnings());
                signature
            };

            // -------------------------------
            //  Resolve and validate signature
            // -------------------------------

            let mut type_resolution_context = self.type_resolution_context_for_with_traits(
                &visibility,
                generic_parameter_scope.as_ref(),
                Some(trait_environment),
            );
            let resolved_signature = resolve_function_signature(
                &header.tokens.src_path,
                &unresolved_signature,
                registered_generic_parameters
                    .as_ref()
                    .map(|parameters| parameters.list_id),
                &mut type_resolution_context,
                string_table,
            )
            .map_err(|diagnostic| self.diagnostic_messages(*diagnostic, string_table))?;

            let mut used_generic_parameters = rustc_hash::FxHashSet::default();
            collect_type_parameter_ids_from_declarations(
                &resolved_signature.signature.parameters,
                &mut used_generic_parameters,
            );
            for return_slot in &resolved_signature.signature.returns {
                collect_type_parameter_ids_from_type(
                    return_slot.data_type(),
                    &mut used_generic_parameters,
                );
            }
            if let Some(registered_generic_parameters) = registered_generic_parameters.as_ref() {
                collect_type_parameter_ids_from_signature_type_ids(
                    &resolved_signature.signature,
                    &self.type_environment,
                    &registered_generic_parameters.canonical_by_local,
                    &mut used_generic_parameters,
                );
            }
            validate_generic_parameters_used(
                generic_parameters,
                &used_generic_parameters,
                &header.tokens.src_path,
                &header.name_location,
            )
            .map_err(|diagnostic| self.diagnostic_messages(*diagnostic, string_table))?;

            // ---------------------------------
            //  Update declaration table
            // ---------------------------------

            let update_result = match self.declaration_table_mut() {
                Ok(declaration_table) => {
                    if let Some(function_declaration) =
                        declaration_table.get_mut_by_path(&header.tokens.src_path)
                    {
                        // Body parsing consults declaration-table placeholders before the
                        // function body expression exists. Keep receiver metadata on the
                        // placeholder so free-call lookup can reject receiver methods without
                        // inspecting diagnostic-only type spelling.
                        function_declaration.value.function_receiver =
                            resolved_signature.receiver.to_owned();
                        function_declaration.value.diagnostic_type = DataType::Function(
                            Box::new(resolved_signature.receiver.to_owned()),
                            resolved_signature.signature.to_owned(),
                        );
                        Ok(())
                    } else {
                        Err(CompilerError::compiler_error(
                            "Function declaration was not registered before AST signature resolution.",
                        ))
                    }
                }
                Err(error) => Err(error),
            };

            update_result.map_err(|error| self.error_messages(error, string_table))?;

            if !generic_parameters.is_empty() {
                let Some(registered_generic_parameters) = registered_generic_parameters.as_ref()
                else {
                    return Err(self.error_messages(
                        CompilerError::compiler_error(
                            "Generic function parameters were not registered before template construction.",
                        ),
                        string_table,
                    ));
                };

                let template = build_generic_function_template(
                    header,
                    generic_parameters,
                    &resolved_signature.signature,
                    registered_generic_parameters.list_id,
                );
                self.generic_function_templates_by_path
                    .insert(header.tokens.src_path.to_owned(), template);
            }

            self.resolved_function_signatures_by_path
                .insert(header.tokens.src_path.to_owned(), resolved_signature);

            #[cfg(feature = "detailed_timers")]
            {
                resolved_function_count += 1;
            }
        }

        #[cfg(feature = "detailed_timers")]
        if detailed_timer_output_enabled() {
            saying::say!(
                "\n AST/function signatures/resolved count: ",
                resolved_function_count
            );
        }

        Ok(())
    }

    /// Builds the receiver method catalog from resolved function signatures.
    ///
    /// WHY: receiver methods are indexed by canonical receiver type after all signatures are
    /// resolved, so that later body emission can perform receiver-call lookup without
    /// re-resolving types.
    pub(in crate::compiler_frontend::ast) fn build_receiver_catalog(
        &self,
        sorted_headers: &[Header],
        string_table: &mut StringTable,
    ) -> Result<Rc<ReceiverMethodCatalog>, CompilerMessages> {
        let catalog = build_receiver_method_catalog(BuildReceiverMethodCatalogInput {
            sorted_headers,
            resolved_function_signatures_by_path: &self.resolved_function_signatures_by_path,
            struct_fields_by_path: &self.resolved_struct_fields_by_path,
            struct_source_by_path: &self.struct_source_by_path,
            choice_source_by_path: &self.choice_source_by_path,
            source_file_by_symbol_path: &self.module_symbols.canonical_source_by_symbol_path,
            file_visibility_by_source: &self.import_environment.file_visibility_by_source,
            resolved_type_aliases_by_path: &self.resolved_type_aliases_by_path,
            external_package_registry: self.context.external_package_registry,
            string_table,
        })
        .map_err(|error| match error {
            ReceiverMethodCatalogError::Diagnostic(diagnostic) => {
                self.diagnostic_messages(diagnostic, string_table)
            }
            ReceiverMethodCatalogError::Infrastructure(error) => {
                self.error_messages(error, string_table)
            }
        })?;

        #[cfg(feature = "detailed_timers")]
        if detailed_timer_output_enabled() {
            saying::say!(
                "\n AST/receiver catalog/methods indexed: ",
                catalog.by_function_path.len()
            );
        }

        Ok(Rc::new(catalog))
    }

    /// Validate file-local receiver-method imports against the resolved receiver catalog.
    ///
    /// WHAT: confirms imported methods have a visible receiver type and that a local method name
    /// does not resolve to two different methods for the same receiver.
    /// WHY: header import preparation can tell that a declaration is receiver-shaped, but only
    /// AST signature resolution knows the canonical receiver key. Keeping this semantic check
    /// here preserves the Stage 2/Stage 4 boundary instead of making headers re-resolve types.
    pub(in crate::compiler_frontend::ast) fn validate_receiver_method_import_visibility(
        &self,
        receiver_methods: &ReceiverMethodCatalog,
        string_table: &mut StringTable,
    ) -> Result<(), CompilerMessages> {
        for (source_file, file_visibility) in &self.import_environment.file_visibility_by_source {
            for visible_methods in file_visibility.visible_receiver_methods.values() {
                let mut methods_by_receiver: FxHashMap<ReceiverKey, &ReceiverMethodVisibility> =
                    FxHashMap::default();

                for visible_method in visible_methods {
                    let Some(method_entry) = receiver_methods
                        .by_function_path
                        .get(&visible_method.function_path)
                    else {
                        continue;
                    };

                    if method_entry.kind == ReceiverMethodKind::FileLocalExtension
                        && &method_entry.visibility_source_file != source_file
                    {
                        return Err(self.diagnostic_messages(
                            CompilerDiagnostic::invalid_receiver_declaration(
                                InvalidReceiverDeclarationReason::NonExportableExtensionMethodImport,
                                visible_method.location.clone(),
                            ),
                            string_table,
                        ));
                    }

                    if !receiver_type_is_visible_in_file(
                        &method_entry.receiver,
                        file_visibility,
                        &self.resolved_type_aliases_by_path,
                    ) {
                        return Err(self.diagnostic_messages(
                            CompilerDiagnostic::invalid_receiver_declaration(
                                InvalidReceiverDeclarationReason::ImportedReceiverTypeNotVisible,
                                visible_method.location.clone(),
                            ),
                            string_table,
                        ));
                    }

                    // Two different methods for the same receiver key were imported
                    // under the same local name.
                    if let Some(previous_method) =
                        methods_by_receiver.insert(method_entry.receiver.to_owned(), visible_method)
                        && previous_method.function_path != visible_method.function_path
                    {
                        return Err(self.diagnostic_messages(
                            CompilerDiagnostic::invalid_receiver_declaration(
                                InvalidReceiverDeclarationReason::ImportedMethodCollision,
                                visible_method.location.clone(),
                            ),
                            string_table,
                        ));
                    }
                }
            }
        }

        Ok(())
    }
}

/// Constructs a generic function template from a resolved header and signature.
///
/// WHY: generic function templates carry the unresolved body tokens and generic parameter
/// metadata so that concrete instantiations can be emitted later during AST body lowering.
fn build_generic_function_template(
    header: &Header,
    generic_parameters: &GenericParameterList,
    signature: &FunctionSignature,
    generic_parameter_list_id: GenericParameterListId,
) -> GenericFunctionTemplate {
    debug_assert!(
        !generic_parameters.is_empty(),
        "generic function template construction requires generic parameters"
    );

    GenericFunctionTemplate {
        function_path: header.tokens.src_path.to_owned(),
        source_file: header.source_file.to_owned(),
        generic_parameter_list_id,
        signature: signature.to_owned(),
        body_tokens: header.tokens.to_owned(),
        declaration_location: header.name_location.to_owned(),
    }
}

fn collect_type_parameter_ids_from_signature_type_ids(
    signature: &FunctionSignature,
    type_environment: &TypeEnvironment,
    canonical_by_local: &FxHashMap<TypeParameterId, GenericParameterId>,
    used_parameters: &mut rustc_hash::FxHashSet<TypeParameterId>,
) {
    for parameter in &signature.parameters {
        collect_type_parameter_ids_from_type_id(
            parameter.value.type_id,
            type_environment,
            canonical_by_local,
            used_parameters,
        );
    }

    for return_slot in &signature.returns {
        if let Some(return_type_id) = return_slot.type_id {
            collect_type_parameter_ids_from_type_id(
                return_type_id,
                type_environment,
                canonical_by_local,
                used_parameters,
            );
        }
    }
}

fn collect_type_parameter_ids_from_type_id(
    type_id: TypeId,
    type_environment: &TypeEnvironment,
    canonical_by_local: &FxHashMap<TypeParameterId, GenericParameterId>,
    used_parameters: &mut rustc_hash::FxHashSet<TypeParameterId>,
) {
    match type_environment.get(type_id) {
        Some(TypeDefinition::GenericParameter(parameter)) => {
            if let Some(local_id) =
                local_id_for_canonical_parameter(parameter.id, canonical_by_local)
            {
                used_parameters.insert(local_id);
            }
        }

        Some(TypeDefinition::Constructed(constructed)) => {
            for argument in constructed.arguments.iter() {
                collect_type_parameter_ids_from_type_id(
                    *argument,
                    type_environment,
                    canonical_by_local,
                    used_parameters,
                );
            }
        }

        Some(TypeDefinition::Function(function)) => {
            for parameter in function.parameters.iter() {
                collect_type_parameter_ids_from_type_id(
                    parameter.type_id,
                    type_environment,
                    canonical_by_local,
                    used_parameters,
                );
            }

            for return_type_id in function.returns.iter() {
                collect_type_parameter_ids_from_type_id(
                    *return_type_id,
                    type_environment,
                    canonical_by_local,
                    used_parameters,
                );
            }

            if let Some(error_return) = function.error_return {
                collect_type_parameter_ids_from_type_id(
                    error_return,
                    type_environment,
                    canonical_by_local,
                    used_parameters,
                );
            }
        }

        Some(TypeDefinition::GenericInstance(instance)) => {
            for argument in instance.arguments.iter() {
                collect_type_parameter_ids_from_type_id(
                    *argument,
                    type_environment,
                    canonical_by_local,
                    used_parameters,
                );
            }
        }

        Some(
            TypeDefinition::Builtin(..)
            | TypeDefinition::Struct(..)
            | TypeDefinition::Choice(..)
            | TypeDefinition::External(..)
            | TypeDefinition::DynamicTrait(..),
        )
        | None => {}
    }
}

fn local_id_for_canonical_parameter(
    canonical_id: GenericParameterId,
    canonical_by_local: &FxHashMap<TypeParameterId, GenericParameterId>,
) -> Option<TypeParameterId> {
    canonical_by_local
        .iter()
        .find_map(|(local_id, mapped_canonical_id)| {
            (*mapped_canonical_id == canonical_id).then_some(*local_id)
        })
}
