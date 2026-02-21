//! HIR Builder
//!
//! Responsible for lowering Typed AST -> HIR.
//!
//! This stage:
//! - Linearizes control flow into blocks
//! - Allocates locals
//! - Constructs HIR expressions/statements
//! - Establishes an explicit region tree
//!
//! This stage does NOT:
//! - Insert possible_drop
//! - Perform borrow checking
//! - Perform ownership eligibility analysis
//!
//! Those occur in later compilation phases.

use crate::compiler_frontend::ast::ast::Ast;
use crate::compiler_frontend::ast::ast_nodes::{AstNode, TextLocation};
use crate::compiler_frontend::compiler_errors::{CompilerError, CompilerMessages};
use crate::compiler_frontend::hir::hir_display::HirSideTable;
use crate::compiler_frontend::hir::{hir_datatypes::*, hir_nodes::*};
use crate::compiler_frontend::string_interning::{InternedString, StringTable};
use crate::return_hir_transformation_error;
use rustc_hash::FxHashMap;

// -----------
// Entry Point
// -----------
pub fn lower_module(
    ast: Ast,
    string_table: &mut StringTable,
) -> Result<HirModule, CompilerMessages> {
    let mut ctx = HirBuilder::new(string_table);
    ctx.build_hir_module(ast)
}

// -------------------
// HIR Builder Context
// -------------------
//
// This struct is the main entry point for the HIR builder. It manages the state of the builder
// and provides the lowering logic for each AST node.
//
// The builder is stateful and re-entrant, so it's not safe to use concurrently.

pub struct HirBuilder<'a> {
    // === Result being built ===
    pub(super) module: HirModule,

    // === For variable name resolution ===
    pub(super) string_table: &'a mut StringTable,

    // === ID Counters ===
    pub(super) next_block_id: u32,
    pub(super) next_local_id: u32,
    pub(super) next_node_id: u32,
    pub(super) next_region_id: u32,
    pub(super) next_function_id: u32,
    pub(super) next_struct_id: u32,
    pub(super) next_field_id: u32,
    pub(super) temp_local_counter: u32,

    // === Type interning ===
    pub(super) type_context: TypeContext,
    pub(super) type_interner: FxHashMap<HirTypeKind, TypeId>,

    // === Source / name side table ===
    pub(super) side_table: HirSideTable,

    // === Name resolution tables (filled during declaration pass) ===
    pub(super) locals_by_name: FxHashMap<InternedString, LocalId>,
    pub(super) functions_by_name: FxHashMap<InternedString, FunctionId>,
    pub(super) structs_by_name: FxHashMap<InternedString, StructId>,
    pub(super) fields_by_struct_and_name: FxHashMap<(StructId, InternedString), FieldId>,

    // === Fast ID -> arena index maps ===
    pub(super) block_index_by_id: FxHashMap<BlockId, usize>,
    pub(super) function_index_by_id: FxHashMap<FunctionId, usize>,
    pub(super) region_index_by_id: FxHashMap<RegionId, usize>,

    // === Current Function State ===
    pub(super) current_function: Option<FunctionId>,
    pub(super) current_block: Option<BlockId>,
    pub(super) current_region: Option<RegionId>,

    // Parallel Metadata Arrays (Index-aligned with the arenas above)
    // This is for resolving statements back to their original source code locations
    pub statement_locations: Vec<TextLocation>,
}

impl<'a> HirBuilder<'a> {
    // -----------
    // Constructor
    // -----------
    pub fn new(string_table: &'a mut StringTable) -> HirBuilder<'a> {
        HirBuilder {
            module: HirModule::new(),

            string_table,

            next_block_id: 0,
            next_local_id: 0,
            next_node_id: 0,
            next_region_id: 0,
            next_function_id: 0,
            next_struct_id: 0,
            next_field_id: 0,
            temp_local_counter: 0,

            type_context: TypeContext::default(),
            type_interner: FxHashMap::default(),
            side_table: HirSideTable::default(),

            locals_by_name: FxHashMap::default(),
            functions_by_name: FxHashMap::default(),
            structs_by_name: FxHashMap::default(),
            fields_by_struct_and_name: FxHashMap::default(),

            block_index_by_id: FxHashMap::default(),
            function_index_by_id: FxHashMap::default(),
            region_index_by_id: FxHashMap::default(),

            current_function: None,
            current_block: None,
            current_region: None,

            statement_locations: vec![],
        }
    }

    // ========================================================================
    // Main Build Method
    // ========================================================================
    /// Builds an HIR module from an AST.
    /// This is the main entry point for HIR generation.
    pub fn build_hir_module(mut self, ast: Ast) -> Result<HirModule, CompilerMessages> {
        self.module.warnings = ast.warnings.clone();

        if let Err(error) = self.prepare_hir_declarations(&ast) {
            return Err(CompilerMessages {
                errors: vec![error],
                warnings: self.module.warnings.clone(),
            });
        }

        for node in &ast.nodes {
            if let Err(error) = self.process_ast_node(node) {
                return Err(CompilerMessages {
                    errors: vec![error],
                    warnings: self.module.warnings.clone(),
                });
            }
        }

        self.module.type_context = self.type_context;
        self.module.side_table = self.side_table;

        Ok(self.module)
    }

    /// Processes a single AST node and generates corresponding HIR.
    fn process_ast_node(&mut self, node: &AstNode) -> Result<(), CompilerError> {
        self.lower_top_level_node(node)
    }

    pub(crate) fn allocate_block_id(&mut self) -> BlockId {
        let id = BlockId(self.next_block_id);
        self.next_block_id += 1;
        id
    }

    pub(crate) fn allocate_function_id(&mut self) -> FunctionId {
        let id = FunctionId(self.next_function_id);
        self.next_function_id += 1;
        id
    }

    pub(crate) fn allocate_region_id(&mut self) -> RegionId {
        let id = RegionId(self.next_region_id);
        self.next_region_id += 1;
        id
    }

    pub(crate) fn allocate_struct_id(&mut self) -> StructId {
        let id = StructId(self.next_struct_id);
        self.next_struct_id += 1;
        id
    }

    pub(crate) fn allocate_field_id(&mut self) -> FieldId {
        let id = FieldId(self.next_field_id);
        self.next_field_id += 1;
        id
    }

    pub(super) fn push_region(&mut self, region: HirRegion) {
        let index = self.module.regions.len();
        self.region_index_by_id.insert(region.id(), index);
        self.module.regions.push(region);
    }

    pub(super) fn push_block(&mut self, block: HirBlock) {
        let index = self.module.blocks.len();
        self.block_index_by_id.insert(block.id, index);
        self.module.blocks.push(block);
    }

    pub(super) fn push_function(&mut self, function: HirFunction) {
        let index = self.module.functions.len();
        self.function_index_by_id.insert(function.id, index);
        self.module.functions.push(function);
    }

    pub(super) fn block_index_or_error(
        &self,
        block_id: BlockId,
        location: &TextLocation,
    ) -> Result<usize, CompilerError> {
        let Some(index) = self.block_index_by_id.get(&block_id).copied() else {
            return_hir_transformation_error!(
                format!("Block {:?} is not registered in HIR module", block_id),
                location.to_error_location(self.string_table)
            );
        };

        Ok(index)
    }

    pub(super) fn function_index_or_error(
        &self,
        function_id: FunctionId,
        location: &TextLocation,
    ) -> Result<usize, CompilerError> {
        let Some(index) = self.function_index_by_id.get(&function_id).copied() else {
            return_hir_transformation_error!(
                format!("Function {:?} is not registered in HIR module", function_id),
                location.to_error_location(self.string_table)
            );
        };

        Ok(index)
    }

    pub(super) fn block_by_id_or_error(
        &self,
        block_id: BlockId,
        location: &TextLocation,
    ) -> Result<&HirBlock, CompilerError> {
        let index = self.block_index_or_error(block_id, location)?;
        Ok(&self.module.blocks[index])
    }

    pub(super) fn block_mut_by_id_or_error(
        &mut self,
        block_id: BlockId,
        location: &TextLocation,
    ) -> Result<&mut HirBlock, CompilerError> {
        let index = self.block_index_or_error(block_id, location)?;
        Ok(&mut self.module.blocks[index])
    }

    pub(super) fn function_by_id_or_error(
        &self,
        function_id: FunctionId,
        location: &TextLocation,
    ) -> Result<&HirFunction, CompilerError> {
        let index = self.function_index_or_error(function_id, location)?;
        Ok(&self.module.functions[index])
    }

    pub(super) fn function_mut_by_id_or_error(
        &mut self,
        function_id: FunctionId,
        location: &TextLocation,
    ) -> Result<&mut HirFunction, CompilerError> {
        let index = self.function_index_or_error(function_id, location)?;
        Ok(&mut self.module.functions[index])
    }

    pub(crate) fn enter_function(
        &mut self,
        function_id: FunctionId,
        location: &TextLocation,
    ) -> Result<(), CompilerError> {
        let entry_block = self.function_by_id_or_error(function_id, location)?.entry;

        self.current_function = Some(function_id);
        self.locals_by_name.clear();
        self.set_current_block(entry_block, location)
    }

    pub(crate) fn leave_function(&mut self) {
        self.current_function = None;
        self.current_block = None;
        self.current_region = None;
        self.locals_by_name.clear();
    }

    pub(crate) fn set_current_block(
        &mut self,
        block_id: BlockId,
        location: &TextLocation,
    ) -> Result<(), CompilerError> {
        let region = self.block_by_id_or_error(block_id, location)?.region;
        self.current_block = Some(block_id);
        self.current_region = Some(region);
        Ok(())
    }

    pub(crate) fn current_block_id_or_error(
        &self,
        location: &TextLocation,
    ) -> Result<BlockId, CompilerError> {
        let Some(block_id) = self.current_block else {
            return_hir_transformation_error!(
                "No current HIR block is active",
                location.to_error_location(self.string_table)
            );
        };

        Ok(block_id)
    }

    pub(crate) fn set_block_terminator(
        &mut self,
        block_id: BlockId,
        terminator: HirTerminator,
        source_location: &TextLocation,
    ) -> Result<(), CompilerError> {
        {
            let block = self.block_mut_by_id_or_error(block_id, source_location)?;
            if !Self::is_placeholder_terminator(&block.terminator) {
                return_hir_transformation_error!(
                    format!("Block {} already has an explicit terminator", block_id),
                    source_location.to_error_location(self.string_table)
                );
            }

            block.terminator = terminator;
        }

        self.side_table.map_terminator(source_location, block_id);
        Ok(())
    }

    pub(crate) fn block_has_explicit_terminator(
        &self,
        block_id: BlockId,
        location: &TextLocation,
    ) -> Result<bool, CompilerError> {
        let block = self.block_by_id_or_error(block_id, location)?;
        Ok(!Self::is_placeholder_terminator(&block.terminator))
    }

    fn is_placeholder_terminator(terminator: &HirTerminator) -> bool {
        matches!(terminator, HirTerminator::Panic { message: None })
    }

    #[cfg(test)]
    pub(crate) fn test_push_block(&mut self, block: HirBlock) {
        self.push_block(block);
    }

    #[cfg(test)]
    pub(crate) fn test_set_current_region(&mut self, region: RegionId) {
        self.current_region = Some(region);
    }

    #[cfg(test)]
    pub(crate) fn test_set_current_block(&mut self, block_id: BlockId) {
        self.current_block = Some(block_id);
    }

    #[cfg(test)]
    pub(crate) fn test_register_local_in_block(&mut self, local: HirLocal, name: InternedString) {
        let current_block = self.current_block.unwrap_or(BlockId(0));
        let _ = self
            .block_mut_by_id_or_error(current_block, &TextLocation::default())
            .map(|block| block.locals.push(local.clone()));

        self.locals_by_name.insert(name, local.id);
        self.side_table.bind_local_name(local.id, name);
        self.side_table.map_local_source(&local);

        if local.id.0 >= self.next_local_id {
            self.next_local_id = local.id.0 + 1;
        }
    }

    #[cfg(test)]
    pub(crate) fn test_register_function_name(&mut self, name: InternedString, id: FunctionId) {
        self.functions_by_name.insert(name, id);
        self.side_table.bind_function_name(id, name);

        if id.0 >= self.next_function_id {
            self.next_function_id = id.0 + 1;
        }
    }
}
