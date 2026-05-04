//! Type resolution for constants and nominal declarations.
//!
//! WHAT: parses constant values and resolves struct field types in header dependency order.
//! WHY: headers are already dependency-sorted; constants are parsed linearly. Struct defaults
//! can reference constants, so constants are resolved before struct fields.

use super::builder::AstModuleEnvironmentBuilder;
use crate::compiler_frontend::ast::ast_nodes::Declaration;
use crate::compiler_frontend::ast::expressions::expression::{Expression, ExpressionKind};
use crate::compiler_frontend::ast::module_ast::environment::constant_resolution::{
    ConstantHeaderParseContext, parse_constant_header_declaration,
};
use crate::compiler_frontend::ast::type_resolution::{
    build_generic_parameter_scope, collect_type_parameter_ids_from_choice_variants,
    collect_type_parameter_ids_from_declarations, resolve_choice_variant_payload_types,
    resolve_struct_field_types, validate_generic_parameters_used,
    validate_no_recursive_generic_type, validate_no_recursive_runtime_structs,
};
use crate::compiler_frontend::compiler_errors::CompilerMessages;
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::value_mode::ValueMode;

use crate::compiler_frontend::headers::parse_file_headers::{Header, HeaderKind};
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::timer_log;
use rustc_hash::FxHashSet;
use std::rc::Rc;
use std::time::Instant;

impl<'context, 'services> AstModuleEnvironmentBuilder<'context, 'services> {
    /// Resolves constants and nominal declaration types in header dependency order.
    /// WHY: headers are already dependency-sorted; constants are parsed in that order.
    /// Struct defaults require constant-context parsing and import gates.
    pub(in crate::compiler_frontend::ast) fn resolve_types(
        &mut self,
        sorted_headers: &[Header],
        string_table: &mut StringTable,
    ) -> Result<(), CompilerMessages> {
        let constant_resolution_start = Instant::now();
        self.resolve_constant_headers(sorted_headers, string_table)?;
        timer_log!(
            constant_resolution_start,
            "AST/environment/constants resolved in: "
        );
        let _ = constant_resolution_start;

        // ----------------------------
        //  Resolve struct field types
        // ----------------------------
        let struct_fields_resolution_start = Instant::now();
        for header in sorted_headers {
            let HeaderKind::Struct {
                generic_parameters,
                fields,
            } = &header.kind
            else {
                continue;
            };

            let visibility = self
                .import_environment
                .visibility_for(&header.source_file)
                .map_err(|error| self.error_messages(error, string_table))?;

            let source_file_scope = header.canonical_source_file(string_table);
            let generic_parameter_scope = build_generic_parameter_scope(
                generic_parameters,
                &visibility.visible_source_names,
                &visibility.visible_type_alias_names,
                &visibility.visible_external_symbols,
                self.declaration_table.as_ref(),
                &self.module_symbols.generic_declarations_by_path,
                string_table,
            )
            .map_err(|error| self.error_messages(error, string_table))?;
            let type_resolution_context =
                self.type_resolution_context_for(visibility, generic_parameter_scope.as_ref());

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

        // ----------------------------
        //  Resolve choice variant payload types
        // ----------------------------
        let choice_resolution_start = Instant::now();
        for header in sorted_headers {
            let HeaderKind::Choice {
                generic_parameters,
                variants,
            } = &header.kind
            else {
                continue;
            };

            let visibility = self
                .import_environment
                .visibility_for(&header.source_file)
                .map_err(|error| self.error_messages(error, string_table))?;

            let generic_parameter_scope = build_generic_parameter_scope(
                generic_parameters,
                &visibility.visible_source_names,
                &visibility.visible_type_alias_names,
                &visibility.visible_external_symbols,
                self.declaration_table.as_ref(),
                &self.module_symbols.generic_declarations_by_path,
                string_table,
            )
            .map_err(|error| self.error_messages(error, string_table))?;
            let type_resolution_context =
                self.type_resolution_context_for(visibility, generic_parameter_scope.as_ref());

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

        // ----------------------------
        //  Validate no recursive runtime structs
        // ----------------------------
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
        string_table: &mut StringTable,
    ) -> Result<(), CompilerMessages> {
        let constants_resolution_start = Instant::now();

        let resolved_type_aliases = Rc::new(self.resolved_type_aliases_by_path.clone());
        let generic_declarations =
            Rc::new(self.module_symbols.generic_declarations_by_path.clone());

        for header in sorted_headers {
            let HeaderKind::Constant { .. } = &header.kind else {
                continue;
            };

            let visibility = self
                .import_environment
                .visibility_for(&header.source_file)
                .map_err(|error| self.error_messages(error, string_table))?;

            let declaration = parse_constant_header_declaration(
                header,
                ConstantHeaderParseContext {
                    top_level_declarations: Rc::clone(&self.declaration_table),
                    visible_declaration_ids: &visibility.visible_declaration_paths,
                    visible_external_symbols: &visibility.visible_external_symbols,
                    visible_source_bindings: &visibility.visible_source_names,
                    visible_type_aliases: &visibility.visible_type_alias_names,
                    resolved_type_aliases: Rc::clone(&resolved_type_aliases),
                    generic_declarations_by_path: Rc::clone(&generic_declarations),
                    external_package_registry: self.context.external_package_registry,
                    style_directives: self.context.style_directives,
                    project_path_resolver: self.context.project_path_resolver.clone(),
                    path_format_config: self.context.path_format_config.clone(),
                    build_profile: self.context.build_profile,
                    warnings: &mut self.warnings,
                    rendered_path_usages: self.rendered_path_usages.clone(),
                    string_table,
                },
            )
            .map_err(|error| self.error_messages(error, string_table))?;

            self.replace_declaration(declaration.clone())
                .map_err(|error| self.error_messages(error, string_table))?;
            self.module_constants.push(declaration);
        }

        timer_log!(
            constants_resolution_start,
            "AST/environment/constants resolved in: "
        );
        let _ = constants_resolution_start;

        Ok(())
    }
}
