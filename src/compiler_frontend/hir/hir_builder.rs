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
use crate::compiler_frontend::ast::ast::AstStartTemplateItem;
use crate::compiler_frontend::ast::ast_nodes::{AstNode, Declaration, SourceLocation};
use crate::compiler_frontend::ast::templates::template_folding::TemplateFoldContext;
use crate::compiler_frontend::compiler_errors::{CompilerError, CompilerMessages};
use crate::compiler_frontend::hir::hir_datatypes::{HirTypeKind, TypeContext, TypeId};
use crate::compiler_frontend::hir::hir_nodes::{
    BlockId, ConstStringId, FieldId, FunctionId, HirBlock, HirConstId, HirDocFragment,
    HirDocFragmentKind, HirFunction, HirFunctionOrigin, HirModule, HirNodeId, HirRegion,
    HirTerminator, HirValueId, LocalId, RegionId, StartFragment, StructId,
};
use crate::compiler_frontend::hir::hir_side_table::HirSideTable;
use crate::compiler_frontend::hir::hir_validation::validate_hir_module;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::paths::path_format::PathStringFormatConfig;
use crate::compiler_frontend::paths::path_resolution::ProjectPathResolver;
use crate::compiler_frontend::string_interning::StringTable;
use crate::projects::settings::IMPLICIT_START_FUNC_NAME;
use crate::return_hir_transformation_error;
use rustc_hash::{FxHashMap, FxHashSet};

// -----------
// Entry Point
// -----------
pub fn lower_module(
    ast: Ast,
    string_table: &mut StringTable,
    path_format_config: PathStringFormatConfig,
    project_path_resolver: ProjectPathResolver,
) -> Result<HirModule, CompilerMessages> {
    let ctx = HirBuilder::new(string_table, path_format_config, project_path_resolver);
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
    pub(super) project_path_resolver: ProjectPathResolver,

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
}

impl<'a> HirBuilder<'a> {
    // -----------
    // Constructor
    // -----------
    pub fn new(
        string_table: &'a mut StringTable,
        path_format_config: PathStringFormatConfig,
        project_path_resolver: ProjectPathResolver,
    ) -> HirBuilder<'a> {
        HirBuilder {
            module: HirModule::new(),

            string_table,
            path_format_config,
            project_path_resolver,

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
        }
    }

    pub(crate) fn new_template_fold_context<'b>(
        &'b mut self,
        source_file_scope: &'b InternedPath,
    ) -> TemplateFoldContext<'b> {
        TemplateFoldContext {
            string_table: self.string_table,
            project_path_resolver: &self.project_path_resolver,
            path_format_config: &self.path_format_config,
            source_file_scope,
        }
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
            return Err(CompilerMessages {
                errors: vec![error],
                warnings: self.module.warnings.to_owned(),
                string_table: Default::default(),
            });
        }

        if let Err(error) = self.lower_module_constants(&ast) {
            return Err(CompilerMessages {
                errors: vec![error],
                warnings: self.module.warnings.to_owned(),
                string_table: Default::default(),
            });
        }

        if let Err(error) = self.resolve_start_fragments(&ast) {
            return Err(CompilerMessages {
                errors: vec![error],
                warnings: self.module.warnings.to_owned(),
                string_table: Default::default(),
            });
        }

        if let Err(error) = self.resolve_doc_fragments(&ast) {
            return Err(CompilerMessages {
                errors: vec![error],
                warnings: self.module.warnings.to_owned(),
                string_table: Default::default(),
            });
        }

        for node in &ast.nodes {
            if let Err(error) = self.process_ast_node(node) {
                return Err(CompilerMessages {
                    errors: vec![error],
                    warnings: self.module.warnings.to_owned(),
                    string_table: Default::default(),
                });
            }
        }

        if let Err(error) = self.assign_function_origins() {
            return Err(CompilerMessages {
                errors: vec![error],
                warnings: self.module.warnings.to_owned(),
                string_table: Default::default(),
            });
        }

        self.module.type_context = self.type_context;
        self.module.side_table = self.side_table;

        if let Err(error) = validate_hir_module(&self.module, self.string_table) {
            return Err(CompilerMessages {
                errors: vec![error],
                warnings: self.module.warnings.to_owned(),
                string_table: Default::default(),
            });
        }

        Ok(self.module)
    }

    fn assign_function_origins(&mut self) -> Result<(), CompilerError> {
        // WHAT: classify every lowered function with a semantic origin tag.
        // WHY: downstream lowering needs explicit role data to avoid heuristic drift.
        self.module.function_origins.clear();

        // Default all functions to user-defined; override specific categories below.
        for function in &self.module.functions {
            self.module
                .function_origins
                .insert(function.id, HirFunctionOrigin::Normal);
        }

        self.module
            .function_origins
            .insert(self.module.start_function, HirFunctionOrigin::EntryStart);

        // Runtime template fragment functions come from ordered start fragments.
        for fragment in &self.module.start_fragments {
            if let StartFragment::RuntimeStringFn(function_id) = fragment {
                self.module
                    .function_origins
                    .insert(*function_id, HirFunctionOrigin::RuntimeTemplate);
            }
        }

        for function in &self.module.functions {
            let Some(function_path) = self.side_table.function_name_path(function.id) else {
                return_hir_transformation_error!(
                    format!(
                        "Missing function symbol path for {:?} while assigning function origins",
                        function.id
                    ),
                    SourceLocation::default()
                );
            };

            let is_implicit_start = function_path
                .name_str(self.string_table)
                .map(|name| name == IMPLICIT_START_FUNC_NAME)
                .unwrap_or(false);
            if !is_implicit_start {
                continue;
            }

            if matches!(
                self.module.function_origins.get(&function.id),
                Some(HirFunctionOrigin::EntryStart | HirFunctionOrigin::RuntimeTemplate)
            ) {
                continue;
            }

            // Remaining implicit-start functions belong to imported files.
            self.module
                .function_origins
                .insert(function.id, HirFunctionOrigin::FileStart);
        }

        Ok(())
    }

    fn resolve_start_fragments(&mut self, ast: &Ast) -> Result<(), CompilerError> {
        self.module.start_fragments.clear();
        self.module.const_string_pool.clear();

        for template_item in &ast.start_template_items {
            match template_item {
                AstStartTemplateItem::ConstString { value, .. } => {
                    let const_string_id = ConstStringId(self.module.const_string_pool.len() as u32);
                    self.module
                        .const_string_pool
                        .push(self.string_table.resolve(*value).to_owned());
                    self.module
                        .start_fragments
                        .push(StartFragment::ConstString(const_string_id));
                }

                AstStartTemplateItem::RuntimeStringFunction { function, location } => {
                    let function_id = self.resolve_function_id_or_error(function, location)?;
                    self.module
                        .start_fragments
                        .push(StartFragment::RuntimeStringFn(function_id));
                }
            }
        }

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

    pub(crate) fn allocate_local_id(&mut self) -> LocalId {
        let id = LocalId(self.next_local_id);
        self.next_local_id += 1;
        id
    }

    pub(crate) fn allocate_node_id(&mut self) -> HirNodeId {
        let id = HirNodeId(self.next_node_id);
        self.next_node_id += 1;
        id
    }

    pub(crate) fn allocate_value_id(&mut self) -> HirValueId {
        let id = HirValueId(self.next_value_id);
        self.next_value_id += 1;
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

    pub(crate) fn allocate_const_id(&mut self) -> HirConstId {
        let id = HirConstId(self.next_const_id);
        self.next_const_id += 1;
        id
    }

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
