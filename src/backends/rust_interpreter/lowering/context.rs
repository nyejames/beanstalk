//! Shared lowering context for one HIR module.
//!
//! WHAT: stores deterministic ID maps, fast lookup tables, and the Exec IR under construction.
//! WHY: lowering will grow across several files, so the shared mutable state should stay centralized.

use crate::backends::rust_interpreter::exec_ir::{
    ExecBlockId, ExecConst, ExecConstId, ExecConstValue, ExecFunctionId, ExecLocal, ExecLocalId,
    ExecModule, ExecProgram, ExecStorageType,
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
                "Rust interpreter lowering could not resolve HIR block {block_id:?}"
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

    /// Counter for allocating temporary locals during expression lowering.
    /// WHAT: tracks the next available temporary local index.
    /// WHY: expression lowering needs unique temporary locals for intermediate results.
    pub(crate) next_temp_local_index: u32,

    /// Total count of temporary locals allocated during function lowering.
    /// WHAT: tracks how many temporaries were allocated.
    /// WHY: function finalization needs to know the total temporary count.
    pub(crate) temp_local_count: u32,

    /// Temporary locals allocated during expression lowering.
    /// WHAT: stores ExecLocal entries for each temporary.
    /// WHY: temporary locals need to be registered in the function's local list.
    pub(crate) temp_locals: Vec<ExecLocal>,
}

impl FunctionLoweringLayout {
    /// Allocate a temporary local for storing intermediate expression results.
    ///
    /// WHAT: creates a new temporary local with a unique index after user locals.
    /// WHY: expression lowering needs storage for intermediate computation results.
    pub(crate) fn allocate_temp_local(&mut self, storage_type: ExecStorageType) -> ExecLocalId {
        let temp_index = self.next_temp_local_index;
        self.next_temp_local_index += 1;
        self.temp_local_count += 1;

        // Temporary locals come after user locals in the local array.
        let local_id = ExecLocalId(self.ordered_hir_local_ids.len() as u32 + temp_index);

        // Register the temporary in the function's local list.
        self.temp_locals.push(ExecLocal {
            id: local_id,
            debug_name: None,
            storage_type,
            role: crate::backends::rust_interpreter::exec_ir::ExecLocalRole::Temp,
        });

        local_id
    }
}
