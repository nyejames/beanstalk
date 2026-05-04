//! Function signature resolution and receiver method catalog construction.
//!
//! WHAT: resolves function parameter/return types using the struct declarations from pass 3,
//! then builds an indexed receiver-method catalog from the resolved signatures.
//! WHY: late resolution lets signatures use named struct types and receiver syntax
//! without adding a second nominal-type system just for headers.

use super::builder::AstModuleEnvironmentBuilder;
use crate::compiler_frontend::ast::receiver_methods::build_receiver_method_catalog;
use crate::compiler_frontend::ast::type_resolution::{
    build_generic_parameter_scope, collect_type_parameter_ids_from_declarations,
    collect_type_parameter_ids_from_type, resolve_function_signature,
    validate_generic_parameters_used,
};
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::compiler_errors::CompilerMessages;
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::datatypes::generics::GenericParameterList;
use crate::compiler_frontend::headers::parse_file_headers::{Header, HeaderKind};
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;
use std::rc::Rc;

use crate::compiler_frontend::ast::module_ast::scope_context::ReceiverMethodCatalog;

impl<'context, 'services> AstModuleEnvironmentBuilder<'context, 'services> {
    /// Resolves function signatures after struct declarations are available.
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
                .map_err(|error| self.error_messages(error, string_table))?;

            reject_generic_receiver_method(
                generic_parameters,
                signature,
                &header.name_location,
                string_table,
            )
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
            let resolved_signature = resolve_function_signature(
                &header.tokens.src_path,
                signature,
                &type_resolution_context,
                string_table,
            )
            .map_err(|error| self.error_messages(error, string_table))?;

            let mut used_parameters = rustc_hash::FxHashSet::default();
            collect_type_parameter_ids_from_declarations(
                &resolved_signature.signature.parameters,
                &mut used_parameters,
            );
            for return_slot in &resolved_signature.signature.returns {
                collect_type_parameter_ids_from_type(return_slot.data_type(), &mut used_parameters);
            }
            validate_generic_parameters_used(
                generic_parameters,
                &used_parameters,
                &header.tokens.src_path,
                &header.name_location,
                string_table,
            )
            .map_err(|error| self.error_messages(error, string_table))?;

            let update_result = match self.declaration_table_mut() {
                Ok(table) => {
                    if let Some(function_declaration) =
                        table.get_mut_by_path(&header.tokens.src_path)
                    {
                        function_declaration.value.data_type = DataType::Function(
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

    /// Builds the receiver method catalog from resolved function signatures.
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

fn reject_generic_receiver_method(
    generic_parameters: &GenericParameterList,
    signature: &crate::compiler_frontend::ast::statements::functions::FunctionSignature,
    location: &SourceLocation,
    string_table: &StringTable,
) -> Result<(), CompilerError> {
    if generic_parameters.is_empty() {
        return Ok(());
    }

    let Some(first_parameter) = signature.parameters.first() else {
        return Ok(());
    };

    if first_parameter.id.name_str(string_table) == Some("this") {
        return Err(CompilerError::new_rule_error(
            "Generic receiver methods are not supported yet. Use a generic free function instead.",
            location.to_owned(),
        ));
    }

    Ok(())
}
