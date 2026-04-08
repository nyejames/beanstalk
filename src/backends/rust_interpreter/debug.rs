//! Text rendering helpers for Exec IR and interpreter debug output.
//!
//! WHAT: builds deterministic human-readable summaries without affecting lowering semantics.
//! WHY: the interpreter will be difficult to trust if it cannot explain what it lowered.

use crate::backends::rust_interpreter::exec_ir::ExecProgram;
use crate::backends::rust_interpreter::request::InterpreterBackendRequest;
use crate::backends::rust_interpreter::result::{
    InterpreterDebugOutputs, InterpreterExecutionResult,
};

pub(crate) fn build_debug_outputs(
    request: &InterpreterBackendRequest,
    exec_program: &ExecProgram,
    execution_result: Option<&InterpreterExecutionResult>,
) -> InterpreterDebugOutputs {
    let mut outputs = InterpreterDebugOutputs::default();

    if request.debug_flags.show_lowering_plan {
        outputs.plan_text = Some(build_lowering_plan_text(exec_program));
    }

    if request.debug_flags.show_exec_ir {
        outputs.exec_ir_text = Some(format!("{exec_program:#?}"));
    }

    if request.debug_flags.show_function_layouts {
        outputs.function_layouts_text = Some(build_function_layouts_text(exec_program));
    }

    if request.debug_flags.show_final_value {
        outputs.final_value_text =
            execution_result.map(|result| format!("{:#?}", result.returned_value));
    }

    outputs
}

fn build_lowering_plan_text(exec_program: &ExecProgram) -> String {
    let function_count = exec_program.module.functions.len();
    let constant_count = exec_program.module.constants.len();

    format!(
        "Rust interpreter lowering produced {} function shell(s) and {} constant shell(s).",
        function_count, constant_count
    )
}

fn build_function_layouts_text(exec_program: &ExecProgram) -> String {
    let mut text = String::new();

    for function in &exec_program.module.functions {
        text.push_str(&format!("fn {} ({:?})\n", function.debug_name, function.id));
        text.push_str(&format!(
            "  params: {}\n  locals: {}\n  blocks: {}\n",
            function.parameter_slots.len(),
            function.locals.len(),
            function.blocks.len(),
        ));
    }

    text
}
