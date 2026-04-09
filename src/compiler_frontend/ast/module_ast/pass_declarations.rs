//! Pass 1: declaration collection.
//!
//! WHAT: iterates sorted headers and registers every symbol into the module-wide declaration
//! and visibility tables. Also absorbs the builtin manifest (error types, builtin structs).
//! WHY: all later passes depend on stable, fully-populated declaration tables; collecting
//! everything in one dedicated pass avoids ordering surprises and missed inserts.

use super::build_state::AstBuildState;
use super::canonical_source_file_for_header;
use crate::compiler_frontend::ast::ast_nodes::Declaration;
use crate::compiler_frontend::ast::expressions::expression::{Expression, ExpressionKind};
use crate::compiler_frontend::ast::statements::functions::{
    FunctionReturn, FunctionSignature, ReturnSlot,
};
use crate::compiler_frontend::builtins::error_type::{
    is_reserved_builtin_symbol, register_builtin_error_types,
};
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::compiler_errors::CompilerMessages;
use crate::compiler_frontend::datatypes::{DataType, Ownership};
use crate::compiler_frontend::headers::parse_file_headers::{Header, HeaderKind};
use crate::compiler_frontend::identifier_policy::ensure_not_keyword_shadow_identifier;
use crate::compiler_frontend::string_interning::StringTable;
use crate::projects::settings::IMPLICIT_START_FUNC_NAME;

impl<'a> AstBuildState<'a> {
    /// Pass 1: Collect every module declaration once.
    /// WHY: resolution stores fully qualified symbol paths.
    /// Each file context later applies its own visibility filter instead of rebuilding
    /// declaration tables.
    pub(super) fn collect_declarations(
        &mut self,
        sorted_headers: &[Header],
        string_table: &mut StringTable,
    ) -> Result<(), CompilerMessages> {
        for header in sorted_headers {
            if let Some(symbol_name) = header.tokens.src_path.name() {
                let symbol_name_text = string_table.resolve(symbol_name).to_owned();

                ensure_not_keyword_shadow_identifier(
                    &symbol_name_text,
                    header.name_location.to_owned(),
                    "Module Declaration Collection",
                )
                .map_err(|error| self.error_messages(error, string_table))?;

                if is_reserved_builtin_symbol(&symbol_name_text) {
                    return Err(self.error_messages(
                        CompilerError::new_rule_error(
                            format!(
                                "'{}' is reserved as a builtin language type.",
                                symbol_name_text
                            ),
                            header.name_location.to_owned(),
                        ),
                        string_table,
                    ));
                }
            }

            self.module_file_paths.insert(header.source_file.to_owned());
            self.canonical_source_by_symbol_path.insert(
                header.tokens.src_path.to_owned(),
                canonical_source_file_for_header(header, string_table),
            );
            self.file_imports_by_source
                .entry(header.source_file.to_owned())
                .or_insert_with(|| header.file_imports.to_owned());

            match &header.kind {
                HeaderKind::Function { signature } => {
                    self.declarations.push(Declaration {
                        id: header.tokens.src_path.to_owned(),
                        value: Expression::new(
                            ExpressionKind::NoValue,
                            header.name_location.to_owned(),
                            DataType::Function(Box::new(None), signature.to_owned()),
                            Ownership::ImmutableReference,
                        ),
                    });
                    self.register_declared_symbol(
                        &header.tokens.src_path,
                        &header.source_file,
                        Some(header.exported),
                    );
                }
                HeaderKind::Struct { .. } => {
                    self.register_declared_symbol(
                        &header.tokens.src_path,
                        &header.source_file,
                        Some(header.exported),
                    );
                }
                HeaderKind::Choice { metadata } => {
                    let variants = metadata
                        .variants
                        .iter()
                        .map(|variant| Declaration {
                            id: header.tokens.src_path.append(variant.name),
                            value: Expression::no_value(
                                variant.location.to_owned(),
                                DataType::None,
                                Ownership::ImmutableOwned,
                            ),
                        })
                        .collect::<Vec<_>>();

                    self.declarations.push(Declaration {
                        id: header.tokens.src_path.to_owned(),
                        value: Expression::new(
                            ExpressionKind::NoValue,
                            header.name_location.to_owned(),
                            DataType::Choices(variants),
                            Ownership::ImmutableReference,
                        ),
                    });

                    self.register_declared_symbol(
                        &header.tokens.src_path,
                        &header.source_file,
                        Some(header.exported),
                    );
                }
                HeaderKind::StartFunction => {
                    let start_name = header
                        .source_file
                        .join_str(IMPLICIT_START_FUNC_NAME, string_table);
                    self.declarations.push(Declaration {
                        id: start_name.to_owned(),
                        value: Expression::new(
                            ExpressionKind::NoValue,
                            header.name_location.to_owned(),
                            DataType::Function(
                                Box::new(None),
                                FunctionSignature {
                                    parameters: vec![],
                                    returns: vec![ReturnSlot::success(FunctionReturn::Value(
                                        DataType::StringSlice,
                                    ))],
                                },
                            ),
                            Ownership::ImmutableReference,
                        ),
                    });
                    self.register_declared_symbol(&start_name, &header.source_file, None);
                }
                HeaderKind::Constant { .. } => {
                    self.register_declared_symbol(
                        &header.tokens.src_path,
                        &header.source_file,
                        Some(header.exported),
                    );
                }
                _ => {}
            }
        }

        let builtin_manifest = register_builtin_error_types(string_table);
        self.builtin_visible_symbol_paths
            .extend(builtin_manifest.visible_symbol_paths.iter().cloned());
        self.declarations.extend(builtin_manifest.declarations);
        self.resolved_struct_fields_by_path
            .extend(builtin_manifest.resolved_struct_fields_by_path);
        self.struct_source_by_path
            .extend(builtin_manifest.struct_source_by_path);
        self.builtin_struct_ast_nodes
            .extend(builtin_manifest.ast_struct_nodes);

        Ok(())
    }
}
