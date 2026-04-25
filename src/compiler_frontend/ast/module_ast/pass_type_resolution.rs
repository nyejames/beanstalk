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
use crate::compiler_frontend::ast::module_ast::scope_context::TopLevelDeclarationIndex;
use crate::compiler_frontend::ast::type_resolution::{
    resolve_struct_field_types, validate_no_recursive_runtime_structs,
};
use crate::compiler_frontend::compiler_errors::CompilerMessages;
use crate::compiler_frontend::compiler_errors::ErrorMetaDataKey;
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::value_mode::ValueMode;

use crate::compiler_frontend::headers::parse_file_headers::{Header, HeaderKind};
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::timer_log;
use rustc_hash::{FxHashMap, FxHashSet};
use std::rc::Rc;
use std::time::Instant;

impl<'a> AstBuildState<'a> {
    /// Pass 3: Resolve constants and struct field types in dependency order.
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
            "AST/type resolution/constants resolved in: "
        );
        let _ = constant_resolution_start;

        let struct_fields_resolution_start = Instant::now();
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
                    DataType::runtime_struct(header.tokens.src_path.to_owned(), fields),
                    ValueMode::ImmutableReference,
                ),
            });
        }
        timer_log!(
            struct_fields_resolution_start,
            "AST/type resolution/struct fields resolved in: "
        );
        let _ = struct_fields_resolution_start;

        let recursive_validation_start = Instant::now();
        validate_no_recursive_runtime_structs(&self.resolved_struct_fields_by_path, string_table)
            .map_err(|error| self.error_messages(error, string_table))?;
        timer_log!(
            recursive_validation_start,
            "AST/type resolution/recursive struct validation in: "
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

            while !pending_headers.is_empty() {
                total_rounds += 1;
                total_headers_attempted += pending_headers.len();

                // Reuse one declaration snapshot for deferred attempts in this round.
                // Refresh only after successful resolutions so later constants can see
                // newly-resolved declarations without cloning on every deferred header.
                let mut declarations_snapshot =
                    Rc::new(TopLevelDeclarationIndex::new(self.declarations.clone()));
                let mut round_snapshot_rebuilds = 1usize;
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

                    match parse_constant_header_declaration(
                        header,
                        ConstantHeaderParseContext {
                            top_level_declarations: Rc::clone(&declarations_snapshot),
                            visible_declaration_ids: visible_symbol_paths,
                            host_registry: self.host_registry,
                            style_directives: self.style_directives,
                            project_path_resolver: self.project_path_resolver.clone(),
                            path_format_config: self.path_format_config.clone(),
                            build_profile: self.build_profile,
                            warnings: &mut self.warnings,
                            rendered_path_usages: self.rendered_path_usages.clone(),
                            unresolved_constant_paths: &unresolved_constant_paths,
                            string_table,
                        },
                    ) {
                        Ok(declaration) => {
                            self.declarations.push(declaration.clone());
                            self.module_constants.push(declaration);
                            declarations_snapshot =
                                Rc::new(TopLevelDeclarationIndex::new(self.declarations.clone()));
                            round_snapshot_rebuilds += 1;
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
            "AST/type resolution/constants deferred resolution in: "
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
