//! Top-level orchestration for HIR -> Exec IR and optional execution.

use crate::backends::rust_interpreter::debug::build_debug_outputs;
use crate::backends::rust_interpreter::error::InterpreterBackendError;
use crate::backends::rust_interpreter::lowering::lower_hir_module_to_exec_program;
use crate::backends::rust_interpreter::request::{
    InterpreterBackendRequest, InterpreterEntrypoint, InterpreterExecutionMode,
};
use crate::backends::rust_interpreter::result::{
    InterpreterBackendResult, InterpreterExecutionMetadata, InterpreterExecutionResult,
};
use crate::backends::rust_interpreter::runtime::RuntimeEngine;
use crate::compiler_frontend::analysis::borrow_checker::BorrowFacts;
use crate::compiler_frontend::compiler_messages::compiler_errors::CompilerMessages;
use crate::compiler_frontend::hir::ids::FunctionId;
use crate::compiler_frontend::hir::module::HirModule;
use crate::compiler_frontend::symbols::string_interning::StringTable;

pub(crate) fn lower_hir_to_exec_program(
    hir_module: &HirModule,
    borrow_facts: &BorrowFacts,
    request: &InterpreterBackendRequest,
    string_table: &StringTable,
) -> Result<InterpreterBackendResult, CompilerMessages> {
    validate_request(hir_module, request).map_err(|error| {
        CompilerMessages::from_error(error.into_compiler_error(), string_table.clone())
    })?;

    let exec_program = lower_hir_module_to_exec_program(hir_module, borrow_facts, string_table)
        .map_err(|error| CompilerMessages::from_error(error, string_table.clone()))?;

    let execution_result = match &request.execution_mode {
        InterpreterExecutionMode::LowerOnly => None,

        InterpreterExecutionMode::Execute { entry, policy } => {
            let returned_value = match entry {
                InterpreterEntrypoint::Start => {
                    let mut runtime = RuntimeEngine::new(exec_program.clone(), *policy);
                    runtime.execute_start().map_err(|error| {
                        CompilerMessages::from_error(
                            error.into_compiler_error(),
                            string_table.clone(),
                        )
                    })?
                }

                InterpreterEntrypoint::Function(function_id) => {
                    return Err(CompilerMessages::from_error(
                        InterpreterBackendError::Execution {
                            message: format!(
                                "Rust interpreter backend cannot execute specific function {function_id:?} yet"
                            ),
                        }
                            .into_compiler_error(),
                        string_table.clone(),
                    ));
                }
            };

            Some(InterpreterExecutionResult { returned_value })
        }
    };

    let execution_metadata = match &request.execution_mode {
        InterpreterExecutionMode::LowerOnly => None,
        InterpreterExecutionMode::Execute { entry, policy } => Some(InterpreterExecutionMetadata {
            entry: entry.clone(),
            policy: *policy,
        }),
    };

    let debug_outputs = build_debug_outputs(request, &exec_program, execution_result.as_ref());

    Ok(InterpreterBackendResult {
        exec_program,
        execution_result,
        execution_metadata,
        debug_outputs,
    })
}

fn validate_request(
    hir_module: &HirModule,
    request: &InterpreterBackendRequest,
) -> Result<(), InterpreterBackendError> {
    if !contains_function(hir_module, hir_module.start_function) {
        return Err(InterpreterBackendError::InvalidRequest {
            message: format!(
                "Rust interpreter backend could not find the module start function {:?}",
                hir_module.start_function
            ),
        });
    }

    if let InterpreterExecutionMode::Execute { entry, .. } = &request.execution_mode {
        match entry {
            InterpreterEntrypoint::Start => {}
            InterpreterEntrypoint::Function(function_id) => {
                if !contains_function(hir_module, *function_id) {
                    return Err(InterpreterBackendError::InvalidRequest {
                        message: format!(
                            "Rust interpreter backend request references missing function {function_id:?}"
                        ),
                    });
                }
            }
        }
    }

    Ok(())
}

fn contains_function(hir_module: &HirModule, function_id: FunctionId) -> bool {
    hir_module
        .functions
        .iter()
        .any(|function| function.id == function_id)
}
