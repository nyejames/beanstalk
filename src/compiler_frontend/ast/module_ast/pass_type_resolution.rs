//! Pass 3: type resolution for constants and struct field types.
//!
//! WHAT: parses constant values and resolves struct field types in dependency order.
//! WHY: struct defaults can reference constants, so constants must be parsed first;
//! both use file-scoped visibility gates from pass 2.

use super::build_state::AstBuildState;
use crate::compiler_frontend::ast::ast_nodes::Declaration;
use crate::compiler_frontend::ast::expressions::expression::{Expression, ExpressionKind};
use crate::compiler_frontend::ast::import_bindings::{
    ConstantHeaderParseContext, FileImportBindings, parse_constant_header_declaration,
};
use crate::compiler_frontend::ast::type_resolution::{
    resolve_struct_field_types, validate_no_recursive_runtime_structs,
};
use crate::compiler_frontend::compiler_errors::CompilerMessages;
use crate::compiler_frontend::compiler_errors::ErrorMetaDataKey;
use crate::compiler_frontend::datatypes::{DataType, Ownership};
use crate::compiler_frontend::headers::module_symbols::DeclarationStubKind;
use crate::compiler_frontend::headers::parse_file_headers::{Header, HeaderKind};
use crate::compiler_frontend::headers::visible_scope::HeaderStageVisibleScope;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use rustc_hash::FxHashMap;
use std::rc::Rc;

impl<'a> AstBuildState<'a> {
    /// Pass 3: Resolve constants and struct field types in dependency order.
    /// WHY: struct defaults require constant-context parsing and import gates, so defaults
    /// can consume constants deterministically.
    pub(super) fn resolve_types(
        &mut self,
        sorted_headers: &[Header],
        file_import_bindings: &FxHashMap<InternedPath, FileImportBindings>,
        string_table: &mut StringTable,
    ) -> Result<(), CompilerMessages> {
        self.resolve_constant_headers(sorted_headers, file_import_bindings, string_table)?;

        for header in sorted_headers {
            let HeaderKind::Struct { fields } = &header.kind else {
                continue;
            };

            let bindings = file_import_bindings
                .get(&header.source_file)
                .cloned()
                .unwrap_or_default();
            let source_file_scope = header.canonical_source_file(string_table);

            let fields = resolve_struct_field_types(
                &header.tokens.src_path,
                fields,
                &self.declarations,
                Some(&bindings.visible_symbol_paths),
                string_table,
            )
            .map_err(|error| self.error_messages(error, string_table))?;

            self.resolved_struct_fields_by_path
                .insert(header.tokens.src_path.to_owned(), fields.to_owned());
            self.struct_source_by_path.insert(
                header.tokens.src_path.to_owned(),
                source_file_scope.to_owned(),
            );

            self.declarations.push(Declaration {
                id: header.tokens.src_path.to_owned(),
                value: Expression::new(
                    ExpressionKind::NoValue,
                    header.name_location.to_owned(),
                    DataType::runtime_struct(
                        header.tokens.src_path.to_owned(),
                        fields,
                        Ownership::MutableOwned,
                    ),
                    Ownership::ImmutableReference,
                ),
            });
        }

        validate_no_recursive_runtime_structs(&self.resolved_struct_fields_by_path, string_table)
            .map_err(|error| self.error_messages(error, string_table))
    }

    fn resolve_constant_headers(
        &mut self,
        sorted_headers: &[Header],
        file_import_bindings: &FxHashMap<InternedPath, FileImportBindings>,
        string_table: &mut StringTable,
    ) -> Result<(), CompilerMessages> {
        let mut pending_headers = sorted_headers
            .iter()
            .filter(|header| matches!(header.kind, HeaderKind::Constant { .. }))
            .collect::<Vec<_>>();
        let visible_scope =
            HeaderStageVisibleScope::new(&self.module_symbols, self.style_directives);

        while !pending_headers.is_empty() {
            let mut deferred_headers = Vec::new();
            let mut deferred_error = None;
            let mut made_progress = false;

            for header in pending_headers {
                let bindings = file_import_bindings
                    .get(&header.source_file)
                    .cloned()
                    .unwrap_or_default();

                match parse_constant_header_declaration(
                    header,
                    ConstantHeaderParseContext {
                        declarations: Rc::new(self.declarations.clone()),
                        visible_declaration_ids: &bindings.visible_symbol_paths,
                        host_registry: self.host_registry,
                        style_directives: self.style_directives,
                        project_path_resolver: self.project_path_resolver.clone(),
                        path_format_config: self.path_format_config.clone(),
                        build_profile: self.build_profile,
                        warnings: &mut self.warnings,
                        rendered_path_usages: self.rendered_path_usages.clone(),
                        string_table,
                    },
                ) {
                    Ok(declaration) => {
                        self.declarations.push(declaration.clone());
                        self.module_constants.push(declaration);
                        made_progress = true;
                    }
                    Err(error)
                        if is_deferrable_constant_resolution_error(
                            header,
                            &error,
                            &visible_scope,
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
    }
}

fn is_deferrable_constant_resolution_error(
    header: &Header,
    error: &crate::compiler_frontend::compiler_errors::CompilerError,
    visible_scope: &HeaderStageVisibleScope<'_>,
    string_table: &mut StringTable,
) -> bool {
    let Some(variable_name) = error.metadata.get(&ErrorMetaDataKey::VariableName) else {
        return false;
    };

    let variable_id = string_table.intern(variable_name);
    let Some(stub) = visible_scope.visible_stub(variable_id, &header.source_file, string_table)
    else {
        return false;
    };

    matches!(stub.kind, DeclarationStubKind::Constant)
}
