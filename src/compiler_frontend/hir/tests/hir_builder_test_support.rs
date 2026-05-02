//! Shared HIR builder test hooks and validation helpers.
//!
//! WHAT: exposes extra builder utilities needed only by HIR unit tests.
//! WHY: tests need direct access to internal builder state without widening the production API.

use crate::compiler_frontend::ast::Ast;
use crate::compiler_frontend::ast::ast_nodes::{AstNode, Declaration, SourceLocation};
use crate::compiler_frontend::ast::expressions::expression::Expression;
use crate::compiler_frontend::compiler_errors::{CompilerError, CompilerMessages};
use crate::compiler_frontend::hir::blocks::{HirBlock, HirLocal};
use crate::compiler_frontend::hir::hir_builder::HirBuilder;
use crate::compiler_frontend::hir::hir_side_table::HirLocalOriginKind;
use crate::compiler_frontend::hir::ids::{BlockId, FieldId, FunctionId, RegionId, StructId};
use crate::compiler_frontend::hir::module::HirModule;
use crate::compiler_frontend::hir::structs::{HirField, HirStruct};
use crate::compiler_frontend::hir::terminators::HirTerminator;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::paths::path_format::PathStringFormatConfig;
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
        self.side_table
            .bind_local_origin(local.id, HirLocalOriginKind::User, None, None);
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

// ---------------------------------------------------------------------------
// Shared AST → HIR test helpers
// ---------------------------------------------------------------------------

/// Recursively scan a `DataType` for `Choices` definitions and collect them.
fn collect_choice_definitions_from_data_type(
    data_type: &crate::compiler_frontend::datatypes::DataType,
    out: &mut Vec<crate::compiler_frontend::ast::AstChoiceDefinition>,
) {
    use crate::compiler_frontend::datatypes::DataType;
    match data_type {
        DataType::Choices {
            nominal_path,
            variants,
        } => {
            if !out.iter().any(|c| &c.nominal_path == nominal_path) {
                out.push(crate::compiler_frontend::ast::AstChoiceDefinition {
                    nominal_path: nominal_path.to_owned(),
                    variants: variants.to_owned(),
                });
            }
            for variant in variants {
                if let crate::compiler_frontend::declaration_syntax::choice::ChoiceVariantPayload::Record { fields } = &variant.payload {
                    for field in fields {
                        collect_choice_definitions_from_data_type(&field.value.data_type, out);
                    }
                }
            }
        }
        DataType::Option(inner) | DataType::Reference(inner) => {
            collect_choice_definitions_from_data_type(inner, out);
        }
        DataType::GenericInstance { arguments, .. } => {
            for argument in arguments {
                collect_choice_definitions_from_data_type(argument, out);
            }
        }
        DataType::Returns(values) => {
            for value in values {
                collect_choice_definitions_from_data_type(value, out);
            }
        }
        DataType::Function(receiver, signature) => {
            if let Some(crate::compiler_frontend::datatypes::ReceiverKey::Struct(path)) =
                receiver.as_ref()
            {
                // No nested DataType in receiver key
                let _ = path;
            }
            for param in &signature.parameters {
                collect_choice_definitions_from_data_type(&param.value.data_type, out);
            }
            for ret in signature.success_returns() {
                collect_choice_definitions_from_data_type(ret.data_type(), out);
            }
            if let Some(err) = signature.error_return() {
                collect_choice_definitions_from_data_type(err.data_type(), out);
            }
        }
        DataType::Struct { fields, .. } | DataType::Parameters(fields) => {
            for field in fields {
                collect_choice_definitions_from_data_type(&field.value.data_type, out);
            }
        }
        _ => {}
    }
}

/// Extract all choice definitions referenced in AST node signatures.
fn extract_choice_definitions_from_nodes(
    nodes: &[AstNode],
) -> Vec<crate::compiler_frontend::ast::AstChoiceDefinition> {
    let mut defs = vec![];
    for node in nodes {
        match &node.kind {
            crate::compiler_frontend::ast::ast_nodes::NodeKind::Function(_, signature, _) => {
                for param in &signature.parameters {
                    collect_choice_definitions_from_data_type(&param.value.data_type, &mut defs);
                }
                for ret in signature.success_returns() {
                    collect_choice_definitions_from_data_type(ret.data_type(), &mut defs);
                }
                if let Some(err) = signature.error_return() {
                    collect_choice_definitions_from_data_type(err.data_type(), &mut defs);
                }
            }
            crate::compiler_frontend::ast::ast_nodes::NodeKind::StructDefinition(_, fields) => {
                for field in fields {
                    collect_choice_definitions_from_data_type(&field.value.data_type, &mut defs);
                }
            }
            _ => {}
        }
    }
    defs
}

/// Build a minimal `Ast` from nodes for HIR lowering tests.
pub(crate) fn build_ast(nodes: Vec<AstNode>, entry_path: InternedPath) -> Ast {
    let choice_definitions = extract_choice_definitions_from_nodes(&nodes);
    Ast {
        nodes,
        module_constants: vec![],
        doc_fragments: vec![],
        entry_path,
        const_top_level_fragments: vec![],
        rendered_path_usages: vec![],
        warnings: vec![],
        choice_definitions,
    }
}

/// Lower a test `Ast` into a `HirModule`.
pub(crate) fn lower_ast(
    ast: Ast,
    string_table: &mut StringTable,
) -> Result<HirModule, CompilerMessages> {
    HirBuilder::new(string_table, PathStringFormatConfig::default()).build_hir_module(ast)
}

/// Assert that no block ends with a placeholder `Panic(None)` terminator.
pub(crate) fn assert_no_placeholder_terminators(module: &HirModule) {
    assert!(
        module
            .blocks
            .iter()
            .all(|block| !matches!(block.terminator, HirTerminator::Panic { message: None })),
        "expected no placeholder Panic(None) terminators in lowered HIR"
    );
}
