//! Shared lowering context for one HIR module.
//!
//! WHAT: stores deterministic ID maps, fast lookup tables, and the Exec IR under construction.
//! WHY: lowering will grow across several files, so the shared mutable state should stay centralized.

use crate::backends::rust_interpreter::exec_ir::{
    ExecBlockId, ExecConst, ExecConstId, ExecConstValue, ExecFunctionId, ExecLocalId, ExecModule,
    ExecProgram, ExecStorageType,
};
use crate::compiler_frontend::compiler_messages::compiler_errors::CompilerError;
use crate::compiler_frontend::hir::hir_datatypes::{HirTypeKind, TypeId};
use crate::compiler_frontend::hir::hir_nodes::{BlockId, FunctionId, HirBlock, HirModule, LocalId};
use rustc_hash::FxHashMap;

pub(crate) struct LoweringContext<'a> {
    pub(crate) hir_module: &'a HirModule,
    pub(crate) block_index_by_id: FxHashMap<BlockId, usize>,
    pub(crate) function_id_by_hir_id: FxHashMap<FunctionId, ExecFunctionId>,
    pub(crate) exec_program: ExecProgram,
}

impl<'a> LoweringContext<'a> {
    pub(crate) fn new(hir_module: &'a HirModule) -> Self {
        let block_index_by_id = hir_module
            .blocks
            .iter()
            .enumerate()
            .map(|(index, block)| (block.id, index))
            .collect::<FxHashMap<_, _>>();

        Self {
            hir_module,
            block_index_by_id,
            function_id_by_hir_id: FxHashMap::default(),
            exec_program: ExecProgram {
                module: ExecModule::new(),
            },
        }
    }

    pub(crate) fn hir_block_by_id(&self, block_id: BlockId) -> Result<&HirBlock, CompilerError> {
        let Some(index) = self.block_index_by_id.get(&block_id).copied() else {
            return Err(CompilerError::compiler_error(format!(
                "Rust interpreter lowering could not resolve HIR block {:?}",
                block_id
            )));
        };

        Ok(&self.hir_module.blocks[index])
    }

    pub(crate) fn lower_storage_type(&self, type_id: TypeId) -> ExecStorageType {
        let hir_type = self.hir_module.type_context.get(type_id);

        match &hir_type.kind {
            HirTypeKind::Unit => ExecStorageType::Unit,
            HirTypeKind::Bool => ExecStorageType::Bool,
            HirTypeKind::Int => ExecStorageType::Int,
            HirTypeKind::Float => ExecStorageType::Float,
            HirTypeKind::Char => ExecStorageType::Char,
            HirTypeKind::Function { .. } => ExecStorageType::FunctionRef,

            HirTypeKind::Decimal
            | HirTypeKind::String
            | HirTypeKind::Range
            | HirTypeKind::Tuple { .. }
            | HirTypeKind::Collection { .. }
            | HirTypeKind::Struct { .. }
            | HirTypeKind::Option { .. }
            | HirTypeKind::Result { .. }
            | HirTypeKind::Union { .. } => ExecStorageType::HeapHandle,
        }
    }

    pub(crate) fn intern_const(&mut self, value: ExecConstValue) -> ExecConstId {
        let const_id = ExecConstId(self.exec_program.module.constants.len() as u32);
        self.exec_program.module.constants.push(ExecConst {
            id: const_id,
            value,
        });
        const_id
    }
}

/// Per-function lowering layout built before block or instruction lowering.
///
/// WHAT: records how HIR locals and blocks map into this function's Exec IR shell.
/// WHY: later statement/expression lowering needs O(1) lookup for both slots and blocks.
pub(crate) struct FunctionLoweringLayout {
    pub(crate) exec_function_id: ExecFunctionId,
    pub(crate) ordered_hir_block_ids: Vec<BlockId>,
    pub(crate) exec_block_by_hir_block: FxHashMap<BlockId, ExecBlockId>,
    pub(crate) ordered_hir_local_ids: Vec<LocalId>,
    pub(crate) exec_local_by_hir_local: FxHashMap<LocalId, ExecLocalId>,
    pub(crate) scratch_local_id: ExecLocalId,
}
