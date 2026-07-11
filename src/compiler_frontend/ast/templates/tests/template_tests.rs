use super::*;
use crate::compiler_frontend::ast::ast_nodes::{AstNode, NodeKind};
use crate::compiler_frontend::ast::expressions::expression::{Expression, ExpressionKind};
use crate::compiler_frontend::ast::statements::functions::{
    FunctionReturn, FunctionSignature, ReturnSlot,
};
use crate::compiler_frontend::ast::templates::error::TemplateError;
use crate::compiler_frontend::ast::templates::styles::markdown::markdown_formatter;
use crate::compiler_frontend::ast::templates::template::{
    CommentDirectiveKind, TemplateSegmentOrigin, TemplateType,
};
use crate::compiler_frontend::ast::templates::template_types::Template;
use crate::compiler_frontend::ast::templates::tir::{
    TemplateIr, TemplateIrRegistry, TemplateIrStore, TemplateOverlaySet, TemplateRef,
    TemplateTirPhase, TemplateTirReference, TirView, finalized_template_tir_id,
    format_tir_template,
};
use crate::compiler_frontend::ast::templates::top_level_templates::FoldedConstTemplateResult;
use crate::compiler_frontend::compiler_messages::{
    DiagnosticPayload, InvalidTemplateStructureReason,
};
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::headers::parse_file_headers::TopLevelConstFragment;
use crate::compiler_frontend::paths::path_format::PathStringFormatConfig;
use crate::compiler_frontend::paths::path_resolution::ProjectPathResolver;
use crate::compiler_frontend::symbols::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tests::parse_support::{
    parse_single_file_ast, parse_single_file_ast_diagnostic,
};
use crate::compiler_frontend::tokenizer::tokens::{CharPosition, SourceLocation};
use crate::compiler_frontend::value_mode::ValueMode;
use crate::projects::settings::{DEFAULT_TEMPLATE_CONST_LOOP_ITERATIONS, IMPLICIT_START_FUNC_NAME};
use rustc_hash::FxHashMap;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

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

fn test_project_path_resolver() -> ProjectPathResolver {
    let cwd = std::env::temp_dir();
    ProjectPathResolver::new(
        cwd.clone(),
        cwd,
        &crate::libraries::SourceLibraryRegistry::default(),
        &crate::libraries::SourceFileKindRegistry::default(),
    )
    .expect("test path resolver should be valid")
}

fn collect_and_strip_comment_templates_for_tests_with_registry(
    ast_nodes: &mut [AstNode],
    string_table: &mut StringTable,
    template_ir_registry: Rc<RefCell<TemplateIrRegistry>>,
) -> Result<Vec<AstDocFragment>, TemplateError> {
    let resolver = test_project_path_resolver();
    collect_and_strip_comment_templates(
        ast_nodes,
        &resolver,
        &PathStringFormatConfig::default(),
        string_table,
        DEFAULT_TEMPLATE_CONST_LOOP_ITERATIONS,
        Some(template_ir_registry),
    )
}

#[test]
fn standalone_insert_helper_value_is_rejected_after_composition() {
    let source = r#"
value = [$insert("style"): color: red;]
"#;

    let diagnostic = parse_single_file_ast_diagnostic(source);

    assert!(matches!(
        diagnostic.payload,
        DiagnosticPayload::InvalidTemplateStructure {
            reason: InvalidTemplateStructureReason::HelperOutsideWrapperSlot,
        }
    ));
}

#[test]
fn finalized_module_constants_materialize_const_templates_before_hir() {
    let source = r#"
wrapper #= [:<div class="frame">[$slot]</div>]
content #= [wrapper: [:Hello]]
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
    let (ast, string_table) = parse_single_file_ast("[$doc:doc]\n[:runtime]");

    assert_eq!(ast.doc_fragments.len(), 1);
    assert!(matches!(ast.doc_fragments[0].kind, AstDocFragmentKind::Doc));
    assert_eq!(
        string_table.resolve(ast.doc_fragments[0].value),
        "<p>doc</p>"
    );

    let entry_start = ast
        .nodes
        .iter()
        .find(|node| matches!(node.kind, NodeKind::Function(_, _, _)))
        .expect("entry start should exist");
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
fn collects_top_level_doc_fragments_in_source_order() {
    let (ast, string_table) = parse_single_file_ast("[$doc:first]\n[$doc:second]\n[$doc:third]");
    let doc_fragments = ast.doc_fragments;

    assert_eq!(doc_fragments.len(), 3);
    assert_eq!(string_table.resolve(doc_fragments[0].value), "<p>first</p>");
    assert_eq!(
        string_table.resolve(doc_fragments[1].value),
        "<p>second</p>"
    );
    assert_eq!(string_table.resolve(doc_fragments[2].value), "<p>third</p>");
}

/// Builds a `$doc` template whose compatibility `TemplateContent` mirror has been
/// cleared and whose authoritative output lives in a same-store `Formatted` TIR root.
///
/// WHAT: creates a markdown-styled doc template, materializes it into `store`, runs
///       the TIR formatter adapter, and installs the formatted root as the template's
///       TIR reference.
/// WHY: lets doc-fragment collection tests prove that folding reads the formatted TIR
///      root when the compatibility content mirror is stale/cleared.
fn formatted_doc_template_with_store(
    text: &str,
    string_table: &mut StringTable,
) -> (Template, Rc<RefCell<TemplateIrRegistry>>) {
    let location = test_location(2);
    let mut template = Template::empty();
    template.kind = TemplateType::Comment(CommentDirectiveKind::Doc);
    template.location = location.clone();
    template.style.formatter = Some(markdown_formatter());
    template.content.add_with_origin(
        Expression::string_slice(
            string_table.intern(text),
            location.clone(),
            ValueMode::ImmutableOwned,
        ),
        TemplateSegmentOrigin::Body,
    );

    let mut store = TemplateIrStore::new();
    let parsed_template_id = finalized_template_tir_id(&template, &mut store, string_table)
        .expect("doc template should convert to TIR");

    let store_handle = Rc::new(RefCell::new(store));
    let mut registry = TemplateIrRegistry::new();
    let store_id = registry.adopt_store(Rc::clone(&store_handle));
    let overlay_set_id = registry.allocate_overlay_set(TemplateOverlaySet::empty());

    let root_ref = TemplateRef::new(store_id, parsed_template_id);
    let view = TirView::new(
        &registry,
        root_ref,
        TemplateTirPhase::Parsed,
        overlay_set_id,
    )
    .expect("TIR view should be valid");

    let formatter_result = format_tir_template(&view, &template.style, string_table)
        .expect("TIR formatter should succeed");

    let original_template = store_handle
        .borrow()
        .get_template(parsed_template_id)
        .cloned()
        .expect("parsed template should exist");
    let mut summary = original_template.summary;
    summary.has_formatter = false;

    let formatted_template_id = store_handle.borrow_mut().push_template(TemplateIr::new(
        formatter_result.root,
        original_template.style,
        original_template.kind,
        summary,
        original_template.location,
    ));

    template.tir_reference = Some(TemplateTirReference {
        root: TemplateRef::new(store_id, formatted_template_id),
        store_owner: Arc::clone(&store_handle.borrow().owner()),
        is_composed: true,
        phase: TemplateTirPhase::Formatted,
        overlay_set_id,
    });

    // Clear the compatibility content mirror to prove the TIR root is authoritative.
    template.content.atoms.clear();

    let registry = Rc::new(RefCell::new(registry));
    (template, registry)
}

#[test]
fn doc_fragment_folding_reads_authoritative_formatted_tir_root_when_content_mirror_is_cleared() {
    let mut string_table = StringTable::new();
    let entry_dir = InternedPath::from_single_str("main.bst", &mut string_table);
    let entry_scope = entry_dir.to_owned();

    let (doc_template, registry) = formatted_doc_template_with_store("doc body", &mut string_table);

    let mut ast_nodes = vec![start_function_node(
        &entry_dir,
        vec![push_start_runtime_fragment_node(
            doc_template,
            test_location(2),
            entry_scope,
        )],
        test_location(1),
        &mut string_table,
    )];

    let doc_fragments = collect_and_strip_comment_templates_for_tests_with_registry(
        &mut ast_nodes,
        &mut string_table,
        registry,
    )
    .expect("doc fragment collection should succeed");

    assert_eq!(doc_fragments.len(), 1);
    assert_eq!(
        string_table.resolve(doc_fragments[0].value),
        "<p>doc body</p>",
        "doc fragment folding must read the same-store Formatted TIR root, not the cleared content mirror"
    );
}

#[test]
fn doc_fragment_registry_path_respects_tir_authority_over_stale_content() {
    let mut string_table = StringTable::new();
    let entry_dir = InternedPath::from_single_str("main.bst", &mut string_table);
    let entry_scope = entry_dir.to_owned();

    // Build a doc template with a formatted TIR root, then add stale content
    // atoms that would have overridden the TIR under the old content-mirror
    // authority. With content-mirror authority deleted (Slice 4), the TIR
    // root is authoritative and the stale content must not bypass it.
    let (mut doc_template, registry) =
        formatted_doc_template_with_store("tir body", &mut string_table);
    doc_template.content.add_with_origin(
        Expression::string_slice(
            string_table.intern("mirror body"),
            test_location(2),
            ValueMode::ImmutableOwned,
        ),
        TemplateSegmentOrigin::Body,
    );

    let mut ast_nodes = vec![start_function_node(
        &entry_dir,
        vec![push_start_runtime_fragment_node(
            doc_template,
            test_location(2),
            entry_scope,
        )],
        test_location(1),
        &mut string_table,
    )];

    let doc_fragments = collect_and_strip_comment_templates_for_tests_with_registry(
        &mut ast_nodes,
        &mut string_table,
        registry,
    )
    .expect("doc fragment collection should succeed");

    assert_eq!(doc_fragments.len(), 1);
    assert_eq!(
        string_table.resolve(doc_fragments[0].value),
        "<p>tir body</p>",
        "registry-backed doc folding must respect the authoritative TIR root, not stale content atoms"
    );
}

#[test]
fn top_level_doc_comment_produces_formatted_doc_fragment() {
    let source = r#"
[$doc:
doc body
]
"#;

    let (ast, string_table) = parse_single_file_ast(source);

    assert_eq!(
        ast.doc_fragments.len(),
        1,
        "top-level $doc comment should produce exactly one doc fragment"
    );
    assert_eq!(
        string_table.resolve(ast.doc_fragments[0].value),
        "<p>doc body</p>",
        "doc fragment should be formatted Markdown from the authoritative TIR root"
    );
}

#[test]
fn collects_const_top_level_fragments_from_tir_result_record() {
    let mut string_table = StringTable::new();
    let path = InternedPath::from_single_str("main.bst", &mut string_table);
    let value = string_table.intern("const html");

    let mut results = FxHashMap::default();
    results.insert(path.clone(), FoldedConstTemplateResult::new(value));

    let fragments = vec![TopLevelConstFragment {
        runtime_insertion_index: 0,
        header_path: path,
        location: test_location(2),
    }];

    let collected =
        collect_const_top_level_fragments(&fragments, &results).expect("collection should succeed");

    assert_eq!(collected.len(), 1);
    assert_eq!(collected[0].runtime_insertion_index, 0);
    assert_eq!(string_table.resolve(collected[0].value), "const html");
}

#[test]
fn collects_const_top_level_fragments_from_folded_value() {
    let mut string_table = StringTable::new();
    let path = InternedPath::from_single_str("main.bst", &mut string_table);
    let value = string_table.intern("folded html");

    let mut results = FxHashMap::default();
    results.insert(path.clone(), FoldedConstTemplateResult::new(value));

    let fragments = vec![TopLevelConstFragment {
        runtime_insertion_index: 2,
        header_path: path,
        location: test_location(4),
    }];

    let collected =
        collect_const_top_level_fragments(&fragments, &results).expect("collection should succeed");

    assert_eq!(collected.len(), 1);
    assert_eq!(collected[0].runtime_insertion_index, 2);
    assert_eq!(string_table.resolve(collected[0].value), "folded html");
}

#[test]
fn collects_mixed_const_top_level_fragments_in_source_order() {
    let mut string_table = StringTable::new();
    let first_path = InternedPath::from_single_str("first.bst", &mut string_table);
    let second_path = InternedPath::from_single_str("second.bst", &mut string_table);

    let first_value = string_table.intern("first");
    let second_value = string_table.intern("second");

    let mut results = FxHashMap::default();
    results.insert(
        first_path.clone(),
        FoldedConstTemplateResult::new(first_value),
    );
    results.insert(
        second_path.clone(),
        FoldedConstTemplateResult::new(second_value),
    );

    let fragments = vec![
        TopLevelConstFragment {
            runtime_insertion_index: 1,
            header_path: first_path,
            location: test_location(2),
        },
        TopLevelConstFragment {
            runtime_insertion_index: 3,
            header_path: second_path,
            location: test_location(5),
        },
    ];

    let collected =
        collect_const_top_level_fragments(&fragments, &results).expect("collection should succeed");

    assert_eq!(collected.len(), 2);
    assert_eq!(collected[0].runtime_insertion_index, 1);
    assert_eq!(string_table.resolve(collected[0].value), "first");
    assert_eq!(collected[1].runtime_insertion_index, 3);
    assert_eq!(string_table.resolve(collected[1].value), "second");
}

#[test]
fn missing_const_top_level_fragment_result_returns_compiler_error() {
    let mut string_table = StringTable::new();
    let path = InternedPath::from_single_str("main.bst", &mut string_table);

    let results = FxHashMap::<InternedPath, FoldedConstTemplateResult>::default();
    let fragments = vec![TopLevelConstFragment {
        runtime_insertion_index: 0,
        header_path: path,
        location: test_location(2),
    }];

    let error = collect_const_top_level_fragments(&fragments, &results)
        .expect_err("missing result should fail");

    assert!(
        format!("{:?}", error).contains("no corresponding folded template value"),
        "error should identify missing folded template value: {:?}",
        error
    );
}
