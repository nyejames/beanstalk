//! Shared HIR builder test hooks and validation helpers.
//!
//! WHAT: exposes extra builder utilities needed only by HIR unit tests.
//! WHY: tests need direct access to internal builder state without widening the production API.

use crate::compiler_frontend::ast::ast_nodes::{Declaration, SourceLocation};
use crate::compiler_frontend::ast::expressions::expression::Expression;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::hir::hir_builder::HirBuilder;
use crate::compiler_frontend::hir::hir_nodes::{
    BlockId, FieldId, FunctionId, HirBlock, HirField, HirLocal, HirModule, HirStruct, RegionId,
    StructId,
};
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::StringTable;

pub(crate) fn validate_module_for_tests(
    module: &HirModule,
    _string_table: &StringTable,
) -> Result<(), CompilerError> {
    super::validate_hir_module(module)
}

impl<'a> HirBuilder<'a> {
    pub(crate) fn test_push_block(&mut self, block: HirBlock) {
        self.reserve_block_id(block.id);
        self.reserve_region_id(block.region);
        self.push_block(block);
    }

    pub(crate) fn test_set_current_region(&mut self, region: RegionId) {
        self.set_current_region_for_tests(region);
    }

    pub(crate) fn test_set_current_block(&mut self, block_id: BlockId) {
        self.set_current_block_for_tests(block_id);
    }

    pub(crate) fn test_set_current_function(&mut self, function_id: FunctionId) {
        self.set_current_function_for_tests(function_id);
    }

    pub(crate) fn test_register_local_in_block(&mut self, local: HirLocal, name: InternedPath) {
        let current_block = self.current_block_id().unwrap_or(BlockId(0));
        let _ =
            self.register_local_in_block(current_block, local.clone(), &SourceLocation::default());

        self.locals_by_name.insert(name.clone(), local.id);
        self.side_table.bind_local_name(local.id, name);
        self.side_table.map_local_source(&local);
        self.reserve_local_id(local.id);
    }

    pub(crate) fn test_register_function_name(&mut self, name: InternedPath, id: FunctionId) {
        self.functions_by_name.insert(name.clone(), id);
        self.side_table.bind_function_name(id, name);
        self.reserve_function_id(id);
    }

    pub(crate) fn test_register_struct_with_fields(
        &mut self,
        struct_id: StructId,
        name: InternedPath,
        fields: Vec<(
            FieldId,
            InternedPath,
            crate::compiler_frontend::hir::hir_datatypes::TypeId,
        )>,
    ) {
        self.structs_by_name.insert(name.clone(), struct_id);
        self.side_table.bind_struct_name(struct_id, name);

        let mut hir_fields = Vec::with_capacity(fields.len());
        for (field_id, field_name, ty) in fields {
            self.fields_by_struct_and_name
                .insert((struct_id, field_name.clone()), field_id);
            self.side_table.bind_field_name(field_id, field_name);
            hir_fields.push(HirField { id: field_id, ty });
            self.reserve_field_id(field_id);
        }

        self.push_struct(HirStruct {
            id: struct_id,
            fields: hir_fields,
        });
        self.reserve_struct_id(struct_id);
    }

    pub(crate) fn test_register_module_constant(&mut self, name: InternedPath, value: Expression) {
        self.module_constants_by_name
            .insert(name.to_owned(), Declaration { id: name, value });
    }
}
