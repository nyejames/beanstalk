#![cfg(test)]

use super::*;
use crate::compiler_frontend::ast::ast_nodes::{AstNode, Declaration, NodeKind};
use crate::compiler_frontend::ast::expressions::expression::Expression;
use crate::compiler_frontend::ast::statements::functions::FunctionSignature;
use crate::compiler_frontend::ast::templates::create_template_node::Template;
use crate::compiler_frontend::ast::templates::template::TemplateType;
use crate::compiler_frontend::datatypes::{DataType, Ownership};
use crate::compiler_frontend::headers::parse_file_headers::{
    TopLevelTemplateItem, TopLevelTemplateKind,
};
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::TextLocation;
use crate::projects::settings::{IMPLICIT_START_FUNC_NAME, TOP_LEVEL_TEMPLATE_NAME};
use rustc_hash::FxHashMap;

fn test_location(line: i32) -> TextLocation {
    TextLocation::new_just_line(line)
}

fn declaration(id: InternedPath, value: Expression) -> Declaration {
    Declaration { id, value }
}

fn top_level_template_declaration(
    content: Vec<Expression>,
    template_kind: TemplateType,
    location: TextLocation,
    string_table: &mut StringTable,
) -> Declaration {
    let mut template = Template::create_default(vec![]);
    template.kind = template_kind;
    template.location = location.to_owned();

    for expression in content {
        template.content.add(expression, false);
    }

    declaration(
        InternedPath::from_single_str(TOP_LEVEL_TEMPLATE_NAME, string_table),
        Expression::template(template, Ownership::ImmutableOwned),
    )
}

fn start_function_node(
    entry_dir: &InternedPath,
    body: Vec<AstNode>,
    location: TextLocation,
    string_table: &mut StringTable,
) -> AstNode {
    AstNode {
        kind: NodeKind::Function(
            entry_dir.join_str(IMPLICIT_START_FUNC_NAME, string_table),
            FunctionSignature {
                parameters: vec![],
                returns: vec![DataType::StringSlice],
            },
            body,
        ),
        location,
        scope: entry_dir.to_owned(),
    }
}

fn variable_declaration_node(
    id: InternedPath,
    value: Expression,
    location: TextLocation,
    scope: InternedPath,
) -> AstNode {
    AstNode {
        kind: NodeKind::VariableDeclaration(declaration(id, value)),
        location,
        scope,
    }
}

fn assignment_node(
    target: AstNode,
    value: Expression,
    location: TextLocation,
    scope: InternedPath,
) -> AstNode {
    AstNode {
        kind: NodeKind::Assignment {
            target: Box::new(target),
            value,
        },
        location,
        scope,
    }
}

fn find_function<'a>(ast_nodes: &'a [AstNode], name: &InternedPath) -> &'a AstNode {
    ast_nodes
        .iter()
        .find(|node| matches!(&node.kind, NodeKind::Function(function_name, _, _) if function_name == name))
        .expect("expected function node to exist")
}

#[test]
fn extracts_runtime_template_declarations_and_builds_runtime_fragment_wrapper() {
    let mut string_table = StringTable::new();
    let entry_dir = InternedPath::from_single_str("main.bst", &mut string_table);
    let entry_scope = entry_dir.to_owned();

    let value_name = InternedPath::from_single_str("value", &mut string_table);
    let value_location = test_location(2);
    let template_location = test_location(3);

    let value_declaration = variable_declaration_node(
        value_name.to_owned(),
        Expression::int(7, value_location.to_owned(), Ownership::ImmutableOwned),
        value_location.to_owned(),
        entry_scope.to_owned(),
    );

    let runtime_template_declaration = variable_declaration_node(
        InternedPath::from_single_str(TOP_LEVEL_TEMPLATE_NAME, &mut string_table),
        top_level_template_declaration(
            vec![Expression::reference(
                value_name.to_owned(),
                DataType::Int,
                template_location.to_owned(),
                Ownership::ImmutableReference,
            )],
            TemplateType::StringFunction,
            template_location.to_owned(),
            &mut string_table,
        )
        .value,
        template_location.to_owned(),
        entry_scope.to_owned(),
    );

    let mut ast_nodes = vec![start_function_node(
        &entry_dir,
        vec![value_declaration, runtime_template_declaration],
        test_location(1),
        &mut string_table,
    )];

    let start_items = synthesize_start_template_items(
        &mut ast_nodes,
        &entry_dir,
        &[],
        &FxHashMap::default(),
        &mut string_table,
    )
    .expect("runtime template synthesis should succeed");

    assert_eq!(start_items.len(), 1);
    let generated_fragment_name = match &start_items[0] {
        AstStartTemplateItem::RuntimeStringFunction { function, .. } => function.to_owned(),
        _ => panic!("expected runtime fragment item"),
    };

    let entry_start_name = entry_dir.join_str(IMPLICIT_START_FUNC_NAME, &mut string_table);
    let entry_start = find_function(&ast_nodes, &entry_start_name);
    let NodeKind::Function(_, _, entry_body) = &entry_start.kind else {
        panic!("entry start node should be a function");
    };

    assert_eq!(entry_body.len(), 1);
    assert!(matches!(
        entry_body[0].kind,
        NodeKind::VariableDeclaration(_)
    ));
    assert!(!entry_body.iter().any(|statement| {
        matches!(
            &statement.kind,
            NodeKind::VariableDeclaration(declaration)
                if declaration
                    .id
                    .name_str(&string_table)
                    .is_some_and(|name| name == TOP_LEVEL_TEMPLATE_NAME)
        )
    }));

    let generated_fragment = find_function(&ast_nodes, &generated_fragment_name);
    let NodeKind::Function(_, _, generated_body) = &generated_fragment.kind else {
        panic!("generated fragment should be a function");
    };

    assert_eq!(generated_body.len(), 2);
    assert!(matches!(
        generated_body[0].kind,
        NodeKind::VariableDeclaration(_)
    ));
    assert!(matches!(generated_body[1].kind, NodeKind::Return(_)));
}

#[test]
fn folds_runtime_candidate_that_is_already_const_into_const_fragment() {
    let mut string_table = StringTable::new();
    let entry_dir = InternedPath::from_single_str("main.bst", &mut string_table);
    let entry_scope = entry_dir.to_owned();

    let folded_value = string_table.intern("<h1>Hello</h1>");
    let location = test_location(2);
    let template_declaration = variable_declaration_node(
        InternedPath::from_single_str(TOP_LEVEL_TEMPLATE_NAME, &mut string_table),
        top_level_template_declaration(
            vec![Expression::string_slice(
                folded_value,
                location.to_owned(),
                Ownership::ImmutableOwned,
            )],
            TemplateType::String,
            location.to_owned(),
            &mut string_table,
        )
        .value,
        location.to_owned(),
        entry_scope.to_owned(),
    );

    let mut ast_nodes = vec![start_function_node(
        &entry_dir,
        vec![template_declaration],
        test_location(1),
        &mut string_table,
    )];

    let start_items = synthesize_start_template_items(
        &mut ast_nodes,
        &entry_dir,
        &[],
        &FxHashMap::default(),
        &mut string_table,
    )
    .expect("const-folded runtime template should synthesize");

    assert_eq!(start_items.len(), 1);
    match &start_items[0] {
        AstStartTemplateItem::ConstString { value, .. } => {
            assert_eq!(string_table.resolve(*value), "<h1>Hello</h1>");
        }
        _ => panic!("expected const string fragment"),
    }

    assert_eq!(
        ast_nodes
            .iter()
            .filter(|node| matches!(node.kind, NodeKind::Function(_, _, _)))
            .count(),
        1
    );
}

#[test]
fn merges_const_and_runtime_fragments_in_source_order() {
    let mut string_table = StringTable::new();
    let entry_dir = InternedPath::from_single_str("main.bst", &mut string_table);
    let entry_scope = entry_dir.to_owned();
    let const_header_path =
        InternedPath::from_single_str("main.bst/#const_template0", &mut string_table);
    let const_value = string_table.intern("<meta charset=\"utf-8\">");

    let template_location = test_location(20);
    let runtime_template_declaration = variable_declaration_node(
        InternedPath::from_single_str(TOP_LEVEL_TEMPLATE_NAME, &mut string_table),
        top_level_template_declaration(
            vec![Expression::reference(
                InternedPath::from_single_str("title", &mut string_table),
                DataType::StringSlice,
                template_location.to_owned(),
                Ownership::ImmutableReference,
            )],
            TemplateType::StringFunction,
            template_location.to_owned(),
            &mut string_table,
        )
        .value,
        template_location.to_owned(),
        entry_scope.to_owned(),
    );

    let mut ast_nodes = vec![start_function_node(
        &entry_dir,
        vec![
            variable_declaration_node(
                InternedPath::from_single_str("title", &mut string_table),
                Expression::string_slice(
                    string_table.intern("Beanstalk"),
                    test_location(10),
                    Ownership::ImmutableOwned,
                ),
                test_location(10),
                entry_scope.to_owned(),
            ),
            runtime_template_declaration,
        ],
        test_location(1),
        &mut string_table,
    )];

    let mut const_templates_by_path = FxHashMap::default();
    const_templates_by_path.insert(const_header_path.to_owned(), const_value);

    let top_level_template_items = vec![TopLevelTemplateItem {
        file_order: 0,
        location: test_location(5),
        kind: TopLevelTemplateKind::ConstTemplate {
            header_path: const_header_path,
        },
    }];

    let start_items = synthesize_start_template_items(
        &mut ast_nodes,
        &entry_dir,
        &top_level_template_items,
        &const_templates_by_path,
        &mut string_table,
    )
    .expect("mixed const/runtime synthesis should succeed");

    assert_eq!(start_items.len(), 2);
    assert!(matches!(
        start_items[0],
        AstStartTemplateItem::ConstString { .. }
    ));
    assert!(matches!(
        start_items[1],
        AstStartTemplateItem::RuntimeStringFunction { .. }
    ));
}

#[test]
fn errors_when_const_template_lookup_is_missing() {
    let mut string_table = StringTable::new();
    let entry_dir = InternedPath::from_single_str("main.bst", &mut string_table);
    let missing_header_path =
        InternedPath::from_single_str("main.bst/#const_template0", &mut string_table);

    let mut ast_nodes = vec![start_function_node(
        &entry_dir,
        vec![],
        test_location(1),
        &mut string_table,
    )];

    let top_level_template_items = vec![TopLevelTemplateItem {
        file_order: 0,
        location: test_location(1),
        kind: TopLevelTemplateKind::ConstTemplate {
            header_path: missing_header_path,
        },
    }];

    let error = synthesize_start_template_items(
        &mut ast_nodes,
        &entry_dir,
        &top_level_template_items,
        &FxHashMap::default(),
        &mut string_table,
    )
    .expect_err("missing const template lookup should fail");

    assert!(error.msg.contains("Missing const template value"));
}

#[test]
fn rejects_mutable_reassignment_of_captured_symbol_before_runtime_template() {
    let mut string_table = StringTable::new();
    let entry_dir = InternedPath::from_single_str("main.bst", &mut string_table);
    let entry_scope = entry_dir.to_owned();
    let counter_name = InternedPath::from_single_str("counter", &mut string_table);

    let declare_counter = variable_declaration_node(
        counter_name.to_owned(),
        Expression::int(1, test_location(2), Ownership::ImmutableOwned),
        test_location(2),
        entry_scope.to_owned(),
    );

    let assign_counter = assignment_node(
        AstNode {
            kind: NodeKind::Rvalue(Expression::reference(
                counter_name.to_owned(),
                DataType::Int,
                test_location(3),
                Ownership::MutableReference,
            )),
            location: test_location(3),
            scope: entry_scope.to_owned(),
        },
        Expression::int(2, test_location(3), Ownership::ImmutableOwned),
        test_location(3),
        entry_scope.to_owned(),
    );

    let runtime_template = variable_declaration_node(
        InternedPath::from_single_str(TOP_LEVEL_TEMPLATE_NAME, &mut string_table),
        top_level_template_declaration(
            vec![Expression::reference(
                counter_name,
                DataType::Int,
                test_location(4),
                Ownership::ImmutableReference,
            )],
            TemplateType::StringFunction,
            test_location(4),
            &mut string_table,
        )
        .value,
        test_location(4),
        entry_scope.to_owned(),
    );

    let mut ast_nodes = vec![start_function_node(
        &entry_dir,
        vec![declare_counter, assign_counter, runtime_template],
        test_location(1),
        &mut string_table,
    )];

    let error = synthesize_start_template_items(
        &mut ast_nodes,
        &entry_dir,
        &[],
        &FxHashMap::default(),
        &mut string_table,
    )
    .expect_err("captured symbol reassignment should fail");

    assert!(error.msg.contains("do not support mutable reassignments"));
}

#[test]
fn errors_when_entry_start_function_is_missing() {
    let mut string_table = StringTable::new();
    let entry_dir = InternedPath::from_single_str("main.bst", &mut string_table);

    let mut ast_nodes = vec![];
    let error = synthesize_start_template_items(
        &mut ast_nodes,
        &entry_dir,
        &[],
        &FxHashMap::default(),
        &mut string_table,
    )
    .expect_err("missing entry start function should fail");

    assert!(error.msg.contains("Failed to find entry start function"));
}
