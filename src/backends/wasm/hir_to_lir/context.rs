//! Lowering contexts shared across module/function lowering stages.

use crate::backends::wasm::lir::function::{WasmLirBlock, WasmLirFunction, WasmLirFunctionOrigin};
use crate::backends::wasm::lir::instructions::WasmLirTerminator;
use crate::backends::wasm::lir::module::WasmLirModule;
use crate::backends::wasm::lir::types::{
    WasmAbiType, WasmImportId, WasmLirBlockId, WasmLirFunctionId, WasmLirLocal, WasmLirLocalId,
    WasmLirSignature, WasmLocalRole, WasmStaticDataId,
};
use crate::backends::wasm::request::WasmBackendRequest;
use crate::backends::wasm::runtime::imports::WasmHostFunction;
use crate::compiler_frontend::analysis::borrow_checker::BorrowFacts;
use crate::compiler_frontend::hir::hir_nodes::{
    BlockId, FunctionId, HirFunction, HirModule, LocalId,
};
use crate::compiler_frontend::interned_path::InternedPath;
use rustc_hash::FxHashMap;

pub(crate) struct WasmLirLoweringContext<'a> {
    /// Immutable semantic input module.
    pub hir_module: &'a HirModule,
    /// Borrow checker side-table facts (never mutates HIR).
    pub borrow_facts: &'a BorrowFacts,
    /// Builder/backend request contract for this lowering run.
    pub request: &'a WasmBackendRequest,

    /// Incrementally built LIR output module.
    pub lir_module: WasmLirModule,

    /// Stable HIR function id -> LIR function id mapping.
    /// WHY: preserves deterministic cross-references during lowering.
    pub function_map: FxHashMap<FunctionId, WasmLirFunctionId>,
    /// Reverse lookup for path-based call targets in HIR call statements.
    pub function_id_by_path: FxHashMap<InternedPath, FunctionId>,
    /// Global UTF-8 interning pool keyed by bytes.
    /// WHY: deduplicates static segments before memory-layout planning.
    pub static_string_pool: FxHashMap<Vec<u8>, WasmStaticDataId>,
    /// Host import registry keyed by semantic host function.
    pub host_imports: FxHashMap<WasmHostFunction, WasmImportId>,
}

impl<'a> WasmLirLoweringContext<'a> {
    pub(crate) fn new(
        hir_module: &'a HirModule,
        borrow_facts: &'a BorrowFacts,
        request: &'a WasmBackendRequest,
    ) -> Self {
        Self {
            hir_module,
            borrow_facts,
            request,
            lir_module: WasmLirModule::default(),
            function_map: FxHashMap::default(),
            function_id_by_path: FxHashMap::default(),
            static_string_pool: FxHashMap::default(),
            host_imports: FxHashMap::default(),
        }
    }
}

pub(crate) struct WasmFunctionLoweringContext<'a, 'b> {
    /// Shared module-level context/state.
    pub module_context: &'a mut WasmLirLoweringContext<'b>,
    /// Current HIR function being lowered.
    pub hir_function: &'a HirFunction,

    /// Current LIR function under construction.
    pub lir_function: WasmLirFunction,

    /// HIR local -> LIR local mapping for this function.
    pub local_map: FxHashMap<LocalId, WasmLirLocalId>,
    /// HIR block -> LIR block mapping for this function.
    pub block_map: FxHashMap<BlockId, WasmLirBlockId>,
    /// Fast index lookup for mutable block access.
    pub block_index_by_id: FxHashMap<BlockId, usize>,
    /// Local-id allocator state scoped to this function.
    pub next_local_id: u32,
}

impl<'a, 'b> WasmFunctionLoweringContext<'a, 'b> {
    pub(crate) fn new(
        module_context: &'a mut WasmLirLoweringContext<'b>,
        hir_function: &'a HirFunction,
        id: WasmLirFunctionId,
        debug_name: String,
        origin: WasmLirFunctionOrigin,
        signature: WasmLirSignature,
    ) -> Self {
        Self {
            module_context,
            hir_function,
            lir_function: WasmLirFunction {
                id,
                debug_name,
                origin,
                signature,
                locals: Vec::new(),
                blocks: Vec::new(),
                linkage: crate::backends::wasm::lir::linkage::WasmFunctionLinkage::Internal,
            },
            local_map: FxHashMap::default(),
            block_map: FxHashMap::default(),
            block_index_by_id: FxHashMap::default(),
            next_local_id: 0,
        }
    }

    pub(crate) fn alloc_local(
        &mut self,
        name: Option<String>,
        ty: WasmAbiType,
        role: WasmLocalRole,
    ) -> WasmLirLocalId {
        let local_id = WasmLirLocalId(self.next_local_id);
        self.next_local_id += 1;

        self.lir_function.locals.push(WasmLirLocal {
            id: local_id,
            name,
            ty,
            role,
        });

        local_id
    }

    pub(crate) fn alloc_temp(&mut self, ty: WasmAbiType) -> WasmLirLocalId {
        self.alloc_local(None, ty, WasmLocalRole::Temp)
    }

    pub(crate) fn alloc_block(&mut self, source_block: BlockId) -> WasmLirBlockId {
        // WHAT: pre-allocate every reachable block before lowering statements/terminators.
        // WHY: branch/jump lowering only needs O(1) id lookup and never forward-fills.
        let block_id = WasmLirBlockId(self.lir_function.blocks.len() as u32);
        let index = self.lir_function.blocks.len();

        self.block_map.insert(source_block, block_id);
        self.block_index_by_id.insert(source_block, index);

        self.lir_function.blocks.push(WasmLirBlock {
            id: block_id,
            statements: Vec::new(),
            terminator: WasmLirTerminator::Trap,
        });

        block_id
    }

    pub(crate) fn block_mut(&mut self, source_block: BlockId) -> Option<&mut WasmLirBlock> {
        let index = self.block_index_by_id.get(&source_block).copied()?;
        self.lir_function.blocks.get_mut(index)
    }
}
