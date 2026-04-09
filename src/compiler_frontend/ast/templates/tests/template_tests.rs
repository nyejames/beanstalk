use super::*;
use crate::compiler_frontend::ast::ast_nodes::{AstNode, Declaration, NodeKind};
use crate::compiler_frontend::ast::test_support::parse_single_file_ast;
use crate::compiler_frontend::ast::expressions::expression::Expression;
use crate::compiler_frontend::ast::statements::functions::{
    FunctionReturn, FunctionSignature, ReturnSlot,
};
use crate::compiler_frontend::ast::templates::template::{CommentDirectiveKind, TemplateType};
use crate::compiler_frontend::ast::templates::template_types::Template;
use crate::compiler_frontend::ast::templates::top_level_templates::{
    AstDocFragment, AstStartTemplateItem,
};
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::datatypes::{DataType, Ownership};
use crate::compiler_frontend::headers::parse_file_headers::{
    TopLevelTemplateItem, TopLevelTemplateKind,
};
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::paths::path_format::PathStringFormatConfig;
use crate::compiler_frontend::paths::path_resolution::ProjectPathResolver;
use crate::compiler_frontend::string_interning::{StringId, StringTable};
use crate::compiler_frontend::tokenizer::tokens::{CharPosition, SourceLocation};
use crate::projects::settings::{IMPLICIT_START_FUNC_NAME, TOP_LEVEL_TEMPLATE_NAME};
use rustc_hash::FxHashMap;

fn test_location(line: i32) -> SourceLocation {
    SourceLocation {
        scope: InternedPath::new(),
        start_pos: CharPosition {
            line_number: line,
            char_column: 0,
        },
        end_pos: CharPosition {
            line_number: line,
            char_column: 120, // Arbitrary number
        },
    }
}

fn declaration(id: InternedPath, value: Expression) -> Declaration {
    Declaration { id, value }
}

fn top_level_template_declaration(
    content: Vec<Expression>,
    template_kind: TemplateType,
    location: SourceLocation,
    string_table: &mut StringTable,
) -> Declaration {
    let mut template = Template::create_default(vec![]);
    template.kind = template_kind;
    template.location = location.to_owned();

    for expression in content {
        template.content.add(expression);
    }

    declaration(
        InternedPath::from_single_str(TOP_LEVEL_TEMPLATE_NAME, string_table),
        Expression::template(template, Ownership::ImmutableOwned),
    )
}

fn start_function_node(
    entry_dir: &InternedPath,
    body: Vec<AstNode>,
    location: SourceLocation,
    string_table: &mut StringTable,
) -> AstNode {
    AstNode {
        kind: NodeKind::Function(
            entry_dir.join_str(IMPLICIT_START_FUNC_NAME, string_table),
            FunctionSignature {
                parameters: vec![],
                returns: vec![ReturnSlot::success(FunctionReturn::Value(
                    DataType::StringSlice,
                ))],
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
    location: SourceLocation,
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
    location: SourceLocation,
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

fn return_node(values: Vec<Expression>, location: SourceLocation, scope: InternedPath) -> AstNode {
    AstNode {
        kind: NodeKind::Return(values),
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

fn test_project_path_resolver() -> ProjectPathResolver {
    let cwd = std::env::temp_dir();
    ProjectPathResolver::new(cwd.clone(), cwd, &[]).expect("test path resolver should be valid")
}

fn synthesize_start_template_items_for_tests(
    ast_nodes: &mut Vec<AstNode>,
    entry_dir: &InternedPath,
    top_level_template_items: &[TopLevelTemplateItem],
    const_templates_by_path: &FxHashMap<InternedPath, StringId>,
    string_table: &mut StringTable,
) -> Result<Vec<AstStartTemplateItem>, CompilerError> {
    let resolver = test_project_path_resolver();
    synthesize_start_template_items(
        ast_nodes,
        entry_dir,
        top_level_template_items,
        const_templates_by_path,
        &resolver,
        &PathStringFormatConfig::default(),
        string_table,
    )
}

fn collect_and_strip_comment_templates_for_tests(
    ast_nodes: &mut [AstNode],
    string_table: &mut StringTable,
) -> Result<Vec<AstDocFragment>, CompilerError> {
    let resolver = test_project_path_resolver();
    collect_and_strip_comment_templates(
        ast_nodes,
        &resolver,
        &PathStringFormatConfig::default(),
        string_table,
    )
}

#[test]
fn single_dynamic_runtime_template_is_lifted_into_runtime_fragment() {
    let source = r#"
rhs_capture |calls ~Int| -> Bool:
    calls = calls + 1
    return true
;

lhs = false
calls ~= 0
value = lhs and rhs_capture(~calls)

[:short_circuit_mutable_rhs_later_runtime_capture calls=[calls]]
"#;

    let (ast, mut string_table) = parse_single_file_ast(source);

    assert!(
        ast.start_template_items.iter().any(|item| matches!(
            item,
            AstStartTemplateItem::RuntimeStringFunction { .. }
        )),
        "single dynamic top-level template should lower into a runtime fragment"
    );

    let start_name = ast
        .entry_path
        .join_str(IMPLICIT_START_FUNC_NAME, &mut string_table);
    let start_function = find_function(&ast.nodes, &start_name);
    let NodeKind::Function(_, _, start_body) = &start_function.kind else {
        panic!("entry start function should exist");
    };

    assert!(
        !start_body.iter().any(|statement| matches!(
            &statement.kind,
            NodeKind::VariableDeclaration(declaration)
                if declaration
                    .id
                    .name_str(&string_table)
                    .is_some_and(|name| name == TOP_LEVEL_TEMPLATE_NAME)
        )),
        "entry start body should not retain top-level runtime template declarations"
    );
}

#[test]
fn extracts_runtime_template_returns_and_builds_runtime_fragment_wrapper() {
    let mut string_table = StringTable::new();
    let entry_dir = InternedPath::from_single_str("main.bst", &mut string_table);
    let entry_scope = entry_dir.to_owned();

    let value_name = InternedPath::from_single_str("value", &mut string_table);
    let value_location = test_location(2);
    let template_location = test_location(3);
    let fallback_return_location = test_location(4);
    let empty_string = string_table.get_or_intern(String::new());

    let value_declaration = variable_declaration_node(
        value_name.to_owned(),
        Expression::int(7, value_location.to_owned(), Ownership::ImmutableOwned),
        value_location.to_owned(),
        entry_scope.to_owned(),
    );

    let runtime_template_return = return_node(
        vec![
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
        ],
        template_location.to_owned(),
        entry_scope.to_owned(),
    );

    let fallback_empty_return = return_node(
        vec![Expression::string_slice(
            empty_string,
            fallback_return_location.to_owned(),
            Ownership::ImmutableOwned,
        )],
        fallback_return_location.to_owned(),
        entry_scope.to_owned(),
    );

    let mut ast_nodes = vec![start_function_node(
        &entry_dir,
        vec![
            value_declaration,
            runtime_template_return,
            fallback_empty_return.to_owned(),
        ],
        test_location(1),
        &mut string_table,
    )];

    let start_items = synthesize_start_template_items_for_tests(
        &mut ast_nodes,
        &entry_dir,
        &[],
        &FxHashMap::default(),
        &mut string_table,
    )
    .expect("runtime template return synthesis should succeed");

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
    assert!(matches!(entry_body[0].kind, NodeKind::Return(_)));

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

    let start_items = synthesize_start_template_items_for_tests(
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

    // Template-only captured declarations are pruned from start so wrapper hydration
    // owns the only execution path for runtime-fragment captures.
    assert!(entry_body.is_empty());
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
fn keeps_captured_declaration_when_later_non_template_statement_uses_it() {
    let mut string_table = StringTable::new();
    let entry_dir = InternedPath::from_single_str("main.bst", &mut string_table);
    let entry_scope = entry_dir.to_owned();

    let value_name = InternedPath::from_single_str("value", &mut string_table);
    let sink_name = InternedPath::from_single_str("sink", &mut string_table);
    let value_location = test_location(2);
    let sink_location = test_location(3);
    let template_location = test_location(4);

    let value_declaration = variable_declaration_node(
        value_name.to_owned(),
        Expression::int(7, value_location.to_owned(), Ownership::ImmutableOwned),
        value_location.to_owned(),
        entry_scope.to_owned(),
    );
    let sink_declaration = variable_declaration_node(
        sink_name,
        Expression::reference(
            value_name.to_owned(),
            DataType::Int,
            sink_location.to_owned(),
            Ownership::ImmutableReference,
        ),
        sink_location.to_owned(),
        entry_scope.to_owned(),
    );
    let runtime_template_declaration = variable_declaration_node(
        InternedPath::from_single_str(TOP_LEVEL_TEMPLATE_NAME, &mut string_table),
        top_level_template_declaration(
            vec![Expression::reference(
                value_name,
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
        vec![
            value_declaration,
            sink_declaration,
            runtime_template_declaration,
        ],
        test_location(1),
        &mut string_table,
    )];

    let start_items = synthesize_start_template_items_for_tests(
        &mut ast_nodes,
        &entry_dir,
        &[],
        &FxHashMap::default(),
        &mut string_table,
    )
    .expect("runtime template synthesis should succeed");

    assert_eq!(start_items.len(), 1);

    let entry_start_name = entry_dir.join_str(IMPLICIT_START_FUNC_NAME, &mut string_table);
    let entry_start = find_function(&ast_nodes, &entry_start_name);
    let NodeKind::Function(_, _, entry_body) = &entry_start.kind else {
        panic!("entry start node should be a function");
    };

    // `value` is captured by the runtime fragment, but it is also required by
    // a later non-template declaration (`sink = value`), so it must remain.
    assert_eq!(entry_body.len(), 2);
    assert!(matches!(
        entry_body[0].kind,
        NodeKind::VariableDeclaration(_)
    ));
    assert!(matches!(
        entry_body[1].kind,
        NodeKind::VariableDeclaration(_)
    ));
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

    let start_items = synthesize_start_template_items_for_tests(
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

    let start_items = synthesize_start_template_items_for_tests(
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

    let error = synthesize_start_template_items_for_tests(
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

    let error = synthesize_start_template_items_for_tests(
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
    let error = synthesize_start_template_items_for_tests(
        &mut ast_nodes,
        &entry_dir,
        &[],
        &FxHashMap::default(),
        &mut string_table,
    )
    .expect_err("missing entry start function should fail");

    assert!(error.msg.contains("Failed to find entry start function"));
}

#[test]
fn collects_and_strips_top_level_doc_comment_templates() {
    let mut string_table = StringTable::new();
    let entry_dir = InternedPath::from_single_str("main.bst", &mut string_table);
    let entry_scope = entry_dir.to_owned();
    let doc_location = test_location(2);
    let runtime_location = test_location(3);

    let doc_declaration = variable_declaration_node(
        InternedPath::from_single_str(TOP_LEVEL_TEMPLATE_NAME, &mut string_table),
        top_level_template_declaration(
            vec![Expression::string_slice(
                string_table.intern("doc"),
                doc_location.to_owned(),
                Ownership::ImmutableOwned,
            )],
            TemplateType::Comment(CommentDirectiveKind::Doc),
            doc_location.to_owned(),
            &mut string_table,
        )
        .value,
        doc_location.to_owned(),
        entry_scope.to_owned(),
    );

    let runtime_declaration = variable_declaration_node(
        InternedPath::from_single_str(TOP_LEVEL_TEMPLATE_NAME, &mut string_table),
        top_level_template_declaration(
            vec![Expression::string_slice(
                string_table.intern("runtime"),
                runtime_location.to_owned(),
                Ownership::ImmutableOwned,
            )],
            TemplateType::String,
            runtime_location.to_owned(),
            &mut string_table,
        )
        .value,
        runtime_location.to_owned(),
        entry_scope.to_owned(),
    );

    let mut ast_nodes = vec![start_function_node(
        &entry_dir,
        vec![doc_declaration, runtime_declaration],
        test_location(1),
        &mut string_table,
    )];

    let doc_fragments =
        collect_and_strip_comment_templates_for_tests(&mut ast_nodes, &mut string_table)
            .expect("doc comment collection should succeed");

    assert_eq!(doc_fragments.len(), 1);
    assert!(matches!(doc_fragments[0].kind, AstDocFragmentKind::Doc));
    assert_eq!(string_table.resolve(doc_fragments[0].value), "doc");

    let entry_start_name = entry_dir.join_str(IMPLICIT_START_FUNC_NAME, &mut string_table);
    let entry_start = find_function(&ast_nodes, &entry_start_name);
    let NodeKind::Function(_, _, body) = &entry_start.kind else {
        panic!("entry start should remain a function");
    };
    assert_eq!(
        body.len(),
        1,
        "top-level doc template should be stripped from runtime start body"
    );
}

#[test]
fn collects_nested_doc_fragments_in_source_order() {
    let mut string_table = StringTable::new();
    let entry_dir = InternedPath::from_single_str("main.bst", &mut string_table);
    let entry_scope = entry_dir.to_owned();

    let mut parent = Template::create_default(vec![]);
    parent.kind = TemplateType::Comment(CommentDirectiveKind::Doc);
    parent.location = test_location(2);
    parent.content.add(Expression::string_slice(
        string_table.intern("parent"),
        test_location(2),
        Ownership::ImmutableOwned,
    ));

    let mut child = Template::create_default(vec![]);
    child.kind = TemplateType::Comment(CommentDirectiveKind::Doc);
    child.location = test_location(3);
    child.content.add(Expression::string_slice(
        string_table.intern("child"),
        test_location(3),
        Ownership::ImmutableOwned,
    ));

    let mut grandchild = Template::create_default(vec![]);
    grandchild.kind = TemplateType::Comment(CommentDirectiveKind::Doc);
    grandchild.location = test_location(4);
    grandchild.content.add(Expression::string_slice(
        string_table.intern("grandchild"),
        test_location(4),
        Ownership::ImmutableOwned,
    ));

    child.doc_children.push(grandchild);
    parent.doc_children.push(child);

    let doc_declaration = variable_declaration_node(
        InternedPath::from_single_str(TOP_LEVEL_TEMPLATE_NAME, &mut string_table),
        Expression::template(parent, Ownership::ImmutableOwned),
        test_location(2),
        entry_scope.to_owned(),
    );

    let mut ast_nodes = vec![start_function_node(
        &entry_dir,
        vec![doc_declaration],
        test_location(1),
        &mut string_table,
    )];

    let doc_fragments =
        collect_and_strip_comment_templates_for_tests(&mut ast_nodes, &mut string_table)
            .expect("nested doc comment collection should succeed");

    assert_eq!(doc_fragments.len(), 3);
    assert_eq!(string_table.resolve(doc_fragments[0].value), "parent");
    assert_eq!(string_table.resolve(doc_fragments[1].value), "child");
    assert_eq!(string_table.resolve(doc_fragments[2].value), "grandchild");
}

#[test]
fn top_level_doc_comments_do_not_generate_start_fragments() {
    let mut string_table = StringTable::new();
    let entry_dir = InternedPath::from_single_str("main.bst", &mut string_table);
    let entry_scope = entry_dir.to_owned();

    let doc_declaration = variable_declaration_node(
        InternedPath::from_single_str(TOP_LEVEL_TEMPLATE_NAME, &mut string_table),
        top_level_template_declaration(
            vec![Expression::string_slice(
                string_table.intern("doc"),
                test_location(2),
                Ownership::ImmutableOwned,
            )],
            TemplateType::Comment(CommentDirectiveKind::Doc),
            test_location(2),
            &mut string_table,
        )
        .value,
        test_location(2),
        entry_scope.to_owned(),
    );

    let runtime_declaration = variable_declaration_node(
        InternedPath::from_single_str(TOP_LEVEL_TEMPLATE_NAME, &mut string_table),
        top_level_template_declaration(
            vec![Expression::string_slice(
                string_table.intern("runtime"),
                test_location(3),
                Ownership::ImmutableOwned,
            )],
            TemplateType::String,
            test_location(3),
            &mut string_table,
        )
        .value,
        test_location(3),
        entry_scope.to_owned(),
    );

    let mut ast_nodes = vec![start_function_node(
        &entry_dir,
        vec![doc_declaration, runtime_declaration],
        test_location(1),
        &mut string_table,
    )];

    let _ = collect_and_strip_comment_templates_for_tests(&mut ast_nodes, &mut string_table)
        .expect("doc comment stripping should succeed");
    let start_items = synthesize_start_template_items_for_tests(
        &mut ast_nodes,
        &entry_dir,
        &[],
        &FxHashMap::default(),
        &mut string_table,
    )
    .expect("start fragment synthesis should succeed");

    assert_eq!(start_items.len(), 1);
}
