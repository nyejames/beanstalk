//! Function signature resolution for AST type resolution.

use crate::compiler_frontend::ast::statements::functions::{
    FunctionReturn, FunctionSignature, ReturnSlot,
};
use crate::compiler_frontend::ast::type_resolution::{
    TypeResolutionContext, TypeResolutionResult, resolve_diagnostic_type_to_type_id_checked,
};
use crate::compiler_frontend::compiler_messages::{
    CompilerDiagnostic, InvalidReceiverDeclarationReason, InvalidThisUsageReason,
};
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::StringTable;

use super::{ResolvedFunctionSignature, resolve_named_signature_type};

// -------------------------------
//  Function signature resolution
// -------------------------------

/// Resolve a function signature and extract receiver metadata for method cataloging.
pub(crate) fn resolve_function_signature(
    function_path: &InternedPath,
    signature: &FunctionSignature,
    type_resolution_context: &mut TypeResolutionContext<'_>,
    string_table: &mut StringTable,
) -> TypeResolutionResult<ResolvedFunctionSignature> {
    let this_name = string_table.intern("this");
    let _function_name = function_path.name_str(string_table).unwrap_or("<function>");
    let function_name_id = function_path
        .name()
        .unwrap_or_else(|| string_table.intern("<function>"));

    let function_location = type_resolution_context
        .declaration_table
        .get_by_path(function_path)
        .map(|declaration| declaration.value.location.clone())
        .unwrap_or_default();

    let mut resolved_parameters = Vec::with_capacity(signature.parameters.len());
    let mut receiver = None;

    // --------------------
    //  Resolve parameters
    // --------------------

    for (parameter_index, parameter) in signature.parameters.iter().enumerate() {
        let mut resolved_parameter = parameter.to_owned();

        resolved_parameter.value.diagnostic_type = resolve_named_signature_type(
            &parameter.value.diagnostic_type,
            &parameter.value.location,
            type_resolution_context,
            string_table,
        )?;

        // Resolve the canonical TypeId for the parameter's data type so that
        // HIR lowering sees the correct type identity (not TypeId(0) from
        // Expression::new's builtin-only mapping).
        resolved_parameter.value.type_id = resolve_diagnostic_type_to_type_id_checked(
            &resolved_parameter.value.diagnostic_type,
            type_resolution_context.type_environment,
            &resolved_parameter.value.location,
        )?;

        if resolved_parameter.id.name() == Some(this_name) {
            if receiver.is_some() {
                return Err(Box::new(CompilerDiagnostic::invalid_this_usage(
                    InvalidThisUsageReason::DuplicateThis {
                        function_name: function_name_id,
                    },
                    parameter.value.location.clone(),
                )));
            }

            if parameter_index != 0 {
                return Err(Box::new(CompilerDiagnostic::invalid_this_usage(
                    InvalidThisUsageReason::NotFirstParameter {
                        function_name: function_name_id,
                    },
                    parameter.value.location.clone(),
                )));
            }

            let Some(receiver_key) = resolved_parameter
                .value
                .diagnostic_type
                .receiver_key_from_type()
            else {
                if resolved_parameter
                    .value
                    .diagnostic_type
                    .is_resolved_generic_nominal_instance()
                    || resolved_parameter
                        .value
                        .diagnostic_type
                        .is_unresolved_generic_application()
                {
                    let type_name = string_table.intern(
                        &resolved_parameter
                            .value
                            .diagnostic_type
                            .display_with_table(string_table),
                    );
                    return Err(Box::new(CompilerDiagnostic::invalid_receiver_declaration(
                        InvalidReceiverDeclarationReason::GenericReceiverType {
                            function_name: function_name_id,
                            type_name,
                        },
                        parameter.value.location.clone(),
                    )));
                }

                let type_name = string_table.intern(
                    &resolved_parameter
                        .value
                        .diagnostic_type
                        .display_with_table(string_table),
                );
                return Err(Box::new(CompilerDiagnostic::invalid_receiver_declaration(
                    InvalidReceiverDeclarationReason::UnsupportedType {
                        function_name: function_name_id,
                        type_name,
                    },
                    parameter.value.location.clone(),
                )));
            };

            receiver = Some(receiver_key);
        }

        resolved_parameters.push(resolved_parameter);
    }

    // -----------------
    //  Resolve returns
    // -----------------

    let mut resolved_returns = Vec::with_capacity(signature.returns.len());

    for return_slot in &signature.returns {
        let resolved_value = match &return_slot.value {
            FunctionReturn::Value(data_type) => {
                FunctionReturn::Value(resolve_named_signature_type(
                    data_type,
                    &function_location,
                    type_resolution_context,
                    string_table,
                )?)
            }

            FunctionReturn::AliasCandidates {
                parameter_indices,
                data_type,
            } => FunctionReturn::AliasCandidates {
                parameter_indices: parameter_indices.to_owned(),
                data_type: resolve_named_signature_type(
                    data_type,
                    &function_location,
                    type_resolution_context,
                    string_table,
                )?,
            },
        };

        let type_id = resolve_diagnostic_type_to_type_id_checked(
            resolved_value.data_type(),
            type_resolution_context.type_environment,
            &function_location,
        )?;

        resolved_returns.push(ReturnSlot {
            value: resolved_value,
            type_id: Some(type_id),
            channel: return_slot.channel,
        });
    }

    Ok(ResolvedFunctionSignature {
        receiver,
        signature: FunctionSignature {
            parameters: resolved_parameters,
            returns: resolved_returns,
        },
    })
}
