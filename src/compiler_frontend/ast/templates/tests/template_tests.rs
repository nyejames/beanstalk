use super::*;
use crate::compiler_frontend::ast::ast_nodes::{AstNode, NodeKind};
use crate::compiler_frontend::ast::expressions::expression::{Expression, ExpressionKind};
use crate::compiler_frontend::ast::statements::functions::{
    FunctionReturn, FunctionSignature, ReturnSlot,
};
use crate::compiler_frontend::ast::templates::template::{CommentDirectiveKind, TemplateType};
use crate::compiler_frontend::ast::templates::template_types::Template;
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::paths::path_format::PathStringFormatConfig;
use crate::compiler_frontend::paths::path_resolution::ProjectPathResolver;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tests::test_support::{
    parse_single_file_ast, parse_single_file_ast_error,
};
use crate::compiler_frontend::tokenizer::tokens::{CharPosition, SourceLocation};
use crate::compiler_frontend::value_mode::ValueMode;
use crate::projects::settings::IMPLICIT_START_FUNC_NAME;

fn test_location(line: i32) -> SourceLocation {
    SourceLocation {
        scope: InternedPath::new(),
        start_pos: CharPosition {
            line_number: line,
            char_column: 0,
        },
        end_pos: CharPosition {
            line_number: line,
            char_column: 120,
        },
    }
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

fn push_start_runtime_fragment_node(
    template: Template,
    location: SourceLocation,
    scope: InternedPath,
) -> AstNode {
    AstNode {
        kind: NodeKind::PushStartRuntimeFragment(Expression::template(
            template,
            ValueMode::ImmutableOwned,
        )),
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
    ProjectPathResolver::new(
        cwd.clone(),
        cwd,
        &[],
        &crate::libraries::SourceLibraryRegistry::default(),
    )
    .expect("test path resolver should be valid")
}

fn collect_and_strip_comment_templates_for_tests(
    ast_nodes: &mut [AstNode],
    string_table: &mut StringTable,
) -> Result<Vec<AstDocFragment>, crate::compiler_frontend::compiler_errors::CompilerError> {
    let resolver = test_project_path_resolver();
    collect_and_strip_comment_templates(
        ast_nodes,
        &resolver,
        &PathStringFormatConfig::default(),
        string_table,
    )
}

#[test]
fn standalone_insert_helper_value_is_rejected_after_composition() {
    let source = r#"
value = [$insert("style"): color: red;]
"#;

    let error = parse_single_file_ast_error(source);

    assert!(
        error.msg.contains(
            "Template helper reached AST finalization outside immediate wrapper-slot composition."
        ) || error
            .msg
            .contains("'$insert(...)' can only be used while filling an immediate parent template"),
        "expected escaped helper failure, got: {}",
        error.msg
    );
}

#[test]
fn finalized_module_constants_materialize_const_templates_before_hir() {
    let source = r#"
#wrapper = [:<div class="frame">[$slot]</div>]
#content = [wrapper: [:Hello]]
"#;

    let (ast, string_table) = parse_single_file_ast(source);

    let wrapper = ast
        .module_constants
        .iter()
        .find(|declaration| declaration.id.name_str(&string_table) == Some("wrapper"))
        .expect("wrapper constant should exist");
    let content = ast
        .module_constants
        .iter()
        .find(|declaration| declaration.id.name_str(&string_table) == Some("content"))
        .expect("content constant should exist");

    let ExpressionKind::StringSlice(wrapper_value) = &wrapper.value.kind else {
        panic!("wrapper template should already be materialized before HIR");
    };
    let ExpressionKind::StringSlice(content_value) = &content.value.kind else {
        panic!("const template application should already be materialized before HIR");
    };

    assert_eq!(
        string_table.resolve(*wrapper_value),
        "<div class=\"frame\"></div>"
    );
    assert_eq!(
        string_table.resolve(*content_value),
        "<div class=\"frame\"> Hello</div>"
    );
}

#[test]
fn collects_and_strips_top_level_doc_comment_templates() {
    let mut string_table = StringTable::new();
    let entry_dir = InternedPath::from_single_str("main.bst", &mut string_table);
    let entry_scope = entry_dir.to_owned();
    let doc_location = test_location(2);
    let runtime_location = test_location(3);

    let mut doc_template = Template::empty();
    doc_template.kind = TemplateType::Comment(CommentDirectiveKind::Doc);
    doc_template.location = doc_location.clone();
    doc_template.content.add(Expression::string_slice(
        string_table.intern("doc"),
        doc_location.clone(),
        ValueMode::ImmutableOwned,
    ));

    let mut runtime_template = Template::empty();
    runtime_template.kind = TemplateType::String;
    runtime_template.location = runtime_location.clone();
    runtime_template.content.add(Expression::string_slice(
        string_table.intern("runtime"),
        runtime_location.clone(),
        ValueMode::ImmutableOwned,
    ));

    let mut ast_nodes = vec![start_function_node(
        &entry_dir,
        vec![
            push_start_runtime_fragment_node(doc_template, doc_location, entry_scope.clone()),
            push_start_runtime_fragment_node(
                runtime_template,
                runtime_location,
                entry_scope.clone(),
            ),
        ],
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

    let mut parent = Template::empty();
    parent.kind = TemplateType::Comment(CommentDirectiveKind::Doc);
    parent.location = test_location(2);
    parent.content.add(Expression::string_slice(
        string_table.intern("parent"),
        test_location(2),
        ValueMode::ImmutableOwned,
    ));

    let mut child = Template::empty();
    child.kind = TemplateType::Comment(CommentDirectiveKind::Doc);
    child.location = test_location(3);
    child.content.add(Expression::string_slice(
        string_table.intern("child"),
        test_location(3),
        ValueMode::ImmutableOwned,
    ));

    let mut grandchild = Template::empty();
    grandchild.kind = TemplateType::Comment(CommentDirectiveKind::Doc);
    grandchild.location = test_location(4);
    grandchild.content.add(Expression::string_slice(
        string_table.intern("grandchild"),
        test_location(4),
        ValueMode::ImmutableOwned,
    ));

    child.doc_children.push(grandchild);
    parent.doc_children.push(child);

    let mut ast_nodes = vec![start_function_node(
        &entry_dir,
        vec![push_start_runtime_fragment_node(
            parent,
            test_location(2),
            entry_scope,
        )],
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
