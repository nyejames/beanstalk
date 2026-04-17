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
use crate::compiler_frontend::ast::ast::AstDocFragmentKind;
use crate::compiler_frontend::ast::ast_nodes::{AstNode, Declaration, SourceLocation};
use crate::compiler_frontend::compiler_errors::{CompilerError, CompilerMessages};
use crate::compiler_frontend::hir::hir_datatypes::{HirTypeKind, TypeContext, TypeId};
use crate::compiler_frontend::hir::hir_nodes::{
    BlockId, FieldId, FunctionId, HirBlock, HirConstId, HirDocFragment, HirDocFragmentKind,
    HirFunction, HirFunctionOrigin, HirModule, HirNodeId, HirRegion, HirTerminator, HirValueId,
    LocalId, RegionId, StructId,
};
use crate::compiler_frontend::hir::hir_side_table::HirSideTable;
use crate::compiler_frontend::hir::hir_validation::validate_hir_module;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::paths::path_format::PathStringFormatConfig;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::return_hir_transformation_error;
use rustc_hash::{FxHashMap, FxHashSet};

// -----------
// Entry Point
// -----------
pub fn lower_module(
    ast: Ast,
    string_table: &mut StringTable,
    path_format_config: PathStringFormatConfig,
) -> Result<HirModule, CompilerMessages> {
    let ctx = HirBuilder::new(string_table, path_format_config);
    ctx.build_hir_module(ast)
}

#[derive(Debug, Clone, Copy)]
pub(super) struct LoopTargets {
    pub break_target: BlockId,
    pub continue_target: BlockId,
}

#[cfg(test)]
#[path = "tests/hir_builder_test_support.rs"]
mod hir_builder_test_support;
#[cfg(test)]
pub(crate) use hir_builder_test_support::validate_module_for_tests;
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

    // === Path formatting config (origin, output style) ===
    pub(super) path_format_config: PathStringFormatConfig,

    // === ID Counters ===
    next_block_id: u32,
    next_local_id: u32,
    next_node_id: u32,
    next_value_id: u32,
    next_region_id: u32,
    next_function_id: u32,
    next_struct_id: u32,
    next_field_id: u32,
    next_const_id: u32,
    pub(super) temp_local_counter: u32,
    pub(super) template_function_counter: u32,

    // === Type interning ===
    pub(super) type_context: TypeContext,
    pub(super) type_interner: FxHashMap<HirTypeKind, TypeId>,

    // === Source / name side table ===
    pub(super) side_table: HirSideTable,

    // === Name resolution tables (filled during declaration pass) ===
    // AST guarantees module-wide unique InternedPath symbol IDs. HIR keys symbol resolution
    // by full paths, never by scope-local leaf strings.
    pub(super) locals_by_name: FxHashMap<InternedPath, LocalId>,
    pub(super) functions_by_name: FxHashMap<InternedPath, FunctionId>,
    pub(super) structs_by_name: FxHashMap<InternedPath, StructId>,
    pub(super) fields_by_struct_and_name: FxHashMap<(StructId, InternedPath), FieldId>,
    pub(super) module_constants_by_name: FxHashMap<InternedPath, Declaration>,
    pub(super) currently_lowering_constants: FxHashSet<InternedPath>,

    // === Fast ID -> arena index maps ===
    pub(super) block_index_by_id: FxHashMap<BlockId, usize>,
    pub(super) function_index_by_id: FxHashMap<FunctionId, usize>,
    pub(super) region_index_by_id: FxHashMap<RegionId, usize>,
    pub(super) local_index_by_id: FxHashMap<LocalId, (usize, usize)>,
    pub(super) struct_index_by_id: FxHashMap<StructId, usize>,
    pub(super) field_index_by_id: FxHashMap<FieldId, (usize, usize)>,

    // === Current Function State ===
    current_function: Option<FunctionId>,
    current_block: Option<BlockId>,
    current_region: Option<RegionId>,
    pub(super) loop_targets: Vec<LoopTargets>,

    /// The runtime fragment vec local inside entry start(), if currently lowering it.
    /// Set when entering entry start() and cleared on leave.
    pub(super) entry_fragment_vec_local: Option<LocalId>,
}

// WHAT: generates a typed `allocate_*_id` method for each HIR entity kind.
// WHY: all nine allocators share identical logic — bump a u32 counter, wrap in a newtype, return.
//      A module-level macro eliminates the repetition without changing the public API.
//      To add a new entity type: add the counter field to HirBuilder, then invoke this macro.
macro_rules! allocate_id {
    ($method:ident, $counter_field:ident, $id_type:ident) => {
        pub(crate) fn $method(&mut self) -> $id_type {
            let id = $id_type(self.$counter_field);
            self.$counter_field += 1;
            id
        }
    };
}

impl<'a> HirBuilder<'a> {
    // -----------
    // Constructor
    // -----------
    pub fn new(
        string_table: &'a mut StringTable,
        path_format_config: PathStringFormatConfig,
    ) -> HirBuilder<'a> {
        HirBuilder {
            module: HirModule::new(),

            string_table,
            path_format_config,

            next_block_id: 0,
            next_local_id: 0,
            next_node_id: 0,
            next_value_id: 0,
            next_region_id: 0,
            next_function_id: 0,
            next_struct_id: 0,
            next_field_id: 0,
            next_const_id: 0,
            temp_local_counter: 0,
            template_function_counter: 0,

            type_context: TypeContext::default(),
            type_interner: FxHashMap::default(),
            side_table: HirSideTable::default(),

            locals_by_name: FxHashMap::default(),
            functions_by_name: FxHashMap::default(),
            structs_by_name: FxHashMap::default(),
            fields_by_struct_and_name: FxHashMap::default(),
            module_constants_by_name: FxHashMap::default(),
            currently_lowering_constants: FxHashSet::default(),

            block_index_by_id: FxHashMap::default(),
            function_index_by_id: FxHashMap::default(),
            region_index_by_id: FxHashMap::default(),
            local_index_by_id: FxHashMap::default(),
            struct_index_by_id: FxHashMap::default(),
            field_index_by_id: FxHashMap::default(),

            current_function: None,
            current_block: None,
            current_region: None,
            loop_targets: vec![],
            entry_fragment_vec_local: None,
        }
    }

    fn lower_error_messages(&self, error: CompilerError) -> CompilerMessages {
        CompilerMessages::from_error_with_warnings(
            error,
            self.module.warnings.to_owned(),
            self.string_table,
        )
    }

    // ========================================================================
    // Main Build Method
    // ========================================================================
    /// Builds an HIR module from an AST.
    /// This is the main entry point for HIR generation.
    pub fn build_hir_module(mut self, ast: Ast) -> Result<HirModule, CompilerMessages> {
        self.module.warnings = ast.warnings.to_owned();
        self.module.rendered_path_usages = ast.rendered_path_usages.to_owned();

        if let Err(error) = self.prepare_hir_declarations(&ast) {
            return Err(self.lower_error_messages(error));
        }

        if let Err(error) = self.lower_module_constants(&ast) {
            return Err(self.lower_error_messages(error));
        }

        if let Err(error) = self.resolve_doc_fragments(&ast) {
            return Err(self.lower_error_messages(error));
        }

        for node in &ast.nodes {
            if let Err(error) = self.process_ast_node(node) {
                return Err(self.lower_error_messages(error));
            }
        }

        if let Err(error) = self.assign_function_origins() {
            return Err(self.lower_error_messages(error));
        }

        let warnings = self.module.warnings.to_owned();
        let string_table = &*self.string_table;
        self.module.type_context = self.type_context;
        self.module.side_table = self.side_table;

        if let Err(error) = validate_hir_module(&self.module) {
            return Err(CompilerMessages::from_error_with_warnings(
                error,
                warnings,
                string_table,
            ));
        }

        Ok(self.module)
    }

    fn assign_function_origins(&mut self) -> Result<(), CompilerError> {
        // WHAT: classify every lowered function with a semantic origin tag.
        // WHY: downstream lowering needs explicit role data to avoid heuristic drift.
        self.module.function_origins.clear();

        for function in &self.module.functions {
            self.module
                .function_origins
                .insert(function.id, HirFunctionOrigin::Normal);
        }

        self.module
            .function_origins
            .insert(self.module.start_function, HirFunctionOrigin::EntryStart);

        Ok(())
    }

    fn resolve_doc_fragments(&mut self, ast: &Ast) -> Result<(), CompilerError> {
        self.module.doc_fragments.clear();

        for fragment in &ast.doc_fragments {
            let kind = match fragment.kind {
                AstDocFragmentKind::Doc => HirDocFragmentKind::Doc,
            };

            self.module.doc_fragments.push(HirDocFragment {
                kind,
                rendered_text: self.string_table.resolve(fragment.value).to_owned(),
                location: fragment.location.to_owned(),
            });
        }

        Ok(())
    }

    /// Processes a single AST node and generates corresponding HIR.
    fn process_ast_node(&mut self, node: &AstNode) -> Result<(), CompilerError> {
        self.lower_top_level_node(node)
    }

    allocate_id!(allocate_block_id, next_block_id, BlockId);
    allocate_id!(allocate_function_id, next_function_id, FunctionId);
    allocate_id!(allocate_region_id, next_region_id, RegionId);
    allocate_id!(allocate_local_id, next_local_id, LocalId);
    allocate_id!(allocate_node_id, next_node_id, HirNodeId);
    allocate_id!(allocate_value_id, next_value_id, HirValueId);
    allocate_id!(allocate_struct_id, next_struct_id, StructId);
    allocate_id!(allocate_field_id, next_field_id, FieldId);
    allocate_id!(allocate_const_id, next_const_id, HirConstId);

    #[cfg(test)]
    fn advance_counter_past(next_counter: &mut u32, used_id: u32) {
        *next_counter = (*next_counter).max(used_id.saturating_add(1));
    }

    #[cfg(test)]
    pub(crate) fn reserve_block_id(&mut self, block_id: BlockId) {
        Self::advance_counter_past(&mut self.next_block_id, block_id.0);
    }

    #[cfg(test)]
    pub(crate) fn reserve_region_id(&mut self, region_id: RegionId) {
        Self::advance_counter_past(&mut self.next_region_id, region_id.0);
    }

    #[cfg(test)]
    pub(crate) fn reserve_local_id(&mut self, local_id: LocalId) {
        Self::advance_counter_past(&mut self.next_local_id, local_id.0);
    }

    #[cfg(test)]
    pub(crate) fn reserve_function_id(&mut self, function_id: FunctionId) {
        Self::advance_counter_past(&mut self.next_function_id, function_id.0);
    }

    #[cfg(test)]
    pub(crate) fn reserve_struct_id(&mut self, struct_id: StructId) {
        Self::advance_counter_past(&mut self.next_struct_id, struct_id.0);
    }

    #[cfg(test)]
    pub(crate) fn reserve_field_id(&mut self, field_id: FieldId) {
        Self::advance_counter_past(&mut self.next_field_id, field_id.0);
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

    pub(super) fn push_struct(
        &mut self,
        hir_struct: crate::compiler_frontend::hir::hir_nodes::HirStruct,
    ) {
        let struct_index = self.module.structs.len();
        self.struct_index_by_id.insert(hir_struct.id, struct_index);

        for (field_index, field) in hir_struct.fields.iter().enumerate() {
            self.field_index_by_id
                .insert(field.id, (struct_index, field_index));
        }

        self.module.structs.push(hir_struct);
    }

    pub(super) fn register_local_in_block(
        &mut self,
        block_id: BlockId,
        local: crate::compiler_frontend::hir::hir_nodes::HirLocal,
        location: &SourceLocation,
    ) -> Result<(), CompilerError> {
        let block_index = self.block_index_or_error(block_id, location)?;
        let local_index = self.module.blocks[block_index].locals.len();
        self.local_index_by_id
            .insert(local.id, (block_index, local_index));
        self.module.blocks[block_index].locals.push(local);
        Ok(())
    }

    pub(super) fn local_type_id_or_error(
        &self,
        local_id: LocalId,
        location: &SourceLocation,
    ) -> Result<TypeId, CompilerError> {
        let Some((block_index, local_index)) = self.local_index_by_id.get(&local_id).copied()
        else {
            return_hir_transformation_error!(
                format!("Local {:?} is not registered in HIR blocks", local_id),
                location.clone()
            );
        };

        Ok(self.module.blocks[block_index].locals[local_index].ty)
    }

    pub(super) fn field_type_id_or_error(
        &self,
        field_id: FieldId,
        location: &SourceLocation,
    ) -> Result<TypeId, CompilerError> {
        let Some((struct_index, field_index)) = self.field_index_by_id.get(&field_id).copied()
        else {
            return_hir_transformation_error!(
                format!("Field {:?} is not registered in HIR structs", field_id),
                location.clone()
            );
        };

        Ok(self.module.structs[struct_index].fields[field_index].ty)
    }

    pub(super) fn block_index_or_error(
        &self,
        block_id: BlockId,
        location: &SourceLocation,
    ) -> Result<usize, CompilerError> {
        let Some(index) = self.block_index_by_id.get(&block_id).copied() else {
            return_hir_transformation_error!(
                format!("Block {:?} is not registered in HIR module", block_id),
                location.clone()
            );
        };

        Ok(index)
    }

    pub(super) fn function_index_or_error(
        &self,
        function_id: FunctionId,
        location: &SourceLocation,
    ) -> Result<usize, CompilerError> {
        let Some(index) = self.function_index_by_id.get(&function_id).copied() else {
            return_hir_transformation_error!(
                format!("Function {:?} is not registered in HIR module", function_id),
                location.clone()
            );
        };

        Ok(index)
    }

    pub(super) fn block_by_id_or_error(
        &self,
        block_id: BlockId,
        location: &SourceLocation,
    ) -> Result<&HirBlock, CompilerError> {
        let index = self.block_index_or_error(block_id, location)?;
        Ok(&self.module.blocks[index])
    }

    pub(super) fn block_mut_by_id_or_error(
        &mut self,
        block_id: BlockId,
        location: &SourceLocation,
    ) -> Result<&mut HirBlock, CompilerError> {
        let index = self.block_index_or_error(block_id, location)?;
        Ok(&mut self.module.blocks[index])
    }

    pub(super) fn function_by_id_or_error(
        &self,
        function_id: FunctionId,
        location: &SourceLocation,
    ) -> Result<&HirFunction, CompilerError> {
        let index = self.function_index_or_error(function_id, location)?;
        Ok(&self.module.functions[index])
    }

    pub(super) fn function_mut_by_id_or_error(
        &mut self,
        function_id: FunctionId,
        location: &SourceLocation,
    ) -> Result<&mut HirFunction, CompilerError> {
        let index = self.function_index_or_error(function_id, location)?;
        Ok(&mut self.module.functions[index])
    }

    pub(crate) fn enter_function(
        &mut self,
        function_id: FunctionId,
        location: &SourceLocation,
    ) -> Result<(), CompilerError> {
        let entry_block = self.function_by_id_or_error(function_id, location)?.entry;

        self.current_function = Some(function_id);
        self.locals_by_name.clear();
        self.loop_targets.clear();
        self.set_current_block(entry_block, location)
    }

    pub(crate) fn leave_function(&mut self) {
        self.current_function = None;
        self.current_block = None;
        self.current_region = None;
        self.locals_by_name.clear();
        self.loop_targets.clear();
        self.entry_fragment_vec_local = None;
    }

    pub(crate) fn set_current_block(
        &mut self,
        block_id: BlockId,
        location: &SourceLocation,
    ) -> Result<(), CompilerError> {
        let region = self.block_by_id_or_error(block_id, location)?.region;
        self.current_block = Some(block_id);
        self.current_region = Some(region);
        Ok(())
    }

    pub(crate) fn current_block_id_or_error(
        &self,
        location: &SourceLocation,
    ) -> Result<BlockId, CompilerError> {
        let Some(block_id) = self.current_block else {
            return_hir_transformation_error!("No current HIR block is active", location.clone());
        };

        Ok(block_id)
    }

    #[cfg(test)]
    pub(crate) fn current_block_id(&self) -> Option<BlockId> {
        self.current_block
    }

    pub(crate) fn current_function_id_or_error(
        &self,
        location: &SourceLocation,
    ) -> Result<FunctionId, CompilerError> {
        let Some(function_id) = self.current_function else {
            return_hir_transformation_error!(
                "No current HIR function is active",
                self.hir_error_location(location)
            );
        };

        Ok(function_id)
    }

    pub(crate) fn current_region_or_error(
        &self,
        location: &SourceLocation,
    ) -> Result<RegionId, CompilerError> {
        let Some(region) = self.current_region else {
            return_hir_transformation_error!(
                "No current HIR region is active",
                self.hir_error_location(location)
            );
        };

        Ok(region)
    }

    #[cfg(test)]
    pub(crate) fn set_current_function_for_tests(&mut self, function_id: FunctionId) {
        self.current_function = Some(function_id);
    }

    #[cfg(test)]
    pub(crate) fn set_current_block_for_tests(&mut self, block_id: BlockId) {
        self.current_block = Some(block_id);
    }

    #[cfg(test)]
    pub(crate) fn set_current_region_for_tests(&mut self, region: RegionId) {
        self.current_region = Some(region);
    }

    pub(crate) fn set_block_terminator(
        &mut self,
        block_id: BlockId,
        terminator: HirTerminator,
        source_location: &SourceLocation,
    ) -> Result<(), CompilerError> {
        {
            let block = self.block_mut_by_id_or_error(block_id, source_location)?;
            if !Self::is_placeholder_terminator(&block.terminator) {
                return_hir_transformation_error!(
                    format!("Block {} already has an explicit terminator", block_id),
                    source_location.clone()
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
        location: &SourceLocation,
    ) -> Result<bool, CompilerError> {
        let block = self.block_by_id_or_error(block_id, location)?;
        Ok(!Self::is_placeholder_terminator(&block.terminator))
    }

    fn is_placeholder_terminator(terminator: &HirTerminator) -> bool {
        matches!(terminator, HirTerminator::Panic { message: None })
    }

    pub(super) fn symbol_name_for_diagnostics(&self, symbol: &InternedPath) -> String {
        symbol
            .name_str(self.string_table)
            .map(str::to_owned)
            .unwrap_or_else(|| symbol.to_string(self.string_table))
    }
}
