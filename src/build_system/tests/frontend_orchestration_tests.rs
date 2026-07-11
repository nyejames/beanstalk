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
use crate::compiler_frontend::compiler_messages::{
    CompilerDiagnostic, DiagnosticPayload, TypeMismatchContext,
};
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
use crate::libraries::SourceFileKind;
use crate::libraries::external_import_providers::resolution_table::ExternalImportResolutionTable;
use crate::projects::settings::Config;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

fn beanstalk_input_file(source_path: PathBuf, source_code: &str) -> InputFile {
    InputFile {
        source_code: source_code.to_owned(),
        source_path,
        source_kind: SourceFileKind::Beanstalk,
    }
}

fn source_byte_count(input_files: &[InputFile]) -> usize {
    input_files
        .iter()
        .map(|input_file| input_file.source_code.len())
        .sum()
}

struct FrontendPreparationFixture {
    _temp_dir: tempfile::TempDir,
    frontend: CompilerFrontend,
    input_files: Vec<InputFile>,
    entry_file_path: PathBuf,
}

fn frontend_preparation_fixture(file_sources: &[(&str, &str)]) -> FrontendPreparationFixture {
    let temp_dir = tempfile::tempdir().expect("should create temp dir");
    let mut canonical_paths = Vec::new();
    let mut input_files = Vec::new();

    for (file_name, source) in file_sources {
        let path = temp_dir.path().join(file_name);
        fs::write(&path, source).expect("test source file should be written");
        let canonical = fs::canonicalize(&path).expect("test source file should canonicalize");
        input_files.push(beanstalk_input_file(canonical.clone(), source));
        canonical_paths.push(canonical);
    }

    let entry_file_path = canonical_paths
        .first()
        .expect("fixture should include at least one source file")
        .clone();

    let mut string_table = StringTable::new();
    let source_files = SourceFileTable::build(
        canonical_paths.iter().map(PathBuf::as_path),
        &entry_file_path,
        None,
        &mut string_table,
    )
    .expect("source file table should build");

    let mut frontend = CompilerFrontend::new(
        &Config::new(temp_dir.path().to_path_buf()),
        string_table,
        StyleDirectiveRegistry::built_ins(),
        Arc::new(ExternalPackageRegistry::new()),
        None,
    );
    frontend.set_source_files(source_files);

    FrontendPreparationFixture {
        _temp_dir: temp_dir,
        frontend,
        input_files,
        entry_file_path,
    }
}

fn header_source_file_names(
    headers: &crate::compiler_frontend::headers::parse_file_headers::Headers,
    string_table: &StringTable,
) -> Vec<String> {
    headers
        .headers
        .iter()
        .map(|header| {
            header
                .source_file
                .to_path_buf(string_table)
                .file_name()
                .expect("test logical source path should have a file name")
                .to_string_lossy()
                .into_owned()
        })
        .collect()
}

fn chunked_fixture_sources() -> Vec<(String, String)> {
    (0..super::FILE_PREPARATION_ALWAYS_PARALLEL_FILE_COUNT)
        .map(|index| {
            (
                format!("{index}.bst"),
                format!("value_{index} #= {index}\n"),
            )
        })
        .collect()
}

fn fixture_source_refs(file_sources: &[(String, String)]) -> Vec<(&str, &str)> {
    file_sources
        .iter()
        .map(|(file_name, source)| (file_name.as_str(), source.as_str()))
        .collect()
}

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
    assert!(
        rendered_lines[0].contains("expected Int, found String"),
        "errors should render before warnings; first line should be the type mismatch, got: {}",
        rendered_lines[0]
    );
    assert!(
        !rendered_lines[0].contains("TypeId("),
        "type names should be rendered, not raw type ids"
    );
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
        Arc::new(ExternalPackageRegistry::new()),
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
                external_package_registry: frontend.external_package_registry.as_ref(),
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
fn file_preparation_strategy_uses_serial_for_tiny_modules() {
    assert_eq!(
        super::FilePreparationStrategy::for_module(
            super::FILE_PREPARATION_ALWAYS_SERIAL_FILE_COUNT,
            super::FILE_PREPARATION_MEDIUM_PARALLEL_MIN_BYTES * 2
        ),
        super::FilePreparationStrategy::Serial
    );
}

#[test]
fn file_preparation_strategy_uses_serial_for_medium_modules_below_byte_threshold() {
    assert_eq!(
        super::FilePreparationStrategy::for_module(
            5,
            super::FILE_PREPARATION_MEDIUM_PARALLEL_MIN_BYTES - 1
        ),
        super::FilePreparationStrategy::Serial
    );
}

#[test]
fn file_preparation_strategy_uses_chunked_parallel_for_large_modules() {
    assert_eq!(
        super::FilePreparationStrategy::for_module(
            super::FILE_PREPARATION_ALWAYS_PARALLEL_FILE_COUNT,
            1
        ),
        super::FilePreparationStrategy::ParallelChunked
    );
}

#[test]
fn file_preparation_strategy_uses_per_file_parallel_for_large_byte_medium_modules() {
    assert_eq!(
        super::FilePreparationStrategy::for_module(
            5,
            super::FILE_PREPARATION_MEDIUM_PARALLEL_MIN_BYTES
        ),
        super::FilePreparationStrategy::ParallelPerFile
    );
}

#[test]
fn chunk_planning_splits_eight_files_into_stable_source_order_chunks() {
    let plans =
        super::plan_file_preparation_chunks(super::FILE_PREPARATION_ALWAYS_PARALLEL_FILE_COUNT, 4);

    assert_eq!(
        plans,
        vec![
            super::FilePreparationChunkPlan {
                chunk_index: 0,
                file_range: 0..4,
            },
            super::FilePreparationChunkPlan {
                chunk_index: 1,
                file_range: 4..8,
            },
        ]
    );
}

#[test]
fn chunk_planning_is_bounded_by_thread_policy_and_minimum_chunk_size() {
    let single_thread_plans = super::plan_file_preparation_chunks(40, 1);
    let four_thread_plans = super::plan_file_preparation_chunks(40, 4);
    let uneven_plans = super::plan_file_preparation_chunks(9, 4);

    assert_eq!(single_thread_plans.len(), 2);
    assert_eq!(single_thread_plans[0].file_range, 0..20);
    assert_eq!(single_thread_plans[1].file_range, 20..40);

    assert_eq!(four_thread_plans.len(), 8);
    assert_eq!(four_thread_plans.first().unwrap().file_range, 0..5);
    assert_eq!(four_thread_plans.last().unwrap().file_range, 35..40);

    assert_eq!(uneven_plans.len(), 2);
    assert!(
        uneven_plans
            .iter()
            .all(|plan| plan.file_range.len() >= super::FILE_PREPARATION_MIN_CHUNK_SIZE)
    );
}

#[test]
fn serial_file_preparation_produces_deterministic_ordered_output() {
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
        Arc::new(ExternalPackageRegistry::new()),
        None,
    );
    frontend.set_source_files(source_files);

    let input_files = vec![
        beanstalk_input_file(canonical_a.clone(), "alpha = 1\n#[hello]\n[runtime]\n"),
        beanstalk_input_file(canonical_b.clone(), "Beta #= 2\n"),
        beanstalk_input_file(canonical_c.clone(), "Gamma #= 3\n"),
    ];
    let source_byte_count = source_byte_count(&input_files);
    assert_eq!(
        super::FilePreparationStrategy::for_module(input_files.len(), source_byte_count),
        super::FilePreparationStrategy::Serial
    );

    let (headers, warnings) = super::FrontendModuleBuildContext::prepare_module_files(
        &mut frontend,
        &input_files,
        &canonical_a,
        &ExternalImportResolutionTable::default(),
        source_byte_count,
    )
    .expect("serial preparation should succeed");

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

#[test]
fn parallel_file_preparation_produces_deterministic_ordered_output() {
    let temp_dir = tempfile::tempdir().expect("should create temp dir");

    let mut canonical_paths = Vec::new();
    let mut input_files = Vec::new();
    for index in 0..super::FILE_PREPARATION_ALWAYS_PARALLEL_FILE_COUNT {
        let path = temp_dir.path().join(format!("{index}.bst"));
        let source = format!("value_{index} #= {index}\n");
        fs::write(&path, &source).unwrap();
        let canonical = fs::canonicalize(&path).unwrap();
        input_files.push(beanstalk_input_file(canonical.clone(), &source));
        canonical_paths.push(canonical);
    }

    let entry_file_path = canonical_paths
        .first()
        .expect("test should create an entry file")
        .clone();

    let mut string_table = StringTable::new();
    let source_files = SourceFileTable::build(
        canonical_paths.iter().map(PathBuf::as_path),
        &entry_file_path,
        None,
        &mut string_table,
    )
    .expect("source file table should build");

    let mut frontend = CompilerFrontend::new(
        &Config::new(temp_dir.path().to_path_buf()),
        string_table,
        StyleDirectiveRegistry::built_ins(),
        Arc::new(ExternalPackageRegistry::new()),
        None,
    );
    frontend.set_source_files(source_files);

    let source_byte_count = source_byte_count(&input_files);
    assert_eq!(
        super::FilePreparationStrategy::for_module(input_files.len(), source_byte_count),
        super::FilePreparationStrategy::ParallelChunked
    );

    let (headers, warnings) = super::FrontendModuleBuildContext::prepare_module_files(
        &mut frontend,
        &input_files,
        &entry_file_path,
        &ExternalImportResolutionTable::default(),
        source_byte_count,
    )
    .expect("parallel preparation should succeed");

    assert!(warnings.is_empty(), "test declarations should not warn");

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

    let mut previous_file_last_header = None;
    for index in 0..super::FILE_PREPARATION_ALWAYS_PARALLEL_FILE_COUNT {
        let expected_name = format!("{index}.bst");
        let first_position = header_source_names
            .iter()
            .position(|name| name == &expected_name)
            .unwrap_or_else(|| panic!("{expected_name} headers should be present"));
        let last_position = header_source_names
            .iter()
            .rposition(|name| name == &expected_name)
            .expect("first position proves this file exists");

        if let Some(previous_file_last_header) = previous_file_last_header {
            assert!(
                previous_file_last_header < first_position,
                "parallel preparation should preserve file order, got: {header_source_names:?}"
            );
        }
        previous_file_last_header = Some(last_position);
    }
    assert_eq!(
        previous_file_last_header,
        Some(header_source_names.len() - 1),
        "all headers should belong to ordered file groups"
    );
}

#[test]
fn chunked_file_preparation_merges_in_source_order_after_out_of_order_completion() {
    let file_sources = chunked_fixture_sources();
    let file_source_refs = fixture_source_refs(&file_sources);
    let mut fixture = frontend_preparation_fixture(&file_source_refs);

    let options = HeaderParseOptions {
        entry_file_id: fixture
            .frontend
            .source_files
            .get_by_canonical_path(&fixture.entry_file_path)
            .map(|identity| identity.file_id),
        project_path_resolver: fixture.frontend.project_path_resolver.clone(),
    };
    let fork_source = fixture.frontend.string_table.fork_source();
    let base_len = fork_source.base_len();

    let mut chunks = {
        let prepare_context = FrontendFilePrepareContext {
            source_files: &fixture.frontend.source_files,
            style_directives: &fixture.frontend.style_directives,
            external_package_registry: fixture.frontend.external_package_registry.as_ref(),
            entry_file_path: &fixture.entry_file_path,
            options: &options,
        };

        super::FrontendModuleBuildContext::prepare_module_file_chunks(
            &fixture.input_files,
            &fork_source,
            &prepare_context,
            0,
            0,
            super::FilePreparationStrategy::ParallelChunked,
        )
    };
    chunks.reverse();

    let (headers, warnings) = super::FrontendModuleBuildContext::merge_file_preparation_chunks(
        &mut fixture.frontend,
        chunks,
        base_len,
        &ExternalImportResolutionTable::default(),
        &options,
    )
    .expect("chunk merge should succeed");

    assert!(warnings.is_empty(), "test declarations should not warn");

    let header_source_names = header_source_file_names(&headers, &fixture.frontend.string_table);
    let mut previous_file_last_header = None;
    for index in 0..super::FILE_PREPARATION_ALWAYS_PARALLEL_FILE_COUNT {
        let expected_name = format!("{index}.bst");
        let first_position = header_source_names
            .iter()
            .position(|name| name == &expected_name)
            .unwrap_or_else(|| panic!("{expected_name} headers should be present"));
        let last_position = header_source_names
            .iter()
            .rposition(|name| name == &expected_name)
            .expect("first position proves this file exists");

        if let Some(previous_file_last_header) = previous_file_last_header {
            assert!(
                previous_file_last_header < first_position,
                "chunked preparation should preserve file order, got: {header_source_names:?}"
            );
        }
        previous_file_last_header = Some(last_position);
    }
}

#[test]
fn chunked_file_preparation_remaps_non_identity_later_chunks() {
    let file_sources = chunked_fixture_sources();
    let file_source_refs = fixture_source_refs(&file_sources);
    let mut fixture = frontend_preparation_fixture(&file_source_refs);
    let source_byte_count = source_byte_count(&fixture.input_files);

    let (headers, warnings) = super::FrontendModuleBuildContext::prepare_module_files(
        &mut fixture.frontend,
        &fixture.input_files,
        &fixture.entry_file_path,
        &ExternalImportResolutionTable::default(),
        source_byte_count,
    )
    .expect("chunked preparation should succeed");

    assert!(warnings.is_empty(), "test declarations should not warn");

    let header_names: Vec<_> = headers
        .headers
        .iter()
        .filter_map(|header| {
            header
                .tokens
                .src_path
                .name_str(&fixture.frontend.string_table)
                .map(str::to_owned)
        })
        .collect();

    for index in 0..super::FILE_PREPARATION_ALWAYS_PARALLEL_FILE_COUNT {
        let expected_name = format!("value_{index}");
        assert!(
            header_names.contains(&expected_name),
            "expected remapped declaration `{expected_name}` in {header_names:?}"
        );
    }
}

#[test]
fn chunked_file_preparation_preserves_warning_source_order() {
    let file_sources: Vec<_> = (0..super::FILE_PREPARATION_ALWAYS_PARALLEL_FILE_COUNT)
        .map(|index| (format!("{index}.bst"), format!("Value{index} #= {index}\n")))
        .collect();
    let file_source_refs = fixture_source_refs(&file_sources);
    let mut fixture = frontend_preparation_fixture(&file_source_refs);
    let source_byte_count = source_byte_count(&fixture.input_files);

    let (_headers, warnings) = super::FrontendModuleBuildContext::prepare_module_files(
        &mut fixture.frontend,
        &fixture.input_files,
        &fixture.entry_file_path,
        &ExternalImportResolutionTable::default(),
        source_byte_count,
    )
    .expect("chunked preparation should succeed with warnings");

    let warning_names: Vec<_> = warnings
        .iter()
        .filter_map(|warning| match &warning.payload {
            DiagnosticPayload::IdentifierNamingConvention { name, .. } => {
                Some(fixture.frontend.string_table.resolve(*name).to_owned())
            }
            _ => None,
        })
        .collect();
    let expected_names: Vec<_> = (0..super::FILE_PREPARATION_ALWAYS_PARALLEL_FILE_COUNT)
        .map(|index| format!("Value{index}"))
        .collect();

    assert_eq!(
        warning_names, expected_names,
        "warnings should aggregate in source-file order"
    );
}

#[cfg(all(feature = "timers", feature = "benchmark_counters"))]
#[test]
fn chunked_file_preparation_skips_identity_payload_remap() {
    use crate::compiler_frontend::compiler_messages::compiler_dev_logging::{
        start_benchmark_collection, stop_and_collect_benchmark_observations,
    };
    use crate::compiler_frontend::instrumentation::{
        capture_frontend_counters_for_test, log_frontend_counters, reset_frontend_counters,
    };

    let _guard = crate::compiler_frontend::instrumentation::lock_counter_test();
    let _counter_capture = capture_frontend_counters_for_test();

    reset_frontend_counters();
    start_benchmark_collection(true);

    let file_sources = chunked_fixture_sources();
    let file_source_refs = fixture_source_refs(&file_sources);
    let mut fixture = frontend_preparation_fixture(&file_source_refs);
    let source_byte_count = source_byte_count(&fixture.input_files);

    super::FrontendModuleBuildContext::prepare_module_files(
        &mut fixture.frontend,
        &fixture.input_files,
        &fixture.entry_file_path,
        &ExternalImportResolutionTable::default(),
        source_byte_count,
    )
    .expect("chunked preparation should succeed");

    log_frontend_counters();
    let observations = stop_and_collect_benchmark_observations();

    assert_counter_value(
        &observations.counters,
        "file_preparation_identity_remap_count",
        1.0,
    );
    assert_counter_value(
        &observations.counters,
        "file_preparation_non_identity_remap_count",
        1.0,
    );
    assert_counter_value(
        &observations.counters,
        "file_prepare_output_remap_calls",
        (super::FILE_PREPARATION_ALWAYS_PARALLEL_FILE_COUNT
            - super::FILE_PREPARATION_MIN_CHUNK_SIZE) as f64,
    );
}

#[cfg(all(feature = "timers", feature = "benchmark_counters"))]
fn assert_counter_value(
    counters: &[crate::compiler_frontend::compiler_messages::compiler_dev_logging::BenchmarkObservationMetric],
    name: &str,
    expected: f64,
) {
    let actual = counters
        .iter()
        .find(|counter| counter.name == name)
        .map(|counter| counter.value)
        .unwrap_or(-1.0);

    assert_eq!(actual, expected, "counter `{name}` did not match");
}
