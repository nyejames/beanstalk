use crate::compiler_frontend::ast::ast_nodes::TextLocation;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::hir::hir_builder::HirBuilder;
use crate::compiler_frontend::hir::hir_nodes::{
    BlockId, FieldId, FunctionId, HirBlock, HirField, HirLocal, HirModule, HirStruct, RegionId,
    StructId,
};
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::string_interning::StringTable;

pub(crate) fn validate_module_for_tests(
    module: &HirModule,
    string_table: &StringTable,
) -> Result<(), CompilerError> {
    super::validate_hir_module(module, string_table)
}

impl<'a> HirBuilder<'a> {
    pub(crate) fn test_push_block(&mut self, block: HirBlock) {
        self.push_block(block);
    }

    pub(crate) fn test_set_current_region(&mut self, region: RegionId) {
        self.current_region = Some(region);
    }

    pub(crate) fn test_set_current_block(&mut self, block_id: BlockId) {
        self.current_block = Some(block_id);
    }

    pub(crate) fn test_register_local_in_block(&mut self, local: HirLocal, name: InternedPath) {
        let current_block = self.current_block.unwrap_or(BlockId(0));
        let _ = self
            .block_mut_by_id_or_error(current_block, &TextLocation::default())
            .map(|block| block.locals.push(local.clone()));

        self.locals_by_name.insert(name.clone(), local.id);
        self.side_table.bind_local_name(local.id, name);
        self.side_table.map_local_source(&local);

        if local.id.0 >= self.next_local_id {
            self.next_local_id = local.id.0 + 1;
        }
    }

    pub(crate) fn test_register_function_name(&mut self, name: InternedPath, id: FunctionId) {
        self.functions_by_name.insert(name.clone(), id);
        self.side_table.bind_function_name(id, name);

        if id.0 >= self.next_function_id {
            self.next_function_id = id.0 + 1;
        }
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

            if field_id.0 >= self.next_field_id {
                self.next_field_id = field_id.0 + 1;
            }
        }

        self.module.structs.push(HirStruct {
            id: struct_id,
            fields: hir_fields,
        });

        if struct_id.0 >= self.next_struct_id {
            self.next_struct_id = struct_id.0 + 1;
        }
    }
}
