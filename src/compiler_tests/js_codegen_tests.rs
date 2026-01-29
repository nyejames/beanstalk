//! Unit tests for JavaScript codegen.

use crate::compiler::codegen::js::lower_hir_to_js;
use crate::compiler::datatypes::DataType;
use crate::compiler::hir::nodes::{
    BlockId, HirBlock, HirExpr, HirExprKind, HirKind, HirModule, HirNode, HirPlace, HirStmt,
    HirTerminator,
};
use crate::compiler::parsers::statements::functions::FunctionSignature;
use crate::compiler::parsers::tokenizer::tokens::TextLocation;
use crate::compiler::string_interning::StringTable;

fn make_block(id: BlockId, nodes: Vec<HirNode>) -> HirBlock {
    HirBlock {
        id,
        params: Vec::new(),
        nodes,
    }
}

fn make_node_id(counter: &mut usize) -> usize {
    let id = *counter;
    *counter += 1;
    id
}

fn make_return_node(counter: &mut usize) -> HirNode {
    HirNode {
        kind: HirKind::Terminator(HirTerminator::Return(Vec::new())),
        location: TextLocation::default(),
        id: make_node_id(counter),
    }
}

#[test]
fn js_codegen_exports_start_function() {
    let mut string_table = StringTable::new();
    let start_id = string_table.intern("start");
    let mut node_id = 0;

    let body_block = make_block(0, vec![make_return_node(&mut node_id)]);
    let signature = FunctionSignature {
        parameters: Vec::new(),
        returns: Vec::new(),
    };
    let function_node = HirNode {
        kind: HirKind::Stmt(HirStmt::FunctionDef {
            name: start_id,
            signature,
            body: 0,
        }),
        location: TextLocation::default(),
        id: make_node_id(&mut node_id),
    };

    let module = HirModule {
        blocks: vec![body_block],
        entry_block: 0,
        functions: vec![function_node],
        structs: Vec::new(),
    };

    let js = lower_hir_to_js(&module, &string_table).expect("JS codegen failed");
    assert!(js.source.contains("function start("));
    assert!(js.source.contains("export { start }"));
}

#[test]
fn js_codegen_emits_host_io_binding() {
    let mut string_table = StringTable::new();
    let start_id = string_table.intern("start");
    let io_id = string_table.intern("host_io_functions");
    let hello_id = string_table.intern("hello");
    let mut node_id = 0;

    let call_node = HirNode {
        kind: HirKind::Stmt(HirStmt::Call {
            target: io_id,
            args: vec![HirExpr {
                kind: HirExprKind::StringLiteral(hello_id),
                data_type: DataType::String,
                location: TextLocation::default(),
            }],
        }),
        location: TextLocation::default(),
        id: make_node_id(&mut node_id),
    };

    let body_block = make_block(0, vec![call_node, make_return_node(&mut node_id)]);

    let signature = FunctionSignature {
        parameters: Vec::new(),
        returns: Vec::new(),
    };
    let function_node = HirNode {
        kind: HirKind::Stmt(HirStmt::FunctionDef {
            name: start_id,
            signature,
            body: 0,
        }),
        location: TextLocation::default(),
        id: make_node_id(&mut node_id),
    };

    let module = HirModule {
        blocks: vec![body_block],
        entry_block: 0,
        functions: vec![function_node],
        structs: Vec::new(),
    };

    let js = lower_hir_to_js(&module, &string_table).expect("JS codegen failed");
    assert!(js.source.contains("function __bst_host_io_functions"));
    assert!(js.source.contains("__bst_host_io_functions(\"hello\")"));
}

#[test]
fn js_codegen_emits_template_results_map() {
    let mut string_table = StringTable::new();
    let start_id = string_table.intern("start");
    let template_id = string_table.intern("tmpl");
    let result_id = string_table.intern("result");
    let hello_id = string_table.intern("hello");
    let mut node_id = 0;

    let template_body = make_block(
        0,
        vec![
            HirNode {
                kind: HirKind::Stmt(HirStmt::ExprStmt(HirExpr {
                    kind: HirExprKind::StringLiteral(hello_id),
                    data_type: DataType::String,
                    location: TextLocation::default(),
                })),
                location: TextLocation::default(),
                id: make_node_id(&mut node_id),
            },
            make_return_node(&mut node_id),
        ],
    );

    let template_node = HirNode {
        kind: HirKind::Stmt(HirStmt::TemplateFn {
            name: template_id,
            params: Vec::new(),
            body: 0,
        }),
        location: TextLocation::default(),
        id: make_node_id(&mut node_id),
    };

    let runtime_call = HirNode {
        kind: HirKind::Stmt(HirStmt::RuntimeTemplateCall {
            template_fn: template_id,
            captures: Vec::new(),
            id: None,
        }),
        location: TextLocation::default(),
        id: make_node_id(&mut node_id),
    };

    let assign_node = HirNode {
        kind: HirKind::Stmt(HirStmt::Assign {
            target: HirPlace::Var(result_id),
            value: HirExpr {
                kind: HirExprKind::HeapString(template_id),
                data_type: DataType::String,
                location: TextLocation::default(),
            },
            is_mutable: true,
        }),
        location: TextLocation::default(),
        id: make_node_id(&mut node_id),
    };

    let start_body = make_block(
        1,
        vec![runtime_call, assign_node, make_return_node(&mut node_id)],
    );

    let signature = FunctionSignature {
        parameters: Vec::new(),
        returns: Vec::new(),
    };
    let start_node = HirNode {
        kind: HirKind::Stmt(HirStmt::FunctionDef {
            name: start_id,
            signature,
            body: 1,
        }),
        location: TextLocation::default(),
        id: make_node_id(&mut node_id),
    };

    let module = HirModule {
        blocks: vec![template_body, start_body],
        entry_block: 1,
        functions: vec![template_node, start_node],
        structs: Vec::new(),
    };

    let js = lower_hir_to_js(&module, &string_table).expect("JS codegen failed");
    assert!(js.source.contains("const __bst_template_results"));
    assert!(js.source.contains("__bst_template_results[\"tmpl\"]"));
    assert!(js.source.contains("let __bst_out = \"\";"));
}

#[test]
fn js_codegen_emits_range_loop_state() {
    let mut string_table = StringTable::new();
    let start_id = string_table.intern("start");
    let item_id = string_table.intern("item");
    let mut node_id = 0;

    let loop_term = HirNode {
        kind: HirKind::Terminator(HirTerminator::Loop {
            label: 2,
            binding: Some((item_id, DataType::Int)),
            iterator: Some(HirExpr {
                kind: HirExprKind::Range {
                    start: Box::new(HirExpr {
                        kind: HirExprKind::Int(0),
                        data_type: DataType::Int,
                        location: TextLocation::default(),
                    }),
                    end: Box::new(HirExpr {
                        kind: HirExprKind::Int(2),
                        data_type: DataType::Int,
                        location: TextLocation::default(),
                    }),
                },
                data_type: DataType::Range,
                location: TextLocation::default(),
            }),
            body: 1,
            index_binding: None,
        }),
        location: TextLocation::default(),
        id: make_node_id(&mut node_id),
    };

    let header_block = make_block(0, vec![loop_term]);
    let body_block = make_block(
        1,
        vec![
            HirNode {
                kind: HirKind::Stmt(HirStmt::ExprStmt(HirExpr {
                    kind: HirExprKind::Load(HirPlace::Var(item_id)),
                    data_type: DataType::Int,
                    location: TextLocation::default(),
                })),
                location: TextLocation::default(),
                id: make_node_id(&mut node_id),
            },
            HirNode {
                kind: HirKind::Terminator(HirTerminator::Continue { target: 1 }),
                location: TextLocation::default(),
                id: make_node_id(&mut node_id),
            },
        ],
    );
    let exit_block = make_block(2, vec![make_return_node(&mut node_id)]);

    let signature = FunctionSignature {
        parameters: Vec::new(),
        returns: Vec::new(),
    };
    let start_node = HirNode {
        kind: HirKind::Stmt(HirStmt::FunctionDef {
            name: start_id,
            signature,
            body: 0,
        }),
        location: TextLocation::default(),
        id: make_node_id(&mut node_id),
    };

    let module = HirModule {
        blocks: vec![header_block, body_block, exit_block],
        entry_block: 0,
        functions: vec![start_node],
        structs: Vec::new(),
    };

    let js = lower_hir_to_js(&module, &string_table).expect("JS codegen failed");
    assert!(js.source.contains("const __bst_loop_state"));
    assert!(js.source.contains("kind: \"range\""));
    assert!(js.source.contains("__bst_block = 0"));
}
