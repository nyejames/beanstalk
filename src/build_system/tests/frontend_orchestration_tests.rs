//! Per-module frontend orchestration regression tests.
//!
//! WHAT: validates Stage 0/frontend boundary helpers such as message merging and parallel local-table
//!       file preparation.
//! WHY: these tests exercise infrastructure invariants that integration cases cannot inspect
//!      directly, while keeping test code out of the production orchestration module.

use super::merge_stage_messages;
use crate::build_system::build::InputFile;
use crate::compiler_frontend::CompilerFrontend;
use crate::compiler_frontend::compiler_errors::{CompilerMessages, SourceLocation};
use crate::compiler_frontend::compiler_messages::display_messages::format_terse_compiler_messages;
use crate::compiler_frontend::compiler_messages::{CompilerDiagnostic, TypeMismatchContext};
use crate::compiler_frontend::datatypes::environment::TypeEnvironment;
use crate::compiler_frontend::external_packages::ExternalPackageRegistry;
use crate::compiler_frontend::headers::parse_file_headers::{
    HeaderKind, HeaderParseOptions, parse_headers,
};
use crate::compiler_frontend::style_directives::StyleDirectiveRegistry;
use crate::compiler_frontend::symbols::identity::SourceFileTable;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::TokenKind;
use crate::compiler_frontend::{FrontendFilePrepareContext, FrontendFilePrepareInput};
use crate::libraries::external_import_providers::resolution_table::ExternalImportResolutionTable;
use crate::projects::settings::Config;
use std::fs;

#[test]
fn merge_stage_messages_preserves_render_type_context_with_warnings() {
    let string_table = StringTable::new();
    let type_environment = TypeEnvironment::new();
    let diagnostic = CompilerDiagnostic::type_mismatch(
        type_environment.builtins().int,
        type_environment.builtins().string,
        TypeMismatchContext::Assignment,
        SourceLocation::default(),
    );
    let warning = CompilerDiagnostic::unreachable_match_arm(SourceLocation::default());
    let messages = CompilerMessages::from_diagnostics(vec![diagnostic], string_table.clone())
        .with_type_context_for_all_diagnostics(type_environment);

    let merged = merge_stage_messages(messages, &[warning], &string_table);
    let rendered_lines = format_terse_compiler_messages(&merged);

    assert_eq!(merged.render_type_contexts().len(), 1);
    assert_eq!(rendered_lines.len(), 2);
    assert!(rendered_lines[1].contains("expected Int, found String"));
    assert!(!rendered_lines[1].contains("TypeId("));
}

#[test]
fn fused_preparation_merges_local_forks_and_resolves_source_and_generated_strings() {
    let temp_dir = tempfile::tempdir().expect("should create temp dir");
    let file_a = temp_dir.path().join("a.bst");
    let file_b = temp_dir.path().join("b.bst");
    // File A is the entry file with a runtime statement and a const template (which generates
    // a synthetic header name during header parsing).
    fs::write(&file_a, "alpha = 1\n#[hello]\n").unwrap();
    // File B is a normal source file with an exported constant declaration.
    fs::write(&file_b, "beta #= 2\n").unwrap();

    let canonical_a = fs::canonicalize(&file_a).unwrap();
    let canonical_b = fs::canonicalize(&file_b).unwrap();

    let mut string_table = StringTable::new();
    let source_files = SourceFileTable::build(
        &[&canonical_a, &canonical_b],
        &canonical_a,
        None,
        &mut string_table,
    )
    .expect("source file table should build");

    let module_table_size_before = string_table.len();

    let mut frontend = CompilerFrontend::new(
        &Config::new(temp_dir.path().to_path_buf()),
        string_table,
        StyleDirectiveRegistry::built_ins(),
        ExternalPackageRegistry::new(),
        None,
    );
    frontend.set_source_files(source_files);

    let options = HeaderParseOptions {
        entry_file_id: frontend
            .source_files
            .get_by_canonical_path(&canonical_a)
            .map(|i| i.file_id),
        project_path_resolver: frontend.project_path_resolver.clone(),
    };

    // Helper to prepare one file using the local-table variant and merge its delta back
    // into the module string table, returning the remapped output.
    let mut prepare_and_merge = |source_code: &str,
                                 source_path: &std::path::PathBuf,
                                 const_template_offset: usize,
                                 runtime_fragment_offset: usize| {
        let fork_source = frontend.string_table.fork_source();
        let (mut local_string_table, base_len) = fork_source.fork_for_module().into_parts();

        let result = {
            let prepare_context = FrontendFilePrepareContext {
                source_files: &frontend.source_files,
                style_directives: &frontend.style_directives,
                external_package_registry: &frontend.external_package_registry,
                entry_file_path: &canonical_a,
                options: &options,
            };
            let input = FrontendFilePrepareInput {
                source_code,
                source_path,
                source_kind: crate::libraries::SourceFileKind::Beanstalk,
                const_template_offset,
                runtime_fragment_offset,
            };

            CompilerFrontend::prepare_file_frontend_local(
                &prepare_context,
                input,
                &mut local_string_table,
            )
        };

        let remap = frontend
            .string_table
            .merge_delta_from(&local_string_table, base_len);
        match result {
            Ok(mut output) => {
                output.remap_string_ids(&remap);
                Ok(output)
            }
            Err(mut error) => {
                error.remap_string_ids(&remap);
                Err(error)
            }
        }
    };

    // Prepare file A (entry) — tokenization creates "alpha" and "hello"; header parsing
    // creates the synthetic "#const_template0" name for the const template.
    let output_a = prepare_and_merge("alpha = 1\n#[hello]\n", &canonical_a, 0, 0)
        .expect("file A preparation should succeed");

    // Prepare file B — tokenization creates "beta".
    let output_b = prepare_and_merge(
        "beta #= 2\n",
        &canonical_b,
        output_a.const_template_count,
        output_a.runtime_fragment_count,
    )
    .expect("file B preparation should succeed");

    // The module table should have grown: source strings (alpha, hello, beta) plus
    // header-generated strings (#const_template0 and possibly others).
    assert!(
        frontend.string_table.len() > module_table_size_before + 2,
        "module table should contain source strings plus header-generated strings"
    );

    // Aggregate the remapped outputs.
    let headers = parse_headers(
        vec![output_a, output_b],
        &frontend.external_package_registry,
        &ExternalImportResolutionTable::default(),
        options.project_path_resolver.as_ref(),
        &mut frontend.string_table,
    )
    .expect("header aggregation should succeed");

    // Verify source text string "beta" resolves through the module table in file B headers.
    let beta_header = headers
        .headers
        .iter()
        .find(|h| h.tokens.src_path.name_str(&frontend.string_table) == Some("beta"));
    assert!(
        beta_header.is_some(),
        "beta header should exist with name resolvable through module table"
    );

    // Verify header-generated string "#const_template0" resolves correctly after merge.
    let const_template_header = headers
        .headers
        .iter()
        .find(|h| matches!(h.kind, HeaderKind::ConstTemplate { .. }));
    assert!(
        const_template_header.is_some(),
        "const template header should exist"
    );
    let const_template_name = const_template_header
        .unwrap()
        .tokens
        .src_path
        .name_str(&frontend.string_table)
        .expect("const template should have a name");
    assert_eq!(
        const_template_name, "#const_template0",
        "generated const template name should resolve through module table"
    );

    // Verify token symbols inside the const template also resolve.
    let hello_token = const_template_header
        .unwrap()
        .tokens
        .tokens
        .iter()
        .find_map(|t| match &t.kind {
            TokenKind::Symbol(id) if frontend.string_table.resolve(*id) == "hello" => Some(*id),
            _ => None,
        });
    assert!(
        hello_token.is_some(),
        "hello symbol inside const template should resolve through module table"
    );

    // Verify that beta and the const template have different global IDs, proving
    // non-identity remapping occurred for at least one file's local suffix.
    let beta_id = beta_header
        .unwrap()
        .tokens
        .src_path
        .name()
        .expect("beta should have a name ID");
    let const_template_id = const_template_header
        .unwrap()
        .tokens
        .src_path
        .name()
        .expect("const template should have a name ID");
    assert_ne!(
        beta_id, const_template_id,
        "beta and #const_template0 should have different global IDs after non-identity remapping"
    );
}

#[test]
fn parallel_file_preparation_produces_deterministic_ordered_output() {
    let temp_dir = tempfile::tempdir().expect("should create temp dir");
    let file_a = temp_dir.path().join("a.bst");
    let file_b = temp_dir.path().join("b.bst");
    let file_c = temp_dir.path().join("c.bst");

    // File A is the entry file with a runtime template, a const template, and a declaration.
    fs::write(&file_a, "alpha = 1\n#[hello]\n[runtime]\n").unwrap();
    // File B is a normal source file with an exported constant (PascalCase produces a warning).
    fs::write(&file_b, "Beta #= 2\n").unwrap();
    // File C is a normal source file with another exported constant (PascalCase produces a warning).
    fs::write(&file_c, "Gamma #= 3\n").unwrap();

    let canonical_a = fs::canonicalize(&file_a).unwrap();
    let canonical_b = fs::canonicalize(&file_b).unwrap();
    let canonical_c = fs::canonicalize(&file_c).unwrap();

    let mut string_table = StringTable::new();
    let source_files = SourceFileTable::build(
        &[&canonical_a, &canonical_b, &canonical_c],
        &canonical_a,
        None,
        &mut string_table,
    )
    .expect("source file table should build");

    let module_table_size_before = string_table.len();

    let mut frontend = CompilerFrontend::new(
        &Config::new(temp_dir.path().to_path_buf()),
        string_table,
        StyleDirectiveRegistry::built_ins(),
        ExternalPackageRegistry::new(),
        None,
    );
    frontend.set_source_files(source_files);

    let input_files = vec![
        InputFile {
            source_code: "alpha = 1\n#[hello]\n[runtime]\n".to_owned(),
            source_path: canonical_a.clone(),
            source_kind: crate::libraries::SourceFileKind::Beanstalk,
        },
        InputFile {
            source_code: "Beta #= 2\n".to_owned(),
            source_path: canonical_b.clone(),
            source_kind: crate::libraries::SourceFileKind::Beanstalk,
        },
        InputFile {
            source_code: "Gamma #= 3\n".to_owned(),
            source_path: canonical_c.clone(),
            source_kind: crate::libraries::SourceFileKind::Beanstalk,
        },
    ];

    let (headers, warnings) = super::FrontendModuleBuildContext::prepare_module_files(
        &mut frontend,
        &input_files,
        &canonical_a,
        &ExternalImportResolutionTable::default(),
    )
    .expect("parallel preparation should succeed");

    // The module table should have grown with strings from all three files plus
    // header-generated strings.
    assert!(
        frontend.string_table.len() > module_table_size_before + 4,
        "module table should contain source strings from all files plus header-generated strings"
    );

    // Verify deterministic header ordering: input order is preserved before aggregation.
    let header_source_names: Vec<_> = headers
        .headers
        .iter()
        .map(|h| {
            h.source_file
                .to_path_buf(&frontend.string_table)
                .file_name()
                .expect("test logical source path should have a file name")
                .to_string_lossy()
                .into_owned()
        })
        .collect();

    let last_a = header_source_names
        .iter()
        .rposition(|name| name == "a.bst")
        .expect("file A headers should be present");
    let first_b = header_source_names
        .iter()
        .position(|name| name == "b.bst")
        .expect("file B headers should be present");
    let first_c = header_source_names
        .iter()
        .position(|name| name == "c.bst")
        .expect("file C headers should be present");
    assert!(
        last_a < first_b && first_b < first_c,
        "prepared headers should preserve input file order, got: {header_source_names:?}"
    );

    // Verify headers from all files exist and strings resolve.
    let beta_header = headers
        .headers
        .iter()
        .find(|h| h.tokens.src_path.name_str(&frontend.string_table) == Some("Beta"));
    assert!(beta_header.is_some(), "Beta header should exist");

    let gamma_header = headers
        .headers
        .iter()
        .find(|h| h.tokens.src_path.name_str(&frontend.string_table) == Some("Gamma"));
    assert!(gamma_header.is_some(), "Gamma header should exist");

    let const_template_header = headers
        .headers
        .iter()
        .find(|h| matches!(h.kind, HeaderKind::ConstTemplate { .. }));
    assert!(
        const_template_header.is_some(),
        "const template header should exist"
    );
    let const_template_name = const_template_header
        .unwrap()
        .tokens
        .src_path
        .name_str(&frontend.string_table)
        .expect("const template should have a name");
    assert_eq!(
        const_template_name, "#const_template0",
        "generated const template name should resolve through module table"
    );

    // Verify token symbols inside the const template resolve.
    let hello_token = const_template_header
        .unwrap()
        .tokens
        .tokens
        .iter()
        .find_map(|t| match &t.kind {
            TokenKind::Symbol(id) if frontend.string_table.resolve(*id) == "hello" => Some(*id),
            _ => None,
        });
    assert!(
        hello_token.is_some(),
        "hello symbol inside const template should resolve through module table"
    );

    // Verify runtime fragment count from entry file.
    assert_eq!(
        headers.entry_runtime_fragment_count, 1,
        "entry file should contribute exactly one runtime fragment"
    );

    // Verify const fragment from entry file.
    assert_eq!(
        headers.top_level_const_fragments.len(),
        1,
        "entry file should contribute exactly one const fragment"
    );

    // Verify warnings from multiple files are preserved deterministically.
    assert_eq!(
        warnings.len(),
        2,
        "expected two naming-convention warnings from Beta and Gamma"
    );
    assert!(
        warnings.iter().all(|w| matches!(
            w.kind,
            crate::compiler_frontend::compiler_messages::DiagnosticKind::Rule(
                crate::compiler_frontend::compiler_messages::RuleDiagnosticKind::IdentifierNamingConvention
            )
        )),
        "all warnings should be naming convention warnings"
    );

    // Verify non-identity remapping: Beta and the const template should have different
    // global IDs, proving at least one file's local suffix was remapped.
    let beta_id = beta_header
        .unwrap()
        .tokens
        .src_path
        .name()
        .expect("Beta should have a name ID");
    let const_template_id = const_template_header
        .unwrap()
        .tokens
        .src_path
        .name()
        .expect("const template should have a name ID");
    assert_ne!(
        beta_id, const_template_id,
        "Beta and #const_template0 should have different global IDs after non-identity remapping"
    );
}
