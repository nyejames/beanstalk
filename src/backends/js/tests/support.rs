//! Shared HIR builders and assertions for JavaScript backend tests.
//!
//! WHAT: keeps test fixture construction in one place while each sibling module owns a backend
//! concern. WHY: the JS backend tests build HIR directly, so duplicated constructors make behavior
//! harder to audit than a single stage-local support surface.

pub(super) use crate::backends::js::test_symbol_helpers::{
    expected_dev_field_name, expected_dev_function_name, expected_dev_local_name,
};
pub(super) use crate::backends::js::{JsLoweringConfig, lower_hir_to_js};
pub(super) use crate::compiler_frontend::analysis::borrow_checker::{
    BorrowCheckReport, BorrowStateSnapshot, LocalBorrowSnapshot, LocalMode,
};
pub(super) use crate::compiler_frontend::hir::hir_datatypes::{
    HirType, HirTypeKind, TypeContext, TypeId,
};
pub(super) use crate::compiler_frontend::hir::hir_nodes::{
    BlockId, FieldId, FunctionId, HirBinOp, HirBlock, HirExpression, HirExpressionKind, HirField,
    HirFunction, HirLocal, HirMatchArm, HirModule, HirNodeId, HirPattern, HirPlace, HirRegion,
    HirRelationalPatternOp, HirStatement, HirStatementKind, HirStruct, HirTerminator, LocalId,
    OptionVariant, RegionId, ResultVariant, StructId, ValueKind,
};
pub(super) use crate::compiler_frontend::host_functions::CallTarget;
pub(super) use crate::compiler_frontend::interned_path::InternedPath;
pub(super) use crate::compiler_frontend::symbols::string_interning::StringTable;
pub(super) use crate::compiler_frontend::tokenizer::tokens::{CharPosition, SourceLocation};

#[derive(Clone, Copy)]
pub(super) struct TypeIds {
    pub(super) unit: TypeId,
    pub(super) int: TypeId,
    pub(super) boolean: TypeId,
    pub(super) string: TypeId,
    pub(super) option_int: TypeId,
    pub(super) union_unit: TypeId,
}

pub(super) fn loc(start: i32) -> SourceLocation {
    SourceLocation {
        scope: InternedPath::new(),
        start_pos: CharPosition {
            line_number: start,
            char_column: 0,
        },
        end_pos: CharPosition {
            line_number: start,
            char_column: 120, // Arbitrary number
        },
    }
}

pub(super) fn build_type_context() -> (TypeContext, TypeIds) {
    let mut type_context = TypeContext::default();

    let unit = type_context.insert(HirType {
        kind: HirTypeKind::Unit,
    });
    let int = type_context.insert(HirType {
        kind: HirTypeKind::Int,
    });
    let boolean = type_context.insert(HirType {
        kind: HirTypeKind::Bool,
    });
    let string = type_context.insert(HirType {
        kind: HirTypeKind::String,
    });
    let option_int = type_context.insert(HirType {
        kind: HirTypeKind::Option { inner: int },
    });
    let union_unit = type_context.insert(HirType {
        kind: HirTypeKind::Union {
            variants: vec![unit, unit, unit],
        },
    });

    (
        type_context,
        TypeIds {
            unit,
            int,
            boolean,
            string,
            option_int,
            union_unit,
        },
    )
}

pub(super) fn expression(
    id: u32,
    kind: HirExpressionKind,
    ty: TypeId,
    region: RegionId,
    value_kind: ValueKind,
) -> HirExpression {
    HirExpression {
        id: crate::compiler_frontend::hir::hir_nodes::HirValueId(id),
        kind,
        ty,
        value_kind,
        region,
    }
}

pub(super) fn unit_expression(id: u32, ty: TypeId, region: RegionId) -> HirExpression {
    expression(
        id,
        HirExpressionKind::TupleConstruct { elements: vec![] },
        ty,
        region,
        ValueKind::Const,
    )
}

pub(super) fn int_expression(id: u32, value: i64, ty: TypeId, region: RegionId) -> HirExpression {
    expression(
        id,
        HirExpressionKind::Int(value),
        ty,
        region,
        ValueKind::Const,
    )
}

pub(super) fn bool_expression(id: u32, value: bool, ty: TypeId, region: RegionId) -> HirExpression {
    expression(
        id,
        HirExpressionKind::Bool(value),
        ty,
        region,
        ValueKind::Const,
    )
}

pub(super) fn string_expression(
    id: u32,
    value: &str,
    ty: TypeId,
    region: RegionId,
) -> HirExpression {
    expression(
        id,
        HirExpressionKind::StringLiteral(value.to_owned()),
        ty,
        region,
        ValueKind::Const,
    )
}

pub(super) fn statement(id: u32, kind: HirStatementKind, line: i32) -> HirStatement {
    HirStatement {
        id: crate::compiler_frontend::hir::hir_nodes::HirNodeId(id),
        kind,
        location: loc(line),
    }
}

pub(super) fn local(local_id: u32, ty: TypeId, region: RegionId) -> HirLocal {
    HirLocal {
        id: LocalId(local_id),
        ty,
        mutable: true,
        region,
        source_info: Some(loc(1)),
    }
}

pub(super) fn build_module(
    string_table: &mut StringTable,
    function_name: &str,
    blocks: Vec<HirBlock>,
    function: HirFunction,
    local_names: &[(LocalId, &str)],
    type_context: TypeContext,
) -> HirModule {
    let mut module = HirModule::new();
    let function_id = function.id;
    module.blocks = blocks;
    module.start_function = function_id;
    module.functions = vec![function];
    module.type_context = type_context;
    module.regions = vec![HirRegion::lexical(RegionId(0), None)];

    let function_path = InternedPath::from_single_str(function_name, string_table);
    module
        .side_table
        .bind_function_name(function_id, function_path.clone());

    for (local_id, local_name) in local_names {
        let local_path = InternedPath::from_single_str(local_name, string_table);
        module.side_table.bind_local_name(*local_id, local_path);
    }

    module
}

/// Builds and lowers a minimal single-function module with an empty body.
///
/// WHY: most prelude and identifier tests only need a module to exist so the prelude is emitted;
/// they do not care about the function body.
pub(super) fn lower_minimal_module(function_name: &str) -> String {
    let mut string_table = StringTable::new();
    let (type_context, types) = build_type_context();

    let block = HirBlock {
        id: BlockId(0),
        region: RegionId(0),
        locals: vec![],
        statements: vec![],
        terminator: HirTerminator::Return(unit_expression(0, types.unit, RegionId(0))),
    };

    let function = HirFunction {
        id: FunctionId(0),
        entry: BlockId(0),
        params: vec![],
        return_type: types.unit,
        return_aliases: vec![],
    };

    let module = build_module(
        &mut string_table,
        function_name,
        vec![block],
        function,
        &[],
        type_context,
    );

    lower_hir_to_js(
        &module,
        &BorrowCheckReport::default(),
        &string_table,
        JsLoweringConfig {
            pretty: true,
            emit_locations: false,
            auto_invoke_start: false,
        },
    )
    .expect("JS lowering should succeed")
    .source
}

pub(super) fn default_config() -> JsLoweringConfig {
    JsLoweringConfig {
        pretty: true,
        emit_locations: false,
        auto_invoke_start: false,
    }
}
