//! Runtime lookup/index helpers.
//!
//! WHAT: builds and queries id-based lookup tables for interpreter program structures.
//! WHY: keeping program indexing separate from the dispatch loop prevents `runtime/mod.rs`
//! from accumulating lookup-specific bloat as function calls and richer execution land.

use super::RuntimeEngine;
use crate::backends::rust_interpreter::error::InterpreterBackendError;
use crate::backends::rust_interpreter::exec_ir::{
    ExecBlock, ExecBlockId, ExecConstId, ExecConstValue, ExecFunction, ExecFunctionId, ExecProgram,
};
use rustc_hash::FxHashMap;

impl RuntimeEngine {
    pub(crate) fn function_by_id(
        &self,
        function_id: ExecFunctionId,
    ) -> Result<&ExecFunction, InterpreterBackendError> {
        let Some(index) = self.function_index_by_id.get(&function_id).copied() else {
            return Err(InterpreterBackendError::Execution {
                message: format!(
                    "Rust interpreter runtime could not resolve function {:?}",
                    function_id
                ),
            });
        };

        self.program
            .module
            .functions
            .get(index)
            .ok_or_else(|| InterpreterBackendError::InternalInvariant {
                message: format!(
                    "Rust interpreter runtime function index map points outside function table for {:?}",
                    function_id
                ),
            })
    }

    pub(crate) fn block_by_ids(
        &self,
        function_id: ExecFunctionId,
        block_id: ExecBlockId,
    ) -> Result<&ExecBlock, InterpreterBackendError> {
        let Some(block_index_by_id) = self.block_index_by_function.get(&function_id) else {
            return Err(InterpreterBackendError::Execution {
                message: format!(
                    "Rust interpreter runtime has no block index for function {:?}",
                    function_id
                ),
            });
        };

        let Some(index) = block_index_by_id.get(&block_id).copied() else {
            return Err(InterpreterBackendError::Execution {
                message: format!(
                    "Rust interpreter runtime could not resolve block {:?} in function {:?}",
                    block_id, function_id
                ),
            });
        };

        let function = self.function_by_id(function_id)?;
        function.blocks.get(index).ok_or_else(|| {
            InterpreterBackendError::InternalInvariant {
                message: format!(
                    "Rust interpreter runtime block index map points outside block table for function {:?}, block {:?}",
                    function_id, block_id
                ),
            }
        })
    }

    pub(crate) fn const_value_by_id(
        &self,
        const_id: ExecConstId,
    ) -> Result<&ExecConstValue, InterpreterBackendError> {
        let Some(index) = self.const_index_by_id.get(&const_id).copied() else {
            return Err(InterpreterBackendError::Execution {
                message: format!(
                    "Rust interpreter runtime could not resolve constant {:?}",
                    const_id
                ),
            });
        };

        self.program
            .module
            .constants
            .get(index)
            .map(|constant| &constant.value)
            .ok_or_else(|| InterpreterBackendError::InternalInvariant {
                message: format!(
                    "Rust interpreter runtime constant index map points outside constant table for {:?}",
                    const_id
                ),
            })
    }
}

pub(crate) fn build_function_index(program: &ExecProgram) -> FxHashMap<ExecFunctionId, usize> {
    let mut index_by_id = FxHashMap::default();

    for (index, function) in program.module.functions.iter().enumerate() {
        index_by_id.insert(function.id, index);
    }

    index_by_id
}

pub(crate) fn build_block_index(
    program: &ExecProgram,
) -> FxHashMap<ExecFunctionId, FxHashMap<ExecBlockId, usize>> {
    let mut block_index_by_function = FxHashMap::default();

    for function in &program.module.functions {
        let mut block_index_by_id = FxHashMap::default();

        for (index, block) in function.blocks.iter().enumerate() {
            block_index_by_id.insert(block.id, index);
        }

        block_index_by_function.insert(function.id, block_index_by_id);
    }

    block_index_by_function
}

pub(crate) fn build_const_index(program: &ExecProgram) -> FxHashMap<ExecConstId, usize> {
    let mut index_by_id = FxHashMap::default();

    for (index, constant) in program.module.constants.iter().enumerate() {
        index_by_id.insert(constant.id, index);
    }

    index_by_id
}
