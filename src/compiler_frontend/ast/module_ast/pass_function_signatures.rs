//! Passes 4–5: function signature resolution and receiver method catalog construction.
//!
//! WHAT: resolves function parameter/return types using the struct declarations from pass 3,
//! then builds an indexed receiver-method catalog from the resolved signatures.
//! WHY: late resolution lets signatures reference named struct types; the catalog depends on
//! resolved signatures and must be built before AST emission in pass 6.

use super::build_state::AstBuildState;
use crate::compiler_frontend::ast::import_bindings::FileImportBindings;
use crate::compiler_frontend::ast::receiver_methods::build_receiver_method_catalog;
use crate::compiler_frontend::ast::type_resolution::resolve_function_signature;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::compiler_errors::CompilerMessages;
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::headers::parse_file_headers::{Header, HeaderKind};
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use rustc_hash::FxHashMap;
use std::rc::Rc;

use crate::compiler_frontend::ast::module_ast::scope_context::ReceiverMethodCatalog;

impl<'a> AstBuildState<'a> {
    /// Pass 4: Resolve function signatures after struct declarations are available.
    /// WHY: late resolution lets signatures use named struct types and receiver syntax
    /// without adding a second nominal-type system just for headers.
    pub(in crate::compiler_frontend::ast) fn resolve_function_signatures(
        &mut self,
        sorted_headers: &[Header],
        file_import_bindings: &FxHashMap<InternedPath, FileImportBindings>,
        string_table: &mut StringTable,
    ) -> Result<(), CompilerMessages> {
        #[cfg(feature = "detailed_timers")]
        let mut resolved_function_count = 0usize;

        for header in sorted_headers {
            let HeaderKind::Function { signature } = &header.kind else {
                continue;
            };

            let bindings = file_import_bindings
                .get(&header.source_file)
                .cloned()
                .unwrap_or_default();
            let resolved_signature = resolve_function_signature(
                &header.tokens.src_path,
                signature,
                &self.declarations,
                Some(&bindings.visible_symbol_paths),
                Some(&bindings.visible_external_symbols),
                Some(&bindings.visible_source_aliases),
                Some(&bindings.visible_type_aliases),
                Some(&self.resolved_type_aliases_by_path),
                string_table,
            )
            .map_err(|error| self.error_messages(error, string_table))?;

            let Some(function_declaration) = self
                .declarations
                .iter_mut()
                .find(|declaration| declaration.id == header.tokens.src_path)
            else {
                return Err(self.error_messages(
                    CompilerError::compiler_error(
                        "Function declaration was not registered before AST signature resolution.",
                    ),
                    string_table,
                ));
            };

            function_declaration.value.data_type = DataType::Function(
                Box::new(resolved_signature.receiver.to_owned()),
                resolved_signature.signature.to_owned(),
            );
            self.resolved_function_signatures_by_path
                .insert(header.tokens.src_path.to_owned(), resolved_signature);
            #[cfg(feature = "detailed_timers")]
            {
                resolved_function_count += 1;
            }
        }

        #[cfg(feature = "detailed_timers")]
        saying::say!(
            "\n AST/function signatures/resolved count: ",
            resolved_function_count
        );

        Ok(())
    }

    /// Pass 5: Build the receiver method catalog from resolved function signatures.
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
        .map_err(|error| self.error_messages(error, string_table))?;

        #[cfg(feature = "detailed_timers")]
        saying::say!(
            "\n AST/receiver catalog/methods indexed: ",
            catalog.by_receiver_and_name.len()
        );

        Ok(Rc::new(catalog))
    }
}
