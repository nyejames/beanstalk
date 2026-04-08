//! Module-level interpreter lowering orchestration.

use crate::backends::rust_interpreter::exec_ir::ExecFunctionId;
use crate::backends::rust_interpreter::exec_ir::ExecProgram;
use crate::backends::rust_interpreter::lowering::context::LoweringContext;
use crate::backends::rust_interpreter::lowering::functions::lower_function_shell;
use crate::compiler_frontend::analysis::borrow_checker::BorrowFacts;
use crate::compiler_frontend::compiler_messages::compiler_errors::CompilerError;
use crate::compiler_frontend::hir::hir_nodes::HirModule;
use crate::compiler_frontend::string_interning::StringTable;

pub(crate) fn lower_hir_module_to_exec_program(
    hir_module: &HirModule,
    _borrow_facts: &BorrowFacts,
    _string_table: &StringTable,
) -> Result<ExecProgram, CompilerError> {
    let mut context = LoweringContext::new(hir_module);

    register_function_ids(&mut context);
    lower_function_shells(&mut context)?;
    attach_start_function(&mut context)?;

    Ok(context.exec_program)
}

fn register_function_ids(context: &mut LoweringContext<'_>) {
    for (index, function) in context.hir_module.functions.iter().enumerate() {
        context
            .function_id_by_hir_id
            .insert(function.id, ExecFunctionId(index as u32));
    }
}

fn lower_function_shells(context: &mut LoweringContext<'_>) -> Result<(), CompilerError> {
    for function in &context.hir_module.functions {
        lower_function_shell(context, function)?;
    }

    Ok(())
}

fn attach_start_function(context: &mut LoweringContext<'_>) -> Result<(), CompilerError> {
    let Some(exec_function_id) = context
        .function_id_by_hir_id
        .get(&context.hir_module.start_function)
        .copied()
    else {
        return Err(CompilerError::compiler_error(format!(
            "Rust interpreter lowering could not map start function {:?} into Exec IR",
            context.hir_module.start_function
        )));
    };

    context.exec_program.module.entry_function = Some(exec_function_id);
    Ok(())
}
