//! Dependency sorting regression tests.
//!
//! WHAT: validates topological ordering, cycle detection, deterministic order, and start-function
//!       exclusion from the import dependency graph.
//! WHY: dependency sorting is the single producer of sorted declaration placeholders; any drift
//!      here breaks cross-file visibility and AST constant dependency ordering.

use super::*;
use crate::compiler_frontend::compiler_messages::CompileTimeEvaluationErrorReason;
use crate::compiler_frontend::compiler_messages::DiagnosticPayload;
use crate::compiler_frontend::external_packages::ExternalPackageRegistry;
use crate::compiler_frontend::headers::module_symbols::{PublicExportEntry, PublicExportTarget};
use crate::compiler_frontend::headers::parse_file_headers::{
    HeaderKind, HeaderParseOptions, Headers, parse_headers, prepare_file_from_tokens,
};
use crate::compiler_frontend::style_directives::StyleDirectiveRegistry;
use crate::compiler_frontend::symbols::interned_path::InternedPath;
use crate::compiler_frontend::tokenizer::lexer::tokenize;
use crate::compiler_frontend::tokenizer::tokens::TokenizerEntryMode;
use crate::libraries::external_import_providers::resolution_table::ExternalImportResolutionTable;
use std::path::PathBuf;

fn parse_module_headers(files: &[(&str, &str)], entry_path: &str) -> (Headers, StringTable) {
    let mut string_table = StringTable::new();
    let external_package_registry = ExternalPackageRegistry::new();
    let options = HeaderParseOptions::default();
    let style_directives = StyleDirectiveRegistry::built_ins();
    let entry_path_buf = PathBuf::from(entry_path);

    let mut prepared_outputs = Vec::with_capacity(files.len());
    let mut const_template_offset = 0usize;
    let mut runtime_fragment_offset = 0usize;

    for (path, source) in files {
        let path_buf = PathBuf::from(path);
        let interned_path = InternedPath::from_path_buf(&path_buf, &mut string_table);
        let file_tokens = tokenize(
            source,
            &interned_path,
            TokenizerEntryMode::SourceFile,
            &style_directives,
            &mut string_table,
            None,
        )
        .expect("tokenization should succeed");

        let output = prepare_file_from_tokens(
            file_tokens,
            &entry_path_buf,
            &options,
            &external_package_registry,
            &mut string_table,
            const_template_offset,
            runtime_fragment_offset,
        )
        .expect("preparation should succeed");

        const_template_offset += output.const_template_count;
        runtime_fragment_offset += output.runtime_fragment_count;
        prepared_outputs.push(output);
    }

    let headers = parse_headers(
        prepared_outputs,
        &external_package_registry,
        &ExternalImportResolutionTable::default(),
        options.project_path_resolver.as_ref(),
        &mut string_table,
    )
    .expect("header parsing should succeed");

    (headers, string_table)
}

fn header_name(
    header: &crate::compiler_frontend::headers::parse_file_headers::Header,
    string_table: &StringTable,
) -> String {
    header
        .tokens
        .src_path
        .name_str(string_table)
        .unwrap_or_default()
        .to_string()
}

#[test]
fn sorts_strict_top_level_dependencies_before_dependents_and_appends_start_last() {
    let (headers, mut string_table) = parse_module_headers(
        &[
            ("src/a.bst", "import @b { Middle }\nTop #Middle = Middle\n"),
            ("src/b.bst", "import @c { Thing }\nMiddle #Thing = Thing\n"),
            ("src/c.bst", "Thing #Int = 1\n"),
        ],
        "src/a.bst",
    );

    let sorted = resolve_module_dependencies(headers, &mut string_table)
        .expect("dependency sort should pass");

    let non_start_order = sorted
        .headers
        .iter()
        .filter(|header| !matches!(header.kind, HeaderKind::StartFunction))
        .map(|header| header_name(header, &string_table))
        .collect::<Vec<_>>();

    assert_eq!(non_start_order, vec!["Thing", "Middle", "Top"]);
    assert!(
        matches!(
            sorted.headers.last().map(|header| &header.kind),
            Some(HeaderKind::StartFunction)
        ),
        "entry start header must be appended after sorted top-level declarations"
    );

    let start_order = sorted
        .headers
        .iter()
        .filter(|header| matches!(header.kind, HeaderKind::StartFunction))
        .map(|header| header.source_file.to_portable_string(&string_table))
        .collect::<Vec<_>>();

    assert_eq!(start_order, vec!["src/a.bst"]);
}

#[test]
fn dependency_sort_preserves_root_activity_metadata() {
    let (headers, mut string_table) =
        parse_module_headers(&[("src/a.bst", "#[static]\n[runtime]\n")], "src/a.bst");

    let sorted = resolve_module_dependencies(headers, &mut string_table)
        .expect("dependency sorting should preserve root activity metadata");

    assert!(sorted.has_non_trivial_root_body);
    assert_eq!(sorted.const_fragment_count, 1);
    assert_eq!(sorted.entry_runtime_fragment_count, 1);
}

#[test]
fn reports_circular_dependencies() {
    let (headers, mut string_table) = parse_module_headers(
        &[
            ("src/a.bst", "import @b { Middle }\nTop #Middle = Middle\n"),
            ("src/b.bst", "import @a { Top }\nMiddle #Top = Top\n"),
        ],
        "src/a.bst",
    );

    let bag = resolve_module_dependencies(headers, &mut string_table)
        .expect_err("cycle should fail dependency sorting");

    let cycle_diagnostic = bag
        .diagnostics()
        .iter()
        .find(|diagnostic| {
            let DiagnosticPayload::CircularDependency { path } = &diagnostic.payload else {
                return false;
            };

            let path = path.to_portable_string(&string_table);
            path.contains("Top") || path.contains("Middle")
        })
        .unwrap_or_else(|| panic!("expected a cycle diagnostic, got: {bag:?}"));

    assert!(
        cycle_diagnostic
            .primary_location
            .scope
            .to_portable_string(&string_table)
            .contains("src/"),
        "cycle diagnostics should point at a declaration location instead of the default location"
    );
}

#[test]
fn constant_initializer_creates_dependency_sort_edge() {
    // WHY: header-stage constant_dependencies.rs now extracts initializer reference edges.
    // Constant initializers that reference other constants create top-level dependency edges
    // that dependency sorting respects.
    let (headers, mut string_table) = parse_module_headers(
        &[
            // Config's initializer references Value.
            // That reference creates a dependency edge from Config to Value.
            ("src/a.bst", "import @b { Value }\nConfig #= Value\n"),
            ("src/b.bst", "Value #Int = 42\n"),
        ],
        "src/a.bst",
    );

    let sorted = resolve_module_dependencies(headers, &mut string_table)
        .expect("sort must succeed — constant initializer edges are resolved by headers");

    let non_start_names: Vec<_> = sorted
        .headers
        .iter()
        .filter(|h| !matches!(h.kind, HeaderKind::StartFunction))
        .map(|h| header_name(h, &string_table))
        .collect();

    // Both headers must be present and Value must precede Config.
    assert_eq!(
        non_start_names,
        vec!["Value", "Config"],
        "constant initializer dependency must order Value before Config"
    );
}

#[test]
fn function_body_references_do_not_influence_header_provided_sort_order() {
    // WHY: function body references are AST/body-phase concerns, not
    // header-provided top-level dependency edges. Sorting should preserve source order
    // for otherwise-independent declarations.
    let (headers, mut string_table) = parse_module_headers(
        &[(
            "src/a.bst",
            "first|| -> Int:\n    return second()\n;\n\nsecond|| -> Int:\n    return 1\n;\n",
        )],
        "src/a.bst",
    );

    let sorted = resolve_module_dependencies(headers, &mut string_table)
        .expect("dependency sort should ignore body-only references");

    let non_start_names: Vec<_> = sorted
        .headers
        .iter()
        .filter(|header| !matches!(header.kind, HeaderKind::StartFunction))
        .map(|header| header_name(header, &string_table))
        .collect();

    assert_eq!(
        non_start_names,
        vec!["first", "second"],
        "function body call graph must not perturb strict header sorting"
    );
}

#[test]
fn function_error_return_dependency_orders_error_type_before_function() {
    // WHY: `T!` is signature metadata, not a body reference. Header dependency sorting must
    // order imported error payload declarations before functions that expose them.
    let (headers, mut string_table) = parse_module_headers(
        &[
            (
                "src/app.bst",
                "import @errors { AppError }\nparse|| -> Int, AppError!:\n    return 1\n;\n",
            ),
            ("src/errors.bst", "AppError = |message String|\n"),
        ],
        "src/app.bst",
    );

    let sorted = resolve_module_dependencies(headers, &mut string_table)
        .expect("error return dependency should be sortable");

    let non_start_names: Vec<_> = sorted
        .headers
        .iter()
        .filter(|header| !matches!(header.kind, HeaderKind::StartFunction))
        .map(|header| header_name(header, &string_table))
        .collect();

    assert_eq!(
        non_start_names,
        vec!["AppError", "parse"],
        "error payload type must be sorted before the fallible function signature"
    );
}

#[test]
fn capacity_reference_in_collection_type_orders_constant_before_user() {
    // WHY: bare capacity constants in fixed collection types create value-namespace
    // dependency edges to the referenced constant, even when the declaration is not a constant.
    let (headers, mut string_table) = parse_module_headers(
        &[
            (
                "src/a.bst",
                "import @b { capacity }\nmake |items ~{capacity Int}| -> Int:\n    return 1\n;\n",
            ),
            ("src/b.bst", "capacity #Int = 64\n"),
        ],
        "src/a.bst",
    );

    let sorted = resolve_module_dependencies(headers, &mut string_table)
        .expect("sort must succeed — capacity reference edges are resolved by headers");

    let non_start_names: Vec<_> = sorted
        .headers
        .iter()
        .filter(|h| !matches!(h.kind, HeaderKind::StartFunction))
        .map(|h| header_name(h, &string_table))
        .collect();

    assert_eq!(
        non_start_names,
        vec!["capacity", "make"],
        "capacity constant must be sorted before the declaration that uses it"
    );
}

#[test]
fn capacity_reference_same_file_forward_reference_is_rejected() {
    // WHY: a capacity constant declared after a typed declaration in the same file is a
    // same-file forward constant reference and must be rejected.
    let mut string_table = StringTable::new();
    let external_package_registry = ExternalPackageRegistry::new();
    let options = HeaderParseOptions::default();
    let style_directives = StyleDirectiveRegistry::built_ins();
    let entry_path = PathBuf::from("src/a.bst");
    let file_path = PathBuf::from("src/a.bst");
    let interned_path = InternedPath::from_path_buf(&file_path, &mut string_table);
    let file_tokens = tokenize(
        "make |items ~{capacity Int}| -> Int:\n    return 1\n;\ncapacity #Int = 64\n",
        &interned_path,
        TokenizerEntryMode::SourceFile,
        &style_directives,
        &mut string_table,
        None,
    )
    .expect("tokenization should succeed");

    let output = prepare_file_from_tokens(
        file_tokens,
        &entry_path,
        &options,
        &external_package_registry,
        &mut string_table,
        0,
        0,
    )
    .expect("preparation should succeed");

    let result = parse_headers(
        vec![output],
        &external_package_registry,
        &ExternalImportResolutionTable::default(),
        options.project_path_resolver.as_ref(),
        &mut string_table,
    );

    let bag = match result {
        Err(bag) => bag,
        Ok(_) => panic!("same-file forward capacity reference should fail during header parsing"),
    };

    let found = bag.diagnostics().iter().any(|diagnostic| {
        matches!(
            diagnostic.payload,
            DiagnosticPayload::CompileTimeEvaluationError {
                reason: CompileTimeEvaluationErrorReason::SameFileForwardConstantReference,
                ..
            }
        )
    });

    assert!(
        found,
        "expected a same-file forward constant reference diagnostic"
    );
}

#[test]
fn capacity_reference_in_function_signature_creates_dependency_edge() {
    let (headers, mut string_table) = parse_module_headers(
        &[
            (
                "src/a.bst",
                "import @b { size }\nmake |items ~{size Int}| -> Int:\n    return 1\n;\n",
            ),
            ("src/b.bst", "size #Int = 8\n"),
        ],
        "src/a.bst",
    );

    let sorted =
        resolve_module_dependencies(headers, &mut string_table).expect("sort must succeed");

    let non_start_names: Vec<_> = sorted
        .headers
        .iter()
        .filter(|h| !matches!(h.kind, HeaderKind::StartFunction))
        .map(|h| header_name(h, &string_table))
        .collect();

    assert_eq!(
        non_start_names,
        vec!["size", "make"],
        "capacity reference in function parameter must order constant before function"
    );
}

#[test]
fn capacity_reference_in_type_alias_creates_dependency_edge() {
    let (headers, mut string_table) = parse_module_headers(
        &[
            ("src/a.bst", "import @b { limit }\nItems as {limit Int}\n"),
            ("src/b.bst", "limit #Int = 16\n"),
        ],
        "src/a.bst",
    );

    let sorted =
        resolve_module_dependencies(headers, &mut string_table).expect("sort must succeed");

    let non_start_names: Vec<_> = sorted
        .headers
        .iter()
        .filter(|h| !matches!(h.kind, HeaderKind::StartFunction))
        .map(|h| header_name(h, &string_table))
        .collect();

    assert_eq!(
        non_start_names,
        vec!["limit", "Items"],
        "capacity reference in type alias must order constant before alias"
    );
}

#[test]
fn capacity_references_across_header_type_surfaces_create_dependency_edges() {
    let (headers, mut string_table) = parse_module_headers(
        &[
            (
                "src/a.bst",
                "import @b { limit }\n\
                 Buffer = |\n\
                     items {limit Int},\n\
                 |\n\
                 Status :: Pending |\n\
                     items {limit Int},\n\
                 |;\n\
                 make|| -> {limit Int}:\n\
                     return {}\n\
                 ;\n",
            ),
            ("src/b.bst", "limit #Int = 16\n"),
        ],
        "src/a.bst",
    );

    let sorted =
        resolve_module_dependencies(headers, &mut string_table).expect("sort must succeed");

    let non_start_names: Vec<_> = sorted
        .headers
        .iter()
        .filter(|h| !matches!(h.kind, HeaderKind::StartFunction))
        .map(|h| header_name(h, &string_table))
        .collect();

    assert_eq!(
        non_start_names,
        vec!["limit", "Buffer", "Status", "make"],
        "capacity references in fields, payloads, and returns must order the constant first"
    );
}

#[test]
fn trait_requirement_type_dependencies_order_required_type_before_trait() {
    let (headers, mut string_table) = parse_module_headers(
        &[
            (
                "src/traits.bst",
                "import @types { Message }\n\
                 DISPLAYABLE must:\n\
                     display |This| -> Message\n\
                 ;\n",
            ),
            ("src/types.bst", "Message = | text String |\n"),
        ],
        "src/traits.bst",
    );

    let sorted =
        resolve_module_dependencies(headers, &mut string_table).expect("sort must succeed");

    let non_start_names: Vec<_> = sorted
        .headers
        .iter()
        .filter(|header| !matches!(header.kind, HeaderKind::StartFunction))
        .map(|header| header_name(header, &string_table))
        .collect();

    assert_eq!(
        non_start_names,
        vec!["Message", "DISPLAYABLE"],
        "trait requirement signatures must order imported type surfaces before the trait"
    );
}

#[test]
fn trait_conformance_references_do_not_create_dependency_sort_edges() {
    let (headers, mut string_table) = parse_module_headers(
        &[
            (
                "src/app.bst",
                "import @traits { DISPLAYABLE }\n\
                 Label = | text String |\n\
                 Label must DISPLAYABLE\n",
            ),
            (
                "src/traits.bst",
                "DISPLAYABLE must:\n\
                     display |This| -> String\n\
                 ;\n",
            ),
        ],
        "src/app.bst",
    );

    let sorted =
        resolve_module_dependencies(headers, &mut string_table).expect("sort must succeed");

    let conformance_position = sorted
        .headers
        .iter()
        .position(|header| matches!(header.kind, HeaderKind::TraitConformance { .. }))
        .expect("expected a conformance header");
    let trait_position = sorted
        .headers
        .iter()
        .position(|header| header_name(header, &string_table) == "DISPLAYABLE")
        .expect("expected imported trait header");

    assert!(
        conformance_position < trait_position,
        "conformance references are resolved by AST after trait definitions are registered, so \
         they intentionally do not add dependency-sort edges"
    );
}

#[test]
fn trait_incompatibility_references_do_not_create_dependency_sort_edges() {
    let (headers, mut string_table) = parse_module_headers(
        &[
            (
                "src/app.bst",
                "import @traits { SERIALIZABLE }\n\
                 DISPLAYABLE must:\n\
                 ;\n\
                 DISPLAYABLE must not SERIALIZABLE\n",
            ),
            ("src/traits.bst", "SERIALIZABLE must:\n;\n"),
        ],
        "src/app.bst",
    );

    let sorted =
        resolve_module_dependencies(headers, &mut string_table).expect("sort must succeed");

    let incompatibility_position = sorted
        .headers
        .iter()
        .position(|header| matches!(header.kind, HeaderKind::TraitIncompatibility { .. }))
        .expect("expected an incompatibility header");
    let imported_trait_position = sorted
        .headers
        .iter()
        .position(|header| header_name(header, &string_table) == "SERIALIZABLE")
        .expect("expected imported trait header");

    assert!(
        incompatibility_position < imported_trait_position,
        "trait incompatibility references are resolved by AST after trait definitions are \
         registered, so they intentionally do not add dependency-sort edges"
    );
}

#[test]
fn source_library_public_export_dependency_edges_do_not_require_concrete_header_paths() {
    let (mut headers, mut string_table) = parse_module_headers(
        &[("src/page.bst", "NeedsWidget #String = \"ok\"\n")],
        "src/page.bst",
    );

    let helper_prefix = string_table.intern("helper");
    let widget_name = string_table.intern("Widget");
    let public_export_path = InternedPath::from_components(vec![helper_prefix, widget_name]);
    let concrete_target = InternedPath::from_path_buf(
        &PathBuf::from("lib/helper/internal/Widget"),
        &mut string_table,
    );

    let dependent_header = headers
        .headers
        .iter_mut()
        .find(|header| header_name(header, &string_table) == "NeedsWidget")
        .expect("expected dependent header");
    dependent_header.dependencies.insert(public_export_path);

    headers
        .module_symbols
        .source_library_public_exports
        .entry("helper".to_owned())
        .or_default()
        .insert(PublicExportEntry {
            export_name: widget_name,
            target: PublicExportTarget::Source(concrete_target),
        });

    let sorted = resolve_module_dependencies(headers, &mut string_table)
        .expect("public export dependency path should be accepted without a graph header");

    let non_start_names: Vec<_> = sorted
        .headers
        .iter()
        .filter(|header| !matches!(header.kind, HeaderKind::StartFunction))
        .map(|header| header_name(header, &string_table))
        .collect();

    assert_eq!(
        non_start_names,
        vec!["NeedsWidget"],
        "source-library public export paths may differ from concrete source headers"
    );
}
