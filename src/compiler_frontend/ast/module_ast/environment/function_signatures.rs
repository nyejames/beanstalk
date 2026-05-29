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
    ReceiverMethodCatalogError, build_receiver_method_catalog,
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
    CompilerDiagnostic, DeferredFeatureReason, InvalidReceiverDeclarationReason,
};
use crate::compiler_frontend::datatypes::generic_parameters::GenericParameterList;
use crate::compiler_frontend::datatypes::ids::GenericParameterListId;
use crate::compiler_frontend::datatypes::{DataType, ReceiverKey};
use crate::compiler_frontend::declaration_syntax::signature_members::FunctionSignatureSyntax;
use crate::compiler_frontend::headers::import_environment::{
    FileVisibility, NamespaceTypeMember, ReceiverMethodVisibility,
};
use crate::compiler_frontend::headers::parse_file_headers::{Header, HeaderKind};
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;
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

            if let Some(diagnostic) = generic_receiver_method_deferred_diagnostic(
                generic_parameters,
                signature,
                &header.name_location,
                string_table,
            ) {
                return Err(self.diagnostic_messages(diagnostic, string_table));
            }

            let registered_generic_parameters = if generic_parameters.is_empty() {
                None
            } else {
                Some(
                    self.type_environment
                        .register_generic_parameter_list(generic_parameters),
                )
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

            let mut type_resolution_context =
                self.type_resolution_context_for(&visibility, generic_parameter_scope.as_ref());
            let resolved_signature = resolve_function_signature(
                &header.tokens.src_path,
                &unresolved_signature,
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
        let catalog = build_receiver_method_catalog(
            sorted_headers,
            &self.resolved_function_signatures_by_path,
            &self.resolved_struct_fields_by_path,
            &self.struct_source_by_path,
            &self.module_symbols.canonical_source_by_symbol_path,
            string_table,
        )
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
                catalog.by_receiver_and_name.len()
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
        for file_visibility in self.import_environment.file_visibility_by_source.values() {
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

                    if !self.receiver_type_is_visible(file_visibility, &method_entry.receiver) {
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

    /// Checks whether a receiver type is visible in the given file visibility context.
    ///
    /// A receiver is considered visible when:
    /// - the receiver is a scalar (not a struct), or
    /// - the struct's declaration path is directly visible, or
    /// - the struct is reachable through a visible type alias or namespace record.
    fn receiver_type_is_visible(
        &self,
        file_visibility: &FileVisibility,
        receiver: &ReceiverKey,
    ) -> bool {
        let ReceiverKey::Struct(receiver_path) = receiver else {
            return true;
        };

        if file_visibility
            .visible_declaration_paths
            .contains(receiver_path)
        {
            return true;
        }

        file_visibility
            .visible_type_alias_names
            .values()
            .any(|alias_path| self.type_path_matches_receiver(alias_path, receiver_path))
            || file_visibility
                .visible_namespace_records
                .values()
                .any(|record| {
                    record.type_members.values().any(|member| {
                        self.namespace_type_member_matches_receiver(member, receiver_path)
                    })
                })
    }

    /// Checks whether a namespace type member resolves to the given receiver struct path.
    fn namespace_type_member_matches_receiver(
        &self,
        member: &NamespaceTypeMember,
        receiver_path: &InternedPath,
    ) -> bool {
        match member {
            NamespaceTypeMember::SourceDeclaration(type_path) => {
                self.type_path_matches_receiver(type_path, receiver_path)
            }
            NamespaceTypeMember::ExternalSymbol(_) => false,
        }
    }

    /// Checks whether a type path resolves to the given receiver struct path.
    ///
    /// This includes direct path equality and resolution through non-const struct type aliases.
    fn type_path_matches_receiver(
        &self,
        type_path: &InternedPath,
        receiver_path: &InternedPath,
    ) -> bool {
        if type_path == receiver_path {
            return true;
        }

        matches!(
            self.resolved_type_aliases_by_path.get(type_path),
            Some(DataType::Struct {
                nominal_path,
                const_record: false,
                ..
            }) if nominal_path == receiver_path
        )
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

/// Produces the deferred-feature diagnostic for generic receiver methods.
///
/// WHY: generic receiver methods are not yet supported. Detecting them early prevents
/// later stages from building invalid generic parameter scopes for receiver-shaped signatures.
fn generic_receiver_method_deferred_diagnostic(
    generic_parameters: &GenericParameterList,
    signature: &FunctionSignatureSyntax,
    location: &SourceLocation,
    string_table: &StringTable,
) -> Option<CompilerDiagnostic> {
    if generic_parameters.is_empty() {
        return None;
    }

    let first_parameter = signature.parameters.first()?;

    if first_parameter.id.name_str(string_table) == Some("this") {
        return Some(CompilerDiagnostic::deferred_feature_reason(
            DeferredFeatureReason::GenericReceiverMethod,
            location.to_owned(),
        ));
    }

    None
}
