//! Type resolution for constants and nominal declarations.
//!
//! WHAT: parses constant values and resolves struct field types in dependency order.
//! WHY: struct defaults can reference constants, so constants must be parsed first;
//! both use file-scoped visibility gates from pass 2.

use super::builder::AstModuleEnvironmentBuilder;
use crate::compiler_frontend::ast::ast_nodes::Declaration;
use crate::compiler_frontend::ast::expressions::expression::{Expression, ExpressionKind};
use crate::compiler_frontend::ast::import_bindings::{
    ConstantHeaderParseContext, FileImportBindings, parse_constant_header_declaration,
};
use crate::compiler_frontend::ast::instrumentation::{AstCounter, increment_ast_counter};
use crate::compiler_frontend::ast::module_ast::scope_context::TopLevelDeclarationIndex;
use crate::compiler_frontend::ast::type_resolution::{
    build_generic_parameter_scope, collect_type_parameter_ids_from_choice_variants,
    collect_type_parameter_ids_from_declarations, resolve_choice_variant_payload_types,
    resolve_struct_field_types, validate_generic_parameters_used,
    validate_no_recursive_generic_type, validate_no_recursive_runtime_structs,
};
use crate::compiler_frontend::compiler_errors::CompilerMessages;
use crate::compiler_frontend::compiler_errors::ErrorMetaDataKey;
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::declaration_syntax::type_syntax::{
    TypeResolutionContext, TypeResolutionContextInputs,
};
use crate::compiler_frontend::value_mode::ValueMode;

use crate::compiler_frontend::headers::parse_file_headers::{Header, HeaderKind};
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::timer_log;
use rustc_hash::{FxHashMap, FxHashSet};
use std::rc::Rc;
use std::time::Instant;

impl<'context, 'services> AstModuleEnvironmentBuilder<'context, 'services> {
    /// Resolves constants and nominal declaration types in dependency order.
    /// WHY: struct defaults require constant-context parsing and import gates, so defaults
    /// can consume constants deterministically.
    pub(in crate::compiler_frontend::ast) fn resolve_types(
        &mut self,
        sorted_headers: &[Header],
        file_import_bindings: &FxHashMap<InternedPath, FileImportBindings>,
        string_table: &mut StringTable,
    ) -> Result<(), CompilerMessages> {
        let constant_resolution_start = Instant::now();
        self.resolve_constant_headers(sorted_headers, file_import_bindings, string_table)?;
        timer_log!(
            constant_resolution_start,
            "AST/environment/constants resolved in: "
        );
        let _ = constant_resolution_start;

        let struct_fields_resolution_start = Instant::now();
        for header in sorted_headers {
            let HeaderKind::Struct {
                generic_parameters,
                fields,
            } = &header.kind
            else {
                continue;
            };

            let bindings = file_import_bindings
                .get(&header.source_file)
                .cloned()
                .unwrap_or_default();
            let source_file_scope = header.canonical_source_file(string_table);
            let generic_parameter_scope = build_generic_parameter_scope(
                generic_parameters,
                &bindings.visible_source_bindings,
                &bindings.visible_type_aliases,
                &bindings.visible_external_symbols,
                &self.declarations,
                &self.module_symbols.generic_declarations_by_path,
                string_table,
            )
            .map_err(|error| self.error_messages(error, string_table))?;
            let type_resolution_context =
                TypeResolutionContext::from_inputs(TypeResolutionContextInputs {
                    declarations: &self.declarations,
                    visible_declaration_ids: Some(&bindings.visible_symbol_paths),
                    visible_external_symbols: Some(&bindings.visible_external_symbols),
                    visible_source_bindings: Some(&bindings.visible_source_bindings),
                    visible_type_aliases: Some(&bindings.visible_type_aliases),
                    resolved_type_aliases: Some(&self.resolved_type_aliases_by_path),
                    generic_declarations_by_path: Some(
                        &self.module_symbols.generic_declarations_by_path,
                    ),
                    resolved_struct_fields_by_path: Some(&self.resolved_struct_fields_by_path),
                    generic_nominal_instantiations: Some(
                        self.generic_nominal_instantiations.as_ref(),
                    ),
                })
                .with_generic_parameters(generic_parameter_scope.as_ref());

            let fields = resolve_struct_field_types(
                &header.tokens.src_path,
                fields,
                &type_resolution_context,
                string_table,
            )
            .map_err(|error| self.error_messages(error, string_table))?;

            self.resolved_struct_fields_by_path
                .insert(header.tokens.src_path.to_owned(), fields.to_owned());

            let mut used_parameters = FxHashSet::default();
            collect_type_parameter_ids_from_declarations(&fields, &mut used_parameters);
            validate_generic_parameters_used(
                generic_parameters,
                &used_parameters,
                &header.tokens.src_path,
                &header.name_location,
                string_table,
            )
            .map_err(|error| self.error_messages(error, string_table))?;

            for field in &fields {
                validate_no_recursive_generic_type(
                    &header.tokens.src_path,
                    &field.value.data_type,
                    &field.value.location,
                    string_table,
                )
                .map_err(|error| self.error_messages(error, string_table))?;
            }

            self.struct_source_by_path.insert(
                header.tokens.src_path.to_owned(),
                source_file_scope.to_owned(),
            );

            self.replace_declaration(Declaration {
                id: header.tokens.src_path.to_owned(),
                value: Expression::new(
                    ExpressionKind::NoValue,
                    header.name_location.to_owned(),
                    DataType::runtime_struct(header.tokens.src_path.to_owned(), fields),
                    ValueMode::ImmutableReference,
                ),
            })
            .map_err(|error| self.error_messages(error, string_table))?;
        }
        timer_log!(
            struct_fields_resolution_start,
            "AST/environment/nominal types/struct fields resolved in: "
        );
        let _ = struct_fields_resolution_start;

        let choice_resolution_start = Instant::now();
        for header in sorted_headers {
            let HeaderKind::Choice {
                generic_parameters,
                variants,
            } = &header.kind
            else {
                continue;
            };

            let bindings = file_import_bindings
                .get(&header.source_file)
                .cloned()
                .unwrap_or_default();
            let generic_parameter_scope = build_generic_parameter_scope(
                generic_parameters,
                &bindings.visible_source_bindings,
                &bindings.visible_type_aliases,
                &bindings.visible_external_symbols,
                &self.declarations,
                &self.module_symbols.generic_declarations_by_path,
                string_table,
            )
            .map_err(|error| self.error_messages(error, string_table))?;
            let type_resolution_context =
                TypeResolutionContext::from_inputs(TypeResolutionContextInputs {
                    declarations: &self.declarations,
                    visible_declaration_ids: Some(&bindings.visible_symbol_paths),
                    visible_external_symbols: Some(&bindings.visible_external_symbols),
                    visible_source_bindings: Some(&bindings.visible_source_bindings),
                    visible_type_aliases: Some(&bindings.visible_type_aliases),
                    resolved_type_aliases: Some(&self.resolved_type_aliases_by_path),
                    generic_declarations_by_path: Some(
                        &self.module_symbols.generic_declarations_by_path,
                    ),
                    resolved_struct_fields_by_path: Some(&self.resolved_struct_fields_by_path),
                    generic_nominal_instantiations: Some(
                        self.generic_nominal_instantiations.as_ref(),
                    ),
                })
                .with_generic_parameters(generic_parameter_scope.as_ref());

            let resolved_variants = resolve_choice_variant_payload_types(
                variants,
                &type_resolution_context,
                string_table,
            )
            .map_err(|error| self.error_messages(error, string_table))?;

            let mut used_parameters = FxHashSet::default();
            collect_type_parameter_ids_from_choice_variants(
                &resolved_variants,
                &mut used_parameters,
            );
            validate_generic_parameters_used(
                generic_parameters,
                &used_parameters,
                &header.tokens.src_path,
                &header.name_location,
                string_table,
            )
            .map_err(|error| self.error_messages(error, string_table))?;

            for variant in &resolved_variants {
                if let crate::compiler_frontend::declaration_syntax::choice::ChoiceVariantPayload::Record {
                    fields,
                } = &variant.payload
                {
                    for field in fields {
                        validate_no_recursive_generic_type(
                            &header.tokens.src_path,
                            &field.value.data_type,
                            &field.value.location,
                            string_table,
                        )
                        .map_err(|error| self.error_messages(error, string_table))?;
                    }
                }
            }

            self.replace_declaration(Declaration {
                id: header.tokens.src_path.to_owned(),
                value: Expression::new(
                    ExpressionKind::NoValue,
                    header.name_location.to_owned(),
                    DataType::Choices {
                        nominal_path: header.tokens.src_path.to_owned(),
                        variants: resolved_variants,
                        generic_instance_key: None,
                    },
                    ValueMode::ImmutableReference,
                ),
            })
            .map_err(|error| self.error_messages(error, string_table))?;
        }
        timer_log!(
            choice_resolution_start,
            "AST/environment/nominal types/choice variants resolved in: "
        );
        let _ = choice_resolution_start;

        let recursive_validation_start = Instant::now();
        validate_no_recursive_runtime_structs(&self.resolved_struct_fields_by_path, string_table)
            .map_err(|error| self.error_messages(error, string_table))?;
        timer_log!(
            recursive_validation_start,
            "AST/environment/nominal types/recursive struct validation in: "
        );
        let _ = recursive_validation_start;

        Ok(())
    }

    fn resolve_constant_headers(
        &mut self,
        sorted_headers: &[Header],
        file_import_bindings: &FxHashMap<InternedPath, FileImportBindings>,
        string_table: &mut StringTable,
    ) -> Result<(), CompilerMessages> {
        let constants_resolution_start = Instant::now();
        let mut total_rounds = 0usize;
        let mut total_headers_attempted = 0usize;
        let mut total_deferred_headers = 0usize;
        let mut total_snapshot_rebuilds = 0usize;

        let constant_header_paths = sorted_headers
            .iter()
            .filter(|header| matches!(header.kind, HeaderKind::Constant { .. }))
            .map(|header| header.tokens.src_path.to_owned())
            .collect::<FxHashSet<_>>();

        let resolution_result = (|| -> Result<(), CompilerMessages> {
            let mut pending_headers = sorted_headers
                .iter()
                .filter(|header| matches!(header.kind, HeaderKind::Constant { .. }))
                .collect::<Vec<_>>();
            let empty_visible_symbol_paths = FxHashSet::default();
            let empty_visible_external_symbols = FxHashMap::default();
            let empty_visible_source_bindings = FxHashMap::default();
            let empty_visible_type_aliases = FxHashMap::default();
            let resolved_type_aliases = Rc::new(self.resolved_type_aliases_by_path.clone());
            let generic_declarations =
                Rc::new(self.module_symbols.generic_declarations_by_path.clone());

            while !pending_headers.is_empty() {
                total_rounds += 1;
                increment_ast_counter(AstCounter::ConstantResolutionRounds);
                total_headers_attempted += pending_headers.len();

                // Reuse one declaration snapshot for deferred attempts in this round.
                // Refresh only after successful resolutions so later constants can see
                // newly-resolved declarations without cloning on every deferred header.
                let mut declarations_snapshot =
                    Rc::new(TopLevelDeclarationIndex::new(self.declarations.clone()));
                let mut round_snapshot_rebuilds = 1usize;
                increment_ast_counter(AstCounter::DeclarationSnapshotRebuilds);
                let mut unresolved_constant_paths = declarations_snapshot
                    .declarations()
                    .iter()
                    .filter(|declaration| declaration.is_unresolved_constant_placeholder())
                    .map(|declaration| declaration.id.to_owned())
                    .collect::<FxHashSet<_>>();
                let mut deferred_headers = Vec::new();
                let mut deferred_error = None;
                let mut made_progress = false;

                for header in pending_headers {
                    let visible_symbol_paths = file_import_bindings
                        .get(&header.source_file)
                        .map(|bindings| &bindings.visible_symbol_paths)
                        .unwrap_or(&empty_visible_symbol_paths);
                    let visible_external_symbols = file_import_bindings
                        .get(&header.source_file)
                        .map(|bindings| &bindings.visible_external_symbols)
                        .unwrap_or(&empty_visible_external_symbols);
                    let visible_source_bindings = file_import_bindings
                        .get(&header.source_file)
                        .map(|bindings| &bindings.visible_source_bindings)
                        .unwrap_or(&empty_visible_source_bindings);
                    let visible_type_aliases = file_import_bindings
                        .get(&header.source_file)
                        .map(|bindings| &bindings.visible_type_aliases)
                        .unwrap_or(&empty_visible_type_aliases);
                    let resolved_type_aliases = Rc::clone(&resolved_type_aliases);
                    let generic_declarations_by_path = Rc::clone(&generic_declarations);

                    match parse_constant_header_declaration(
                        header,
                        ConstantHeaderParseContext {
                            top_level_declarations: Rc::clone(&declarations_snapshot),
                            visible_declaration_ids: visible_symbol_paths,
                            visible_external_symbols,
                            visible_source_bindings,
                            visible_type_aliases,
                            resolved_type_aliases,
                            generic_declarations_by_path,
                            external_package_registry: self.context.external_package_registry,
                            style_directives: self.context.style_directives,
                            project_path_resolver: self.context.project_path_resolver.clone(),
                            path_format_config: self.context.path_format_config.clone(),
                            build_profile: self.context.build_profile,
                            warnings: &mut self.warnings,
                            rendered_path_usages: self.rendered_path_usages.clone(),
                            unresolved_constant_paths: &unresolved_constant_paths,
                            string_table,
                        },
                    ) {
                        Ok(declaration) => {
                            self.replace_declaration(declaration.clone())
                                .map_err(|error| self.error_messages(error, string_table))?;
                            self.module_constants.push(declaration);
                            declarations_snapshot =
                                Rc::new(TopLevelDeclarationIndex::new(self.declarations.clone()));
                            round_snapshot_rebuilds += 1;
                            increment_ast_counter(AstCounter::DeclarationSnapshotRebuilds);
                            unresolved_constant_paths = declarations_snapshot
                                .declarations()
                                .iter()
                                .filter(|resolved| resolved.is_unresolved_constant_placeholder())
                                .map(|resolved| resolved.id.to_owned())
                                .collect::<FxHashSet<_>>();
                            made_progress = true;
                        }
                        Err(error)
                            if is_deferrable_constant_resolution_error(
                                &error,
                                visible_symbol_paths,
                                &constant_header_paths,
                                string_table,
                            ) =>
                        {
                            deferred_headers.push(header);
                            deferred_error.get_or_insert(error);
                        }
                        Err(error) => {
                            return Err(self.error_messages(error, string_table));
                        }
                    }
                }

                total_snapshot_rebuilds += round_snapshot_rebuilds;
                total_deferred_headers += deferred_headers.len();

                if !made_progress {
                    let error = deferred_error.unwrap_or_else(|| {
                        crate::compiler_frontend::compiler_errors::CompilerError::compiler_error(
                            "Constant header resolution stalled without making progress.",
                        )
                    });
                    return Err(self.error_messages(error, string_table));
                }

                pending_headers = deferred_headers;
            }

            Ok(())
        })();

        timer_log!(
            constants_resolution_start,
            "AST/environment/constants deferred resolution in: "
        );
        let _ = constants_resolution_start;

        #[cfg(feature = "detailed_timers")]
        saying::say!(
            "AST/type resolution/constants deferred summary: \n rounds = ", Dark Green total_rounds,
            Reset "\n headers attempted = ", Dark Green total_headers_attempted,
            Reset "\n headers deferred = ", Dark Green total_deferred_headers,
            Reset "\n declaration snapshot rebuilds = ", Dark Green total_snapshot_rebuilds
        );

        resolution_result
    }
}

fn is_deferrable_constant_resolution_error(
    error: &crate::compiler_frontend::compiler_errors::CompilerError,
    visible_symbol_paths: &FxHashSet<InternedPath>,
    constant_header_paths: &FxHashSet<InternedPath>,
    string_table: &mut StringTable,
) -> bool {
    let Some(variable_name) = error.metadata.get(&ErrorMetaDataKey::VariableName) else {
        return false;
    };

    let variable_id = string_table.intern(variable_name);

    visible_symbol_paths
        .iter()
        .filter(|path| path.name() == Some(variable_id))
        .any(|path| constant_header_paths.contains(path))
}
