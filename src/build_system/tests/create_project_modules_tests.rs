use super::*;
use crate::build_system::build::BackendBuilder;
use crate::build_system::create_project_modules::resolve_project_entry_root;
use crate::build_system::project_config::{
    ProjectConfigParseServices, load_project_config, parse_project_config_file,
};
use crate::compiler_frontend::compiler_errors::CompilerMessages;
use crate::compiler_frontend::compiler_messages::render::{DiagnosticRenderContext, terse};
use crate::compiler_frontend::compiler_messages::{
    CompileTimeEvaluationErrorReason, CompilerDiagnostic, DiagnosticCategory, DiagnosticPayload,
    InvalidAssignmentTargetReason, InvalidConfigReason, InvalidImportClauseReason,
    InvalidLibraryFolderReason,
};
use crate::compiler_frontend::external_packages::ExternalPackageRegistry;
use crate::compiler_frontend::paths::path_resolution::ProjectPathResolver;
use crate::compiler_frontend::style_directives::StyleDirectiveRegistry;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_tests::test_support::temp_dir;
use crate::libraries::external_import_providers::provider::{
    ExternalFileExtension, ExternalImportProvider, ExternalImportProviderContext,
    ExternalImportProviderKind, ExternalImportRequest, ResolvedExternalImport,
};
use crate::libraries::external_import_providers::registry::ExternalImportProviderRegistry;
use std::collections::HashSet;
use std::ffi::OsStr;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

fn configured_resolver(config: &Config) -> ProjectPathResolver {
    configured_resolver_with_source_file_kinds(
        config,
        &crate::libraries::SourceFileKindRegistry::default(),
    )
}

fn configured_resolver_with_source_file_kinds(
    config: &Config,
    source_file_kinds: &crate::libraries::SourceFileKindRegistry,
) -> ProjectPathResolver {
    // WHAT: rebuilds the same canonical resolver the real project build uses.
    // WHY: module-discovery tests should exercise the exact path rules used in production.
    let project_root = fs::canonicalize(&config.entry_dir).expect("project root should resolve");
    let entry_root =
        fs::canonicalize(resolve_project_entry_root(config)).expect("entry root should resolve");
    let mut index_string_table = StringTable::new();
    let source_tree_index = super::source_tree_index::SourceTreeIndex::discover(
        entry_root.clone(),
        &project_root,
        config,
        &mut index_string_table,
    )
    .expect("source tree index should build");

    ProjectPathResolver::new_with_module_roots(
        project_root,
        entry_root,
        &crate::libraries::SourceLibraryRegistry::default(),
        source_file_kinds,
        source_tree_index.module_roots().clone(),
    )
    .expect("project path resolver should build")
}

fn test_style_directives() -> StyleDirectiveRegistry {
    StyleDirectiveRegistry::built_ins()
}

fn parse_project_config_for_test(
    config: &mut Config,
    config_path: &std::path::Path,
    style_directives: &StyleDirectiveRegistry,
) -> Result<(), CompilerMessages> {
    let libraries = crate::libraries::LibrarySet::with_mandatory_core();
    let mut string_table = StringTable::new();
    let services = ProjectConfigParseServices {
        style_directives,
        libraries: &libraries,
    };
    parse_project_config_file(config, config_path, &services, &mut string_table)
}

fn parse_project_config_for_test_with_html_keys(
    config: &mut Config,
    config_path: &std::path::Path,
    style_directives: &StyleDirectiveRegistry,
) -> Result<(), CompilerMessages> {
    let libraries =
        crate::projects::html_project::html_project_builder::HtmlProjectBuilder::new().libraries();
    let mut string_table = StringTable::new();
    let services = ProjectConfigParseServices {
        style_directives,
        libraries: &libraries,
    };
    parse_project_config_file(config, config_path, &services, &mut string_table)
}

fn parse_project_config_for_test_with_libraries(
    config: &mut Config,
    config_path: &std::path::Path,
    style_directives: &StyleDirectiveRegistry,
    libraries: &crate::libraries::LibrarySet,
) -> Result<(), CompilerMessages> {
    let mut string_table = StringTable::new();
    let services = ProjectConfigParseServices {
        style_directives,
        libraries,
    };
    parse_project_config_file(config, config_path, &services, &mut string_table)
}

fn discover_modules_for_test(
    config: &Config,
    resolver: &ProjectPathResolver,
    style_directives: &StyleDirectiveRegistry,
) -> Result<Vec<DiscoveredModule>, CompilerMessages> {
    let mut string_table = StringTable::new();
    let project_root = fs::canonicalize(&config.entry_dir).expect("project root should resolve");
    let entry_root =
        fs::canonicalize(resolve_project_entry_root(config)).expect("entry root should resolve");
    let source_tree_index = super::source_tree_index::SourceTreeIndex::discover(
        entry_root,
        &project_root,
        config,
        &mut string_table,
    )?;
    let mut external_packages = ExternalPackageRegistry::new();
    let external_import_providers =
        crate::libraries::external_import_providers::registry::ExternalImportProviderRegistry::empty();
    let mut external_import_cache =
        crate::libraries::external_import_providers::cache::ExternalImportProviderCache::new();
    let mut external_import_resolution_table =
        crate::libraries::external_import_providers::resolution_table::ExternalImportResolutionTable::new();
    let mut external_imports = super::reachable_file_discovery::ExternalImportDiscoveryState {
        external_packages: &mut external_packages,
        providers: &external_import_providers,
        cache: &mut external_import_cache,
        resolution_table: &mut external_import_resolution_table,
    };
    discover_all_modules_in_project(
        config,
        resolver,
        &source_tree_index,
        style_directives,
        &mut external_imports,
        &mut string_table,
    )
}

fn discover_modules_for_test_with_providers(
    config: &Config,
    resolver: &ProjectPathResolver,
    style_directives: &StyleDirectiveRegistry,
    external_import_providers: &ExternalImportProviderRegistry,
) -> Result<Vec<DiscoveredModule>, CompilerMessages> {
    let mut string_table = StringTable::new();
    let project_root = fs::canonicalize(&config.entry_dir).expect("project root should resolve");
    let entry_root =
        fs::canonicalize(resolve_project_entry_root(config)).expect("entry root should resolve");
    let source_tree_index = super::source_tree_index::SourceTreeIndex::discover(
        entry_root,
        &project_root,
        config,
        &mut string_table,
    )?;
    let mut external_packages = ExternalPackageRegistry::new();
    let mut external_import_cache =
        crate::libraries::external_import_providers::cache::ExternalImportProviderCache::new();
    let mut external_import_resolution_table =
        crate::libraries::external_import_providers::resolution_table::ExternalImportResolutionTable::new();
    let mut external_imports = super::reachable_file_discovery::ExternalImportDiscoveryState {
        external_packages: &mut external_packages,
        providers: external_import_providers,
        cache: &mut external_import_cache,
        resolution_table: &mut external_import_resolution_table,
    };

    discover_all_modules_in_project(
        config,
        resolver,
        &source_tree_index,
        style_directives,
        &mut external_imports,
        &mut string_table,
    )
}

fn rendered_first_error(messages: &CompilerMessages) -> String {
    let diagnostic = messages
        .error_diagnostics()
        .next()
        .expect("expected one diagnostic");
    terse::format_terse_diagnostic_with_context(
        diagnostic,
        DiagnosticRenderContext::new(&messages.string_table),
    )
}

fn assert_has_config_error(messages: &CompilerMessages) {
    assert!(
        messages
            .error_diagnostics()
            .any(|diagnostic| diagnostic.kind.category() == DiagnosticCategory::Config),
        "expected config-classified diagnostic"
    );
}

fn first_invalid_config_reason(messages: &CompilerMessages) -> &InvalidConfigReason {
    let diagnostic = messages
        .error_diagnostics()
        .next()
        .expect("expected one diagnostic");

    let DiagnosticPayload::InvalidConfig { reason, .. } = &diagnostic.payload else {
        panic!(
            "expected invalid config diagnostic, got {:?}",
            diagnostic.payload
        );
    };

    reason
}

fn discover_modules_for_test_messages(
    config: &Config,
    resolver: &ProjectPathResolver,
    style_directives: &StyleDirectiveRegistry,
) -> Result<Vec<DiscoveredModule>, CompilerMessages> {
    discover_modules_for_test(config, resolver, style_directives)
}

fn first_error_diagnostic(messages: &CompilerMessages) -> &CompilerDiagnostic {
    messages
        .error_diagnostics()
        .next()
        .expect("expected at least one typed error diagnostic")
}

fn first_rendered_error_message(messages: &CompilerMessages) -> String {
    let diagnostics = messages.error_diagnostics().cloned().collect::<Vec<_>>();
    crate::compiler_frontend::compiler_messages::render::terse::format_terse_diagnostics(
        &diagnostics,
        &messages.string_table,
    )
    .into_iter()
    .next()
    .expect("expected at least one rendered diagnostic")
}

#[test]
fn source_tree_index_collects_one_scan_and_applies_skip_policy() {
    let root = temp_dir("source_tree_index_outputs");
    let entry_root = root.clone();
    let nested = entry_root.join("nested");
    fs::create_dir_all(&nested).expect("should create nested module directory");

    for directory_name in [
        ".git",
        "target",
        "node_modules",
        "release",
        "dev",
        "dist",
        "build",
        ".cache",
        "generated",
        "scratch",
    ] {
        let directory = entry_root.join(directory_name);
        fs::create_dir_all(&directory).expect("should create skipped directory");
        fs::write(directory.join("#skipped.bst"), "").expect("should write skipped root");
    }

    fs::write(entry_root.join("#page.bst"), "").expect("should write entry root");
    fs::write(entry_root.join("ordinary.bst"), "").expect("should write ordinary source");
    fs::write(nested.join("#mod.bst"), "").expect("should write nested root");

    let mut config = Config::new(root.clone());
    config.dev_folder = PathBuf::from("scratch");
    config.release_folder = PathBuf::from("generated");
    let canonical_root = fs::canonicalize(&root).expect("project root should canonicalize");
    let canonical_entry_root =
        fs::canonicalize(&entry_root).expect("entry root should canonicalize");
    let mut string_table = StringTable::new();

    let index = super::source_tree_index::SourceTreeIndex::discover(
        canonical_entry_root.clone(),
        &canonical_root,
        &config,
        &mut string_table,
    )
    .expect("source tree index should build");

    assert_eq!(index.entry_root(), canonical_entry_root);
    assert_eq!(index.entry_candidates().len(), 2);
    assert!(index.entry_candidates()[0].ends_with("#page.bst"));
    assert_eq!(index.stats().dirs_visited, 2);
    assert_eq!(index.stats().dirs_skipped, 10);
    assert_eq!(index.stats().files_seen, 3);
    assert_eq!(index.stats().hash_root_files_seen, 2);
    assert_eq!(index.stats().module_roots_found, 2);

    let root_directories = index
        .module_roots()
        .root_directories()
        .map(|path| path.file_name().and_then(OsStr::to_str).unwrap_or_default())
        .collect::<Vec<_>>();
    assert_eq!(
        root_directories[0],
        canonical_entry_root
            .file_name()
            .and_then(OsStr::to_str)
            .unwrap()
    );
    assert_eq!(root_directories[1], "nested");

    fs::remove_dir_all(&root).expect("should remove temp root");
}

#[test]
fn source_tree_index_rejects_duplicate_hash_root_files() {
    let root = temp_dir("source_tree_index_duplicate_roots");
    let entry_root = root.join("src");
    fs::create_dir_all(&entry_root).expect("should create entry root");
    fs::write(entry_root.join("#page.bst"), "").expect("should write page root");
    fs::write(entry_root.join("#layout.bst"), "").expect("should write layout root");

    let config = Config::new(root.clone());
    let canonical_root = fs::canonicalize(&root).expect("project root should canonicalize");
    let canonical_entry_root =
        fs::canonicalize(&entry_root).expect("entry root should canonicalize");
    let mut string_table = StringTable::new();
    let messages = super::source_tree_index::SourceTreeIndex::discover(
        canonical_entry_root,
        &canonical_root,
        &config,
        &mut string_table,
    )
    .expect_err("a module directory may contain only one hash root");

    let reason = first_invalid_config_reason(&messages);
    let InvalidConfigReason::MultipleModuleRootFiles {
        directory,
        candidates,
    } = reason
    else {
        panic!("expected duplicate module root diagnostic, got {reason:?}");
    };
    assert_eq!(
        *directory,
        string_table.intern(&fs::canonicalize(&entry_root).unwrap().display().to_string())
    );
    assert_eq!(candidates.len(), 2);

    fs::remove_dir_all(&root).expect("should remove temp root");
}

#[test]
fn project_path_resolver_consumes_source_tree_module_roots() {
    let root = temp_dir("source_tree_index_resolver_consumption");
    let entry_root = root.join("src");
    let nested = entry_root.join("nested");
    fs::create_dir_all(&nested).expect("should create nested module directory");
    fs::write(entry_root.join("#page.bst"), "").expect("should write entry root");
    fs::write(nested.join("#mod.bst"), "").expect("should write nested facade");

    let mut config = Config::new(root.clone());
    config.entry_root = PathBuf::from("src");
    let mut string_table = StringTable::new();
    let setup = super::project_roots::build_project_path_resolver_with_index(
        &config,
        &crate::libraries::SourceLibraryRegistry::default(),
        &crate::libraries::SourceFileKindRegistry::default(),
        &mut string_table,
    )
    .expect("resolver setup should build from prepared roots");
    let resolver = setup.resolver;

    let mut import_path = crate::compiler_frontend::symbols::interned_path::InternedPath::new();
    import_path.push_str("nested", &mut string_table);
    import_path.push_str("identity", &mut string_table);
    let resolved = resolver
        .resolve_import_to_source_file_with_facade_fallback(
            &import_path,
            &entry_root.join("#page.bst"),
            &mut string_table,
        )
        .expect("prepared nested module root should resolve its facade");
    assert_eq!(
        resolved.path,
        fs::canonicalize(nested.join("#mod.bst")).unwrap()
    );

    fs::remove_dir_all(&root).expect("should remove temp root");
}

#[derive(Debug)]
struct CountingExternalImportProvider {
    calls: Arc<AtomicUsize>,
    extensions: Vec<ExternalFileExtension>,
}

impl CountingExternalImportProvider {
    fn new(calls: Arc<AtomicUsize>) -> Self {
        Self {
            calls,
            extensions: vec![ExternalFileExtension::from("js")],
        }
    }
}

impl ExternalImportProvider for CountingExternalImportProvider {
    fn kind(&self) -> ExternalImportProviderKind {
        ExternalImportProviderKind::new("counting-js")
    }

    fn supported_extensions(&self) -> &[ExternalFileExtension] {
        &self.extensions
    }

    fn resolve_external_import(
        &self,
        _request: ExternalImportRequest,
        _context: &mut ExternalImportProviderContext,
    ) -> Result<Option<ResolvedExternalImport>, CompilerMessages> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        Ok(None)
    }
}

#[test]
fn parses_config_constant_declarations() {
    let root = temp_dir("config_constants");
    fs::create_dir_all(&root).expect("should create root dir");
    let config_path = root.join(settings::CONFIG_FILE_NAME);

    fs::write(
        &config_path,
        "entry_root #= \"src\"\ndev_folder #= \"dev\"\noutput_folder #= \"release\"\nname #= \"docs\"\nversion #= \"1.2.3\"\nproject #= \"html\"\npage_url_style #= \"trailing_slash\"\nredirect_index_html #= true\nlibrary_folders #= { \"lib\", \"packages\" }\n",
    )
    .expect("should write config");

    let mut config = Config::new(root.clone());
    let style_directives = test_style_directives();
    parse_project_config_for_test_with_html_keys(&mut config, &config_path, &style_directives)
        .expect("config should parse");

    assert_eq!(config.entry_root, PathBuf::from("src"));
    assert_eq!(config.dev_folder, PathBuf::from("dev"));
    assert_eq!(config.release_folder, PathBuf::from("release"));
    assert_eq!(config.project_name, "docs");
    assert_eq!(config.version, "1.2.3");
    assert_eq!(config.settings.get("project"), Some(&"html".to_string()));
    assert_eq!(
        config.settings.get("page_url_style"),
        Some(&"trailing_slash".to_string())
    );
    assert_eq!(
        config.settings.get("redirect_index_html"),
        Some(&"true".to_string())
    );
    assert_eq!(
        config.library_folders,
        vec![PathBuf::from("lib"), PathBuf::from("packages")]
    );
    assert!(
        config.has_explicit_library_folders,
        "library_folders should be marked as explicitly configured"
    );

    fs::remove_dir_all(&root).expect("should remove temp root");
}

#[test]
fn loads_canonical_config_file_from_project_root() {
    let root = temp_dir("canonical_config_lookup");
    fs::create_dir_all(&root).expect("should create root dir");
    fs::write(
        root.join(settings::CONFIG_FILE_NAME),
        "entry_root #= \"src\"\n",
    )
    .expect("should write config");

    let mut config = Config::new(root.clone());
    let style_directives = test_style_directives();
    let libraries = crate::libraries::LibrarySet::with_mandatory_core();
    let services = ProjectConfigParseServices {
        style_directives: &style_directives,
        libraries: &libraries,
    };
    let mut string_table = StringTable::new();

    load_project_config(&mut config, &services, &mut string_table)
        .expect("canonical config should load");

    assert_eq!(config.config_file_path(), root.join("config.bst"));
    assert_eq!(config.entry_root, PathBuf::from("src"));

    fs::remove_dir_all(&root).expect("should remove root dir");
}

#[test]
fn rejects_direct_canonical_config_import_paths() {
    let mut string_table = StringTable::new();

    for import_path in ["config", "config.bst"] {
        let path = crate::compiler_frontend::symbols::interned_path::InternedPath::from_single_str(
            import_path,
            &mut string_table,
        );

        assert!(
            crate::compiler_frontend::source_libraries::root_file::import_path_references_config_file(
                &path,
                false,
                &string_table,
            ),
            "direct config import should be treated as a special file: {import_path}"
        );
    }

    let mut nested_source_path =
        crate::compiler_frontend::symbols::interned_path::InternedPath::new();
    nested_source_path.push_str("config", &mut string_table);
    nested_source_path.push_str("init_config", &mut string_table);

    assert!(
        !crate::compiler_frontend::source_libraries::root_file::import_path_references_config_file(
            &nested_source_path,
            false,
            &string_table,
        ),
        "a folder named config must remain a valid source path prefix"
    );

    let mut grouped_config_path =
        crate::compiler_frontend::symbols::interned_path::InternedPath::new();
    grouped_config_path.push_str("config", &mut string_table);
    grouped_config_path.push_str("project", &mut string_table);

    assert!(
        crate::compiler_frontend::source_libraries::root_file::import_path_references_config_file(
            &grouped_config_path,
            true,
            &string_table,
        ),
        "a grouped import must classify its source component as config"
    );
}

#[test]
fn rejects_unknown_config_key() {
    let root = temp_dir("config_unknown_key");
    fs::create_dir_all(&root).expect("should create root dir");
    let config_path = root.join(settings::CONFIG_FILE_NAME);

    fs::write(&config_path, "custom_key #= \"custom_value\"\n").expect("should write config");

    let mut config = Config::new(root.clone());
    let style_directives = test_style_directives();
    let messages = parse_project_config_for_test(&mut config, &config_path, &style_directives)
        .expect_err("config should fail");

    let diagnostic = first_error_diagnostic(&messages);
    assert!(
        matches!(
            &diagnostic.payload,
            DiagnosticPayload::InvalidConfig {
                reason: InvalidConfigReason::UnknownKey { .. },
                ..
            }
        ),
        "unexpected diagnostic payload: {:?}",
        diagnostic.payload
    );

    fs::remove_dir_all(&root).expect("should remove temp root");
}

#[test]
fn rejects_config_plain_immutable_bindings() {
    let root = temp_dir("config_plain_immutable");
    fs::create_dir_all(&root).expect("should create root dir");
    let config_path = root.join(settings::CONFIG_FILE_NAME);

    fs::write(
        &config_path,
        "entry_root = \"src\"\ndev_folder = \"dev\"\noutput_folder = \"release\"\n",
    )
    .expect("should write config");

    let mut config = Config::new(root.clone());
    let style_directives = test_style_directives();
    let messages = parse_project_config_for_test(&mut config, &config_path, &style_directives)
        .expect_err("config should reject plain immutable bindings");

    let diagnostic = first_error_diagnostic(&messages);
    assert!(
        matches!(
            &diagnostic.payload,
            DiagnosticPayload::InvalidConfig {
                reason: InvalidConfigReason::PlainBindingUnsupported,
                ..
            }
        ),
        "unexpected diagnostic payload: {:?}",
        diagnostic.payload
    );

    fs::remove_dir_all(&root).expect("should remove temp root");
}

#[test]
fn parses_config_explicit_hash_binding_mode() {
    let root = temp_dir("config_hash_binding");
    fs::create_dir_all(&root).expect("should create root dir");
    let config_path = root.join(settings::CONFIG_FILE_NAME);

    fs::write(
        &config_path,
        "entry_root #= \"src\"\nproject_name #String = \"docs\"\nversion #= \"1.0\"\n",
    )
    .expect("should write config");

    let mut config = Config::new(root.clone());
    let style_directives = test_style_directives();
    parse_project_config_for_test(&mut config, &config_path, &style_directives)
        .expect("config should parse");

    assert_eq!(config.entry_root, PathBuf::from("src"));
    assert_eq!(config.project_name, "docs");
    assert_eq!(config.version, "1.0");

    fs::remove_dir_all(&root).expect("should remove temp root");
}

#[test]
fn rejects_config_function_declarations() {
    let root = temp_dir("config_function_rejected");
    fs::create_dir_all(&root).expect("should create root dir");
    let config_path = root.join(settings::CONFIG_FILE_NAME);

    fs::write(&config_path, "helper ||:\n    entry_root = \"src\"\n;\n")
        .expect("should write config");

    let mut config = Config::new(root.clone());
    let style_directives = test_style_directives();
    let messages = parse_project_config_for_test(&mut config, &config_path, &style_directives)
        .expect_err("config should fail");

    let diagnostic = first_error_diagnostic(&messages);
    assert!(
        matches!(
            &diagnostic.payload,
            DiagnosticPayload::InvalidConfig {
                reason: InvalidConfigReason::FunctionUnsupported,
                ..
            }
        ),
        "unexpected diagnostic payload: {:?}",
        diagnostic.payload
    );

    fs::remove_dir_all(&root).expect("should remove temp root");
}

#[test]
fn accepts_config_type_declarations() {
    let cases = [
        (
            "struct",
            "Config = |\n    value String,\n|\nentry_root #= \"src\"\n",
        ),
        ("choice", "Mode ::\n    Ready,\n;\nentry_root #= \"src\"\n"),
        (
            "alias",
            "EntryRoot as String\nentry_root #EntryRoot = \"src\"\n",
        ),
    ];

    for (case_name, source) in cases {
        let root = temp_dir(&format!("config_{case_name}_accepted"));
        fs::create_dir_all(&root).expect("should create root dir");
        let config_path = root.join(settings::CONFIG_FILE_NAME);

        fs::write(&config_path, source).expect("should write config");

        let mut config = Config::new(root.clone());
        let style_directives = test_style_directives();
        parse_project_config_for_test(&mut config, &config_path, &style_directives)
            .expect("config should accept type declarations");

        assert_eq!(
            config.entry_root,
            PathBuf::from("src"),
            "config key should be parsed for {case_name}"
        );

        fs::remove_dir_all(&root).expect("should remove temp root");
    }
}

#[test]
fn rejects_config_mutable_bindings() {
    let root = temp_dir("config_mutable_rejected");
    fs::create_dir_all(&root).expect("should create root dir");
    let config_path = root.join(settings::CONFIG_FILE_NAME);

    fs::write(&config_path, "entry_root ~= \"src\"\n").expect("should write config");

    let mut config = Config::new(root.clone());
    let style_directives = test_style_directives();
    let messages = parse_project_config_for_test(&mut config, &config_path, &style_directives)
        .expect_err("config should fail");

    let diagnostic = first_error_diagnostic(&messages);
    assert!(
        matches!(
            &diagnostic.payload,
            DiagnosticPayload::InvalidConfig {
                reason: InvalidConfigReason::PlainBindingUnsupported,
                ..
            }
        ),
        "unexpected diagnostic payload: {:?}",
        diagnostic.payload
    );

    fs::remove_dir_all(&root).expect("should remove temp root");
}

#[test]
fn rejects_config_standalone_template() {
    let root = temp_dir("config_standalone_template_rejected");
    fs::create_dir_all(&root).expect("should create root dir");
    let config_path = root.join(settings::CONFIG_FILE_NAME);

    fs::write(&config_path, "[: hello]\n").expect("should write config");

    let mut config = Config::new(root.clone());
    let style_directives = test_style_directives();
    let messages = parse_project_config_for_test(&mut config, &config_path, &style_directives)
        .expect_err("config should fail");

    let diagnostic = first_error_diagnostic(&messages);
    assert!(
        matches!(
            &diagnostic.payload,
            DiagnosticPayload::InvalidConfig {
                reason: InvalidConfigReason::StandaloneTemplateUnsupported,
                ..
            }
        ),
        "unexpected diagnostic payload: {:?}",
        diagnostic.payload
    );

    fs::remove_dir_all(&root).expect("should remove temp root");
}

#[test]
fn rejects_config_const_page_fragment() {
    let root = temp_dir("config_const_fragment_rejected");
    fs::create_dir_all(&root).expect("should create root dir");
    let config_path = root.join(settings::CONFIG_FILE_NAME);

    fs::write(&config_path, "#[: hello]\n").expect("should write config");

    let mut config = Config::new(root.clone());
    let style_directives = test_style_directives();
    let messages = parse_project_config_for_test(&mut config, &config_path, &style_directives)
        .expect_err("config should fail");

    let diagnostic = first_error_diagnostic(&messages);
    assert!(
        matches!(
            &diagnostic.payload,
            DiagnosticPayload::InvalidConfig {
                reason: InvalidConfigReason::StandaloneTemplateUnsupported,
                ..
            }
        ),
        "unexpected diagnostic payload: {:?}",
        diagnostic.payload
    );

    fs::remove_dir_all(&root).expect("should remove temp root");
}

#[test]
fn rejects_project_local_config_import_even_when_module_facade_exists() {
    let root = temp_dir("config_project_local_import_rejected");
    fs::create_dir_all(&root).expect("should create root dir");
    fs::create_dir_all(root.join("settings")).expect("should create settings module");
    fs::write(root.join("settings/#mod.bst"), "value #= \"src\"\n")
        .expect("should write settings facade");
    let config_path = root.join(settings::CONFIG_FILE_NAME);

    fs::write(&config_path, "import @settings { value }\n").expect("should write config");

    let mut config = Config::new(root.clone());
    let style_directives = test_style_directives();
    let messages = parse_project_config_for_test(&mut config, &config_path, &style_directives)
        .expect_err("config should fail");

    let diagnostic = first_error_diagnostic(&messages);
    assert!(
        matches!(
            &diagnostic.payload,
            DiagnosticPayload::InvalidConfig {
                reason: InvalidConfigReason::ConfigImportRootViolation,
                ..
            }
        ),
        "unexpected diagnostic payload: {:?}",
        diagnostic.payload
    );

    fs::remove_dir_all(&root).expect("should remove temp root");
}

#[test]
fn rejects_relative_config_imports() {
    let root = temp_dir("config_relative_import_rejected");
    fs::create_dir_all(&root).expect("should create root dir");
    fs::write(root.join("defaults.bst"), "value #= \"src\"\n").expect("should write defaults");
    let config_path = root.join(settings::CONFIG_FILE_NAME);

    fs::write(&config_path, "import @./defaults { value }\n").expect("should write config");

    let mut config = Config::new(root.clone());
    let style_directives = test_style_directives();
    let messages = parse_project_config_for_test(&mut config, &config_path, &style_directives)
        .expect_err("config should fail");

    let diagnostic = first_error_diagnostic(&messages);
    assert!(
        matches!(
            &diagnostic.payload,
            DiagnosticPayload::InvalidConfig {
                reason: InvalidConfigReason::ConfigImportRootViolation,
                ..
            }
        ),
        "unexpected diagnostic payload: {:?}",
        diagnostic.payload
    );

    fs::remove_dir_all(&root).expect("should remove temp root");
}

#[test]
fn rejects_provider_backed_js_config_imports() {
    let root = temp_dir("config_js_import_rejected");
    fs::create_dir_all(&root).expect("should create root dir");
    fs::write(root.join("drawing.js"), "export const root = 'src';\n").expect("should write js");
    let config_path = root.join(settings::CONFIG_FILE_NAME);

    fs::write(&config_path, "import @./drawing.js { root }\n").expect("should write config");

    let mut config = Config::new(root.clone());
    let style_directives = test_style_directives();
    let messages =
        parse_project_config_for_test_with_html_keys(&mut config, &config_path, &style_directives)
            .expect_err("config should reject provider-backed JS imports");

    let diagnostic = first_error_diagnostic(&messages);
    assert!(
        matches!(
            &diagnostic.payload,
            DiagnosticPayload::InvalidConfig {
                reason: InvalidConfigReason::ConfigImportRootViolation,
                ..
            }
        ),
        "expected config import-root diagnostic for JS import, got: {:?}",
        diagnostic.payload
    );

    fs::remove_dir_all(&root).expect("should remove temp root");
}

#[test]
fn accepts_config_imported_builder_source_library_constant() {
    let root = temp_dir("config_builder_library_constant");
    let library_root = root.join("builder/defaults");
    fs::create_dir_all(&library_root).expect("should create builder library");
    fs::write(
        library_root.join("#mod.bst"),
        "export default_entry_root #= \"src\"\n",
    )
    .expect("should write builder facade");
    let config_path = root.join(settings::CONFIG_FILE_NAME);
    fs::write(
        &config_path,
        "import @defaults { default_entry_root }\nentry_root #= default_entry_root\n",
    )
    .expect("should write config");

    let mut libraries = crate::libraries::LibrarySet::with_mandatory_core();
    libraries
        .source_libraries
        .register_filesystem_root("defaults", library_root);

    let mut config = Config::new(root.clone());
    let style_directives = test_style_directives();
    parse_project_config_for_test_with_libraries(
        &mut config,
        &config_path,
        &style_directives,
        &libraries,
    )
    .expect("config should resolve builder source-library constant");

    assert_eq!(config.entry_root, PathBuf::from("src"));

    fs::remove_dir_all(&root).expect("should remove temp root");
}

#[test]
fn accepts_config_imported_constant_that_depends_on_imported_constant() {
    let root = temp_dir("config_builder_library_constant_chain");
    let library_root = root.join("builder/defaults");
    fs::create_dir_all(&library_root).expect("should create builder library");
    fs::write(
        library_root.join("#mod.bst"),
        "root_folder #= \"src\"\nexport default_entry_root #= root_folder\n",
    )
    .expect("should write builder facade");
    let config_path = root.join(settings::CONFIG_FILE_NAME);
    fs::write(
        &config_path,
        "import @defaults { default_entry_root }\nentry_root #= default_entry_root\n",
    )
    .expect("should write config");

    let mut libraries = crate::libraries::LibrarySet::with_mandatory_core();
    libraries
        .source_libraries
        .register_filesystem_root("defaults", library_root);

    let mut config = Config::new(root.clone());
    let style_directives = test_style_directives();
    parse_project_config_for_test_with_libraries(
        &mut config,
        &config_path,
        &style_directives,
        &libraries,
    )
    .expect("config should resolve imported constant dependency");

    assert_eq!(config.entry_root, PathBuf::from("src"));

    fs::remove_dir_all(&root).expect("should remove temp root");
}

#[test]
fn accepts_config_imported_constant_reexported_from_builder_source_library_file() {
    let root = temp_dir("config_builder_library_reexport");
    let library_root = root.join("builder/defaults");
    fs::create_dir_all(&library_root).expect("should create builder library");
    fs::write(
        library_root.join("#mod.bst"),
        "import @./values { root_folder as internal_root }\nexport default_entry_root #= internal_root\n",
    )
    .expect("should write builder facade");
    fs::write(library_root.join("values.bst"), "root_folder #= \"src\"\n")
        .expect("should write builder support file");
    let config_path = root.join(settings::CONFIG_FILE_NAME);
    fs::write(
        &config_path,
        "import @defaults { default_entry_root }\nentry_root #= default_entry_root\n",
    )
    .expect("should write config");

    let mut libraries = crate::libraries::LibrarySet::with_mandatory_core();
    libraries
        .source_libraries
        .register_filesystem_root("defaults", library_root);

    let mut config = Config::new(root.clone());
    let style_directives = test_style_directives();
    parse_project_config_for_test_with_libraries(
        &mut config,
        &config_path,
        &style_directives,
        &libraries,
    )
    .expect("config should resolve re-exported builder source-library constant");

    assert_eq!(config.entry_root, PathBuf::from("src"));

    fs::remove_dir_all(&root).expect("should remove temp root");
}

#[test]
fn accepts_config_imported_type_declarations_as_support_surface() {
    let root = temp_dir("config_builder_library_type_alias");
    let library_root = root.join("builder/defaults");
    fs::create_dir_all(&library_root).expect("should create builder library");
    fs::write(
        library_root.join("#mod.bst"),
        "export EntryRoot as String\nexport Config = |\n    value String,\n|\nexport Mode ::\n    Ready,\n;\nexport default_entry_root #= \"src\"\n",
    )
    .expect("should write builder facade");
    let config_path = root.join(settings::CONFIG_FILE_NAME);
    fs::write(
        &config_path,
        "import @defaults { EntryRoot, Config, Mode, default_entry_root }\nentry_root #EntryRoot = default_entry_root\n",
    )
    .expect("should write config");

    let mut libraries = crate::libraries::LibrarySet::with_mandatory_core();
    libraries
        .source_libraries
        .register_filesystem_root("defaults", library_root);

    let mut config = Config::new(root.clone());
    let style_directives = test_style_directives();
    parse_project_config_for_test_with_libraries(
        &mut config,
        &config_path,
        &style_directives,
        &libraries,
    )
    .expect("config should allow imported type declarations as support surface");

    assert_eq!(config.entry_root, PathBuf::from("src"));

    fs::remove_dir_all(&root).expect("should remove temp root");
}

#[test]
fn imported_config_support_duplicate_keeps_normal_duplicate_diagnostic() {
    let root = temp_dir("config_builder_library_duplicate");
    let library_root = root.join("builder/defaults");
    fs::create_dir_all(&library_root).expect("should create builder library");
    fs::write(
        library_root.join("#mod.bst"),
        "default_entry_root #= \"src\"\ndefault_entry_root #= \"app\"\n",
    )
    .expect("should write duplicate builder facade");
    let config_path = root.join(settings::CONFIG_FILE_NAME);
    fs::write(
        &config_path,
        "import @defaults { default_entry_root }\nentry_root #= default_entry_root\n",
    )
    .expect("should write config");

    let mut libraries = crate::libraries::LibrarySet::with_mandatory_core();
    libraries
        .source_libraries
        .register_filesystem_root("defaults", library_root);

    let mut config = Config::new(root.clone());
    let style_directives = test_style_directives();
    let messages = parse_project_config_for_test_with_libraries(
        &mut config,
        &config_path,
        &style_directives,
        &libraries,
    )
    .expect_err("duplicate imported support declarations should fail");

    let diagnostic = first_error_diagnostic(&messages);
    assert!(
        matches!(
            &diagnostic.payload,
            DiagnosticPayload::DuplicateDeclaration { .. }
        ),
        "expected normal duplicate declaration diagnostic, got: {:?}",
        diagnostic.payload
    );

    fs::remove_dir_all(&root).expect("should remove temp root");
}

#[test]
fn rejects_config_call_to_imported_builder_source_library_function() {
    let root = temp_dir("config_builder_library_function_call");
    let library_root = root.join("builder/defaults");
    fs::create_dir_all(&library_root).expect("should create builder library");
    fs::write(
        library_root.join("#mod.bst"),
        "export default_entry_root || -> String:\n    return \"src\"\n;\n",
    )
    .expect("should write builder facade");
    let config_path = root.join(settings::CONFIG_FILE_NAME);
    fs::write(
        &config_path,
        "import @defaults { default_entry_root }\nentry_root #= default_entry_root()\n",
    )
    .expect("should write config");

    let mut libraries = crate::libraries::LibrarySet::with_mandatory_core();
    libraries
        .source_libraries
        .register_filesystem_root("defaults", library_root);

    let mut config = Config::new(root.clone());
    let style_directives = test_style_directives();
    let messages = parse_project_config_for_test_with_libraries(
        &mut config,
        &config_path,
        &style_directives,
        &libraries,
    )
    .expect_err("config should reject imported function calls");

    let diagnostic = first_error_diagnostic(&messages);
    assert!(
        matches!(
            &diagnostic.payload,
            DiagnosticPayload::CompileTimeEvaluationError {
                reason: CompileTimeEvaluationErrorReason::NonConstantReferenceInConstant,
                ..
            }
        ),
        "expected non-constant-reference-in-constant diagnostic for imported function call, got: {:?}",
        diagnostic.payload
    );

    fs::remove_dir_all(&root).expect("should remove temp root");
}

#[test]
fn library_prefix_collision_with_entry_root_folder_rejected() {
    let root = temp_dir("entry_root_lib_collision");
    fs::create_dir_all(root.join("src/helper")).expect("should create src/helper");
    fs::create_dir_all(root.join("lib/helper")).expect("should create lib/helper");
    fs::write(root.join("src/#page.bst"), "x ~= 1\n").expect("should write entry");
    fs::write(root.join("lib/helper/#mod.bst"), "foo #= 1\n").expect("should write facade");
    fs::write(root.join("config.bst"), "entry_root #= \"src\"\n").expect("should write config");

    let mut config = Config::new(root.clone());
    let style_directives = test_style_directives();
    parse_project_config_for_test(
        &mut config,
        &root.join(settings::CONFIG_FILE_NAME),
        &style_directives,
    )
    .expect("config should parse");

    let mut string_table = StringTable::new();
    let result = super::project_roots::build_project_path_resolver(
        &config,
        &crate::libraries::SourceLibraryRegistry::default(),
        &crate::libraries::SourceFileKindRegistry::default(),
        &mut string_table,
    );

    assert!(
        result.is_err(),
        "entry-root folder colliding with source-library prefix should fail"
    );
    let messages = result.expect_err("checked above");
    let error_text = rendered_first_error(&messages);
    assert!(
        error_text.contains("collides") || error_text.contains("Ambiguous"),
        "error should mention collision or ambiguity: {error_text}"
    );
    assert_has_config_error(&messages);
    assert!(matches!(
        first_invalid_config_reason(&messages),
        InvalidConfigReason::EntryRootLibraryPrefixCollision { .. }
    ));

    fs::remove_dir_all(&root).expect("should remove temp root");
}

#[test]
fn rejects_hash_config_assignment_syntax() {
    let root = temp_dir("config_invalid_assignment");
    fs::create_dir_all(&root).expect("should create root dir");
    let config_path = root.join(settings::CONFIG_FILE_NAME);

    fs::write(&config_path, "#output_folder dist\n").expect("should write config");

    let mut config = Config::new(root.clone());
    let style_directives = test_style_directives();
    let messages = parse_project_config_for_test(&mut config, &config_path, &style_directives)
        .expect_err("config should fail");

    let error_message = first_rendered_error_message(&messages);
    assert!(
        error_message.contains("Use standard constant syntax: 'output_folder #= value'."),
        "unexpected error message: {error_message}"
    );

    fs::remove_dir_all(&root).expect("should remove temp root");
}

#[test]
fn rejects_deprecated_src_config_key() {
    let root = temp_dir("config_src_rename");
    fs::create_dir_all(&root).expect("should create root dir");
    let config_path = root.join(settings::CONFIG_FILE_NAME);

    fs::write(&config_path, "src #= \"src\"\n").expect("should write config");

    let mut config = Config::new(root.clone());
    let style_directives = test_style_directives();
    let messages = parse_project_config_for_test(&mut config, &config_path, &style_directives)
        .expect_err("config should fail");

    let diagnostic = first_error_diagnostic(&messages);
    assert!(
        matches!(
            &diagnostic.payload,
            DiagnosticPayload::InvalidConfig {
                reason: InvalidConfigReason::DeprecatedSrcKey,
                ..
            }
        ),
        "unexpected diagnostic payload: {:?}",
        diagnostic.payload
    );

    fs::remove_dir_all(&root).expect("should remove temp root");
}

#[test]
fn rejects_legacy_libraries_config_key() {
    let root = temp_dir("config_libraries_rename");
    fs::create_dir_all(&root).expect("should create root dir");
    let config_path = root.join(settings::CONFIG_FILE_NAME);

    fs::write(&config_path, "libraries #= { \"lib\" }\n").expect("should write config");

    let mut config = Config::new(root.clone());
    let style_directives = test_style_directives();
    let messages = parse_project_config_for_test(&mut config, &config_path, &style_directives)
        .expect_err("config should fail");

    let diagnostic = first_error_diagnostic(&messages);
    assert!(
        matches!(
            &diagnostic.payload,
            DiagnosticPayload::InvalidConfig {
                reason: InvalidConfigReason::ReplacedLibrariesKey,
                ..
            }
        ),
        "unexpected diagnostic payload: {:?}",
        diagnostic.payload
    );

    fs::remove_dir_all(&root).expect("should remove temp root");
}

#[test]
fn rejects_library_folder_absolute_path_entry() {
    let root = temp_dir("invalid_library_folders_absolute");
    fs::create_dir_all(&root).expect("should create root dir");
    let config_path = root.join(settings::CONFIG_FILE_NAME);

    fs::write(&config_path, "library_folders #= { \"/absolute/lib\" }\n")
        .expect("should write config");

    let mut config = Config::new(root.clone());
    let style_directives = test_style_directives();
    let messages = parse_project_config_for_test(&mut config, &config_path, &style_directives)
        .expect_err("config should fail");

    let diagnostic = first_error_diagnostic(&messages);
    assert!(
        matches!(
            &diagnostic.payload,
            DiagnosticPayload::InvalidConfig {
                reason: InvalidConfigReason::InvalidLibraryFolder {
                    reason: InvalidLibraryFolderReason::AbsolutePath,
                    ..
                },
                ..
            }
        ),
        "unexpected diagnostic payload: {:?}",
        diagnostic.payload
    );

    fs::remove_dir_all(&root).expect("should remove temp root");
}

#[test]
fn rejects_library_folder_parent_directory_entry() {
    let root = temp_dir("invalid_library_folders_dotdot");
    fs::create_dir_all(&root).expect("should create root dir");
    let config_path = root.join(settings::CONFIG_FILE_NAME);

    fs::write(&config_path, "library_folders #= { \"../lib\" }\n").expect("should write config");

    let mut config = Config::new(root.clone());
    let style_directives = test_style_directives();
    let messages = parse_project_config_for_test(&mut config, &config_path, &style_directives)
        .expect_err("config should fail");

    let diagnostic = first_error_diagnostic(&messages);
    assert!(
        matches!(
            &diagnostic.payload,
            DiagnosticPayload::InvalidConfig {
                reason: InvalidConfigReason::InvalidLibraryFolder {
                    reason: InvalidLibraryFolderReason::ParentDirectorySegment,
                    ..
                },
                ..
            }
        ),
        "unexpected diagnostic payload: {:?}",
        diagnostic.payload
    );

    fs::remove_dir_all(&root).expect("should remove temp root");
}

#[test]
fn rejects_duplicate_library_folder_entries() {
    let root = temp_dir("duplicate_library_folders");
    fs::create_dir_all(&root).expect("should create root dir");
    let config_path = root.join(settings::CONFIG_FILE_NAME);

    fs::write(&config_path, "library_folders #= { \"lib\", \"lib\" }\n")
        .expect("should write config");

    let mut config = Config::new(root.clone());
    let style_directives = test_style_directives();
    let messages = parse_project_config_for_test(&mut config, &config_path, &style_directives)
        .expect_err("config should fail");

    let diagnostic = first_error_diagnostic(&messages);
    assert!(
        matches!(
            &diagnostic.payload,
            DiagnosticPayload::InvalidConfig {
                reason: InvalidConfigReason::DuplicateLibraryFolder { .. },
                ..
            }
        ),
        "unexpected diagnostic payload: {:?}",
        diagnostic.payload
    );

    fs::remove_dir_all(&root).expect("should remove temp root");
}

#[test]
fn rejects_nested_library_folder_entry() {
    let root = temp_dir("invalid_library_folders_nested");
    fs::create_dir_all(&root).expect("should create root dir");
    let config_path = root.join(settings::CONFIG_FILE_NAME);

    fs::write(&config_path, "library_folders #= { \"lib/helpers\" }\n")
        .expect("should write config");

    let mut config = Config::new(root.clone());
    let style_directives = test_style_directives();
    let messages = parse_project_config_for_test(&mut config, &config_path, &style_directives)
        .expect_err("config should fail");

    let diagnostic = first_error_diagnostic(&messages);
    assert!(
        matches!(
            &diagnostic.payload,
            DiagnosticPayload::InvalidConfig {
                reason: InvalidConfigReason::InvalidLibraryFolder {
                    reason: InvalidLibraryFolderReason::NestedPath,
                    ..
                },
                ..
            }
        ),
        "unexpected diagnostic payload: {:?}",
        diagnostic.payload
    );

    fs::remove_dir_all(&root).expect("should remove temp root");
}

#[test]
fn missing_default_library_folder_is_ignored() {
    let root = temp_dir("missing_default_lib_ignored");
    fs::create_dir_all(root.join("src")).expect("should create src");
    fs::write(root.join("src/#page.bst"), "x ~= 1\n").expect("should write entry");
    fs::write(root.join("config.bst"), "entry_root #= \"src\"\n").expect("should write config");

    let mut config = Config::new(root.clone());
    let style_directives = test_style_directives();
    parse_project_config_for_test(
        &mut config,
        &root.join(settings::CONFIG_FILE_NAME),
        &style_directives,
    )
    .expect("config should parse");

    assert!(
        !config.has_explicit_library_folders,
        "default library folders should not be marked explicit"
    );

    let mut string_table = StringTable::new();
    let resolver = super::project_roots::build_project_path_resolver(
        &config,
        &crate::libraries::SourceLibraryRegistry::default(),
        &crate::libraries::SourceFileKindRegistry::default(),
        &mut string_table,
    )
    .expect("resolver should build even when default /lib is missing");

    assert!(
        resolver.source_library_roots().is_empty(),
        "no source libraries should be discovered when default /lib is missing"
    );

    fs::remove_dir_all(&root).expect("should remove temp root");
}

#[test]
fn accepts_config_const_record_field_projection() {
    let root = temp_dir("config_const_record_projection");
    fs::create_dir_all(&root).expect("should create root dir");
    let config_path = root.join(settings::CONFIG_FILE_NAME);

    fs::write(
        &config_path,
        "Defaults = |\n    entry_root String = \"src\",\n|\n\nentry_root #= Defaults().entry_root\n",
    )
    .expect("should write config");

    let mut config = Config::new(root.clone());
    let style_directives = test_style_directives();
    parse_project_config_for_test(&mut config, &config_path, &style_directives)
        .expect("config with const-record field projection should succeed");

    assert_eq!(
        config.entry_root,
        PathBuf::from("src"),
        "entry_root should resolve through const-record field projection"
    );

    fs::remove_dir_all(&root).expect("should remove temp root");
}

#[test]
fn malformed_import_syntax_keeps_precise_location_during_module_discovery() {
    let root = temp_dir("malformed_import_location");
    let src = root.join("src");
    fs::create_dir_all(&src).expect("should create src dir");
    fs::write(
        root.join(settings::CONFIG_FILE_NAME),
        "entry_root #= \"src\"\n",
    )
    .expect("should write config");
    fs::write(src.join("#page.bst"), "import\n#[:ok]\n").expect("should write malformed entry");

    let mut config = Config::new(root.clone());
    let style_directives = test_style_directives();
    parse_project_config_for_test(
        &mut config,
        &root.join(settings::CONFIG_FILE_NAME),
        &style_directives,
    )
    .expect("config should parse");
    let resolver = configured_resolver(&config);
    let messages = match discover_modules_for_test_messages(&config, &resolver, &style_directives) {
        Ok(_) => panic!("malformed import should fail discovery"),
        Err(messages) => messages,
    };

    let diagnostics = messages.error_diagnostics().collect::<Vec<_>>();
    assert_eq!(diagnostics.len(), 1);
    let diagnostic = diagnostics[0];
    assert_eq!(
        diagnostic
            .primary_location
            .scope
            .to_path_buf(&messages.string_table),
        src.join("#page.bst")
            .canonicalize()
            .expect("entry file path should canonicalize")
    );
    assert_eq!(diagnostic.primary_location.start_pos.line_number, 1);
    assert_eq!(diagnostic.primary_location.start_pos.char_column, 1);
    assert!(
        matches!(
            &diagnostic.payload,
            DiagnosticPayload::InvalidImportClause {
                reason: InvalidImportClauseReason::ExpectedPath,
                ..
            }
        ),
        "unexpected diagnostic payload: {:?}",
        diagnostic.payload
    );

    fs::remove_dir_all(&root).expect("should remove temp root");
}

#[test]
fn config_import_parse_failure_keeps_precise_location_in_compiler_messages() {
    let root = temp_dir("config_import_location");
    fs::create_dir_all(&root).expect("should create root dir");
    let config_path = root.join(settings::CONFIG_FILE_NAME);
    fs::write(&config_path, "import\n").expect("should write malformed config");

    let mut config = Config::new(root.clone());
    let style_directives = test_style_directives();
    let messages = parse_project_config_for_test(&mut config, &config_path, &style_directives)
        .expect_err("config should fail");

    let diagnostics = messages.error_diagnostics().collect::<Vec<_>>();
    assert_eq!(diagnostics.len(), 1);
    let diagnostic = diagnostics[0];
    assert_eq!(
        diagnostic
            .primary_location
            .scope
            .to_path_buf(&messages.string_table),
        config_path
    );
    assert_eq!(diagnostic.primary_location.start_pos.line_number, 1);
    assert_eq!(diagnostic.primary_location.start_pos.char_column, 0);
    assert!(
        matches!(
            &diagnostic.payload,
            DiagnosticPayload::InvalidImportClause {
                reason: InvalidImportClauseReason::ExpectedPath,
                ..
            }
        ),
        "unexpected diagnostic payload: {:?}",
        diagnostic.payload
    );

    fs::remove_dir_all(&root).expect("should remove temp root");
}

#[test]
fn discover_modules_uses_reachable_files_only() {
    let root = temp_dir("reachable_only");
    let src = root.join("src");
    fs::create_dir_all(src.join("libs")).expect("should create libs folder");
    fs::create_dir_all(src.join("styles")).expect("should create styles folder");
    fs::create_dir_all(src.join("docs")).expect("should create docs folder");

    fs::write(
        root.join(settings::CONFIG_FILE_NAME),
        "entry_root #= \"src\"\n",
    )
    .expect("should write config");
    fs::create_dir_all(src.join("errors")).expect("should create errors folder");
    fs::write(src.join("#page.bst"), "import @libs/html/basic\n#[:ok]\n")
        .expect("should write entry");
    fs::write(src.join("errors/#404.bst"), "#[:404]\n").expect("should write 404");
    fs::write(src.join("libs/html.bst"), "basic #= [:basic]\n").expect("should write lib");
    fs::write(src.join("styles/docs.bst"), "navbar #= [:nav]\n").expect("should write style");
    fs::write(src.join("docs/outdated.bst"), "this is invalid syntax")
        .expect("should write outdated file");

    let mut config = Config::new(root.clone());
    let style_directives = test_style_directives();
    parse_project_config_for_test(
        &mut config,
        &root.join(settings::CONFIG_FILE_NAME),
        &style_directives,
    )
    .expect("config parse");
    let resolver = configured_resolver(&config);

    let modules = discover_modules_for_test(&config, &resolver, &style_directives)
        .expect("module discovery should pass");

    assert_eq!(modules.len(), 2);

    let page_module = modules
        .iter()
        .find(|module| module.entry_point.file_name() == Some(OsStr::new("#page.bst")))
        .expect("should include #page module");
    let page_paths = page_module
        .input_files
        .iter()
        .map(|file| {
            file.source_path
                .file_name()
                .unwrap()
                .to_string_lossy()
                .to_string()
        })
        .collect::<HashSet<_>>();

    assert!(page_paths.contains("#page.bst"));
    assert!(page_paths.contains("html.bst"));
    assert!(!page_paths.contains("outdated.bst"));

    fs::remove_dir_all(&root).expect("should remove temp root");
}

#[test]
fn discover_modules_resolves_relative_child_imports() {
    let root = temp_dir("relative_imports");
    let src = root.join("src");
    fs::create_dir_all(src.join("components")).expect("should create components folder");

    fs::write(
        root.join(settings::CONFIG_FILE_NAME),
        "entry_root #= \"src\"\n",
    )
    .expect("should write config");
    fs::write(
        src.join("#page.bst"),
        "import @./components/widget\nio.line([: [\"page\"]])\n",
    )
    .expect("should write page");
    fs::write(
        src.join("components/widget.bst"),
        "import @./common\nio.line([: [\"widget\"]])\n",
    )
    .expect("should write widget file");
    fs::write(
        src.join("components/common.bst"),
        "io.line([: [\"common\"]])\n",
    )
    .expect("should write common");

    let mut config = Config::new(root.clone());
    let style_directives = test_style_directives();
    parse_project_config_for_test(
        &mut config,
        &root.join(settings::CONFIG_FILE_NAME),
        &style_directives,
    )
    .expect("config parse");
    let resolver = configured_resolver(&config);

    let modules = discover_modules_for_test(&config, &resolver, &style_directives)
        .expect("module discovery should pass");
    assert_eq!(modules.len(), 1, "expected exactly one entry module");

    let discovered = modules[0]
        .input_files
        .iter()
        .map(|file| {
            file.source_path
                .file_name()
                .and_then(OsStr::to_str)
                .unwrap_or_default()
                .to_string()
        })
        .collect::<HashSet<_>>();

    assert!(discovered.contains("#page.bst"));
    assert!(discovered.contains("widget.bst"));
    assert!(discovered.contains("common.bst"));

    fs::remove_dir_all(&root).expect("should remove temp root");
}

#[test]
fn entry_root_fallback_wins_for_unmatched_non_relative_imports() {
    let root = temp_dir("entry_root_fallback");
    let src = root.join("src");
    let lib = root.join("lib");
    fs::create_dir_all(src.join("helpers")).expect("should create source helpers");
    fs::create_dir_all(lib.join("helpers")).expect("should create root-folder helpers");

    fs::write(
        root.join(settings::CONFIG_FILE_NAME),
        "entry_root #= \"src\"\n",
    )
    .expect("should write config");
    fs::write(
        src.join("#page.bst"),
        "import @helpers/theme\nio.line([: [\"page\"]])\n",
    )
    .expect("should write page");
    fs::write(src.join("helpers/theme.bst"), "io.line([: [\"source\"]])\n")
        .expect("should write source");
    fs::write(
        lib.join("helpers/theme.bst"),
        "io.line([: [\"library\"]])\n",
    )
    .expect("should write root-folder helper");

    let mut config = Config::new(root.clone());
    let style_directives = test_style_directives();
    parse_project_config_for_test(
        &mut config,
        &root.join(settings::CONFIG_FILE_NAME),
        &style_directives,
    )
    .expect("config parse");
    let resolver = configured_resolver(&config);

    let modules = discover_modules_for_test(&config, &resolver, &style_directives)
        .expect("module discovery should pass");
    assert_eq!(modules.len(), 1, "expected exactly one entry module");

    let source_theme = fs::canonicalize(src.join("helpers/theme.bst")).expect("canonical source");
    let _library_theme =
        fs::canonicalize(lib.join("helpers/theme.bst")).expect("canonical library");
    let discovered_paths = modules[0]
        .input_files
        .iter()
        .map(|file| file.source_path.clone())
        .collect::<HashSet<_>>();

    assert!(
        discovered_paths.contains(&source_theme),
        "unmatched non-relative imports should fall back to the entry root"
    );

    fs::remove_dir_all(&root).expect("should remove temp root");
}

#[test]
fn discover_all_modules_finds_multiple_hash_entries_per_root() {
    let root = temp_dir("multi_hash_entries");
    let src = root.join("src");
    fs::create_dir_all(src.join("nested")).expect("should create nested folder");

    fs::write(
        root.join(settings::CONFIG_FILE_NAME),
        "entry_root #= \"src\"\n",
    )
    .expect("should write config");
    fs::write(src.join("#page.bst"), "io.line([: [\"page\"]])\n").expect("should write #page");
    fs::create_dir_all(src.join("layout")).expect("should create layout folder");
    fs::write(
        src.join("layout/#layout.bst"),
        "io.line([: [\"layout\"]])\n",
    )
    .expect("should write #layout");
    fs::write(src.join("nested/#lib.bst"), "io.line([: [\"lib\"]])\n")
        .expect("should write nested #lib");
    fs::write(src.join("nested/file.bst"), "io.line([: [\"regular\"]])\n")
        .expect("should write regular");

    let mut config = Config::new(root.clone());
    let style_directives = test_style_directives();
    parse_project_config_for_test(
        &mut config,
        &root.join(settings::CONFIG_FILE_NAME),
        &style_directives,
    )
    .expect("config parse");
    let resolver = configured_resolver(&config);

    let modules = discover_modules_for_test(&config, &resolver, &style_directives)
        .expect("module discovery should pass");
    assert_eq!(modules.len(), 3, "expected one module per root directory");

    let entry_names = modules
        .iter()
        .map(|module| {
            module
                .entry_point
                .file_name()
                .and_then(OsStr::to_str)
                .unwrap_or_default()
                .to_string()
        })
        .collect::<HashSet<_>>();

    assert!(entry_names.contains("#page.bst"));
    assert!(entry_names.contains("#layout.bst"));
    assert!(entry_names.contains("#lib.bst"));

    fs::remove_dir_all(&root).expect("should remove temp root");
}

#[test]
fn detects_duplicate_config_keys() {
    // Duplicate constants are caught by the header parser during parsing.
    // This test verifies that config parsing properly reports the duplicate key error.
    let root = temp_dir("config_duplicate_keys");
    fs::create_dir_all(&root).expect("should create root dir");
    let config_path = root.join(settings::CONFIG_FILE_NAME);

    fs::write(
        &config_path,
        "entry_root #= \"src\"\ndev_folder #= \"dev\"\nentry_root #= \"other\"\n",
    )
    .expect("should write config");

    let mut config = Config::new(root.clone());
    let style_directives = test_style_directives();
    let messages = parse_project_config_for_test(&mut config, &config_path, &style_directives)
        .expect_err("config should fail");

    let duplicate_diagnostic = messages.error_diagnostics().find(|diagnostic| {
        matches!(
            &diagnostic.payload,
            DiagnosticPayload::InvalidConfig {
                reason: InvalidConfigReason::DuplicateKey,
                ..
            }
        )
    });
    assert!(
        duplicate_diagnostic.is_some(),
        "should have a duplicate config key diagnostic"
    );

    fs::remove_dir_all(&root).expect("should remove temp root");
}

#[test]
fn accepts_folded_template_initializer_for_compile_time_config_binding() {
    let root = temp_dir("config_folded_template");
    fs::create_dir_all(&root).expect("should create root dir");
    let config_path = root.join(settings::CONFIG_FILE_NAME);

    fs::write(&config_path, "project #= [:html]\n").expect("should write config");

    let mut config = Config::new(root.clone());
    let style_directives = test_style_directives();
    parse_project_config_for_test(&mut config, &config_path, &style_directives)
        .expect("folded template initializer should be accepted");

    assert_eq!(
        config.settings.get("project"),
        Some(&"html".to_string()),
        "folded template should become config string value"
    );

    fs::remove_dir_all(&root).expect("should remove temp root");
}

#[test]
fn accepts_config_local_reference_to_earlier_private_const() {
    let root = temp_dir("config_local_reference");
    fs::create_dir_all(&root).expect("should create root dir");
    let config_path = root.join(settings::CONFIG_FILE_NAME);

    fs::write(
        &config_path,
        "output_folder #= \"release\"\ndev_folder #= output_folder\n",
    )
    .expect("should write config");

    let mut config = Config::new(root.clone());
    let style_directives = test_style_directives();
    parse_project_config_for_test(&mut config, &config_path, &style_directives)
        .expect("config with private const reference should succeed");

    assert_eq!(
        config.release_folder,
        PathBuf::from("release"),
        "output_folder should be set"
    );
    assert_eq!(
        config.dev_folder,
        PathBuf::from("release"),
        "dev_folder should resolve through private const reference"
    );

    fs::remove_dir_all(&root).expect("should remove temp root");
}

#[test]
fn rejects_config_unresolved_local_reference() {
    let root = temp_dir("config_unresolved_local_reference");
    fs::create_dir_all(&root).expect("should create root dir");
    let config_path = root.join(settings::CONFIG_FILE_NAME);

    fs::write(
        &config_path,
        "entry_root #= \"src\"\nproject #= missing_value\n",
    )
    .expect("should write config");

    let mut config = Config::new(root.clone());
    let style_directives = test_style_directives();
    let messages = parse_project_config_for_test(&mut config, &config_path, &style_directives)
        .expect_err("config should fail");

    let diagnostic = first_error_diagnostic(&messages);
    assert!(
        matches!(&diagnostic.payload, DiagnosticPayload::UnknownName { .. }),
        "unexpected diagnostic payload: {:?}",
        diagnostic.payload
    );

    fs::remove_dir_all(&root).expect("should remove temp root");
}

#[test]
fn rejects_config_non_compile_time_constant_value() {
    let root = temp_dir("config_non_foldable");
    fs::create_dir_all(&root).expect("should create root dir");
    let config_path = root.join(settings::CONFIG_FILE_NAME);

    fs::write(&config_path, "project #= Error(\"bad\").message\n").expect("should write config");

    let mut config = Config::new(root.clone());
    let style_directives = test_style_directives();
    let messages = parse_project_config_for_test(&mut config, &config_path, &style_directives)
        .expect_err("config should fail");

    let diagnostic = first_error_diagnostic(&messages);
    assert!(
        matches!(
            &diagnostic.payload,
            DiagnosticPayload::CompileTimeEvaluationError {
                reason: CompileTimeEvaluationErrorReason::NonConstantReferenceInConstant,
                ..
            }
        ),
        "unexpected diagnostic payload: {:?}",
        diagnostic.payload
    );

    fs::remove_dir_all(&root).expect("should remove temp root");
}

#[test]
fn accepts_config_private_key_referencing_explicit_const() {
    let root = temp_dir("config_private_ref_explicit");
    fs::create_dir_all(&root).expect("should create root dir");
    let config_path = root.join(settings::CONFIG_FILE_NAME);

    fs::write(
        &config_path,
        "output_folder #= \"release\"\ndev_folder #= output_folder\n",
    )
    .expect("should write config");

    let mut config = Config::new(root.clone());
    let style_directives = test_style_directives();
    parse_project_config_for_test(&mut config, &config_path, &style_directives)
        .expect("private const referencing explicit constant should succeed");

    assert_eq!(
        config.release_folder,
        PathBuf::from("release"),
        "output_folder should be set"
    );
    assert_eq!(
        config.dev_folder,
        PathBuf::from("release"),
        "dev_folder should resolve through private reference to explicit const"
    );

    fs::remove_dir_all(&root).expect("should remove temp root");
}

#[test]
fn rejects_duplicate_plain_config_bindings_before_config_validation() {
    let root = temp_dir("config_duplicate_private");
    fs::create_dir_all(&root).expect("should create root dir");
    let config_path = root.join(settings::CONFIG_FILE_NAME);

    fs::write(
        &config_path,
        "entry_root = \"src\"\nentry_root = \"other\"\n",
    )
    .expect("should write config");

    let mut config = Config::new(root.clone());
    let style_directives = test_style_directives();
    let messages = parse_project_config_for_test(&mut config, &config_path, &style_directives)
        .expect_err("config should fail");

    let diagnostic = first_error_diagnostic(&messages);
    // The frontend catches duplicate start-body declarations as assignments to immutable variables
    // before config validation runs.
    assert!(
        matches!(
            &diagnostic.payload,
            DiagnosticPayload::InvalidAssignmentTarget {
                reason: InvalidAssignmentTargetReason::ImmutableVariable,
                ..
            }
        ),
        "expected immutable-assignment diagnostic for duplicate private keys, got: {:?}",
        diagnostic.payload
    );

    fs::remove_dir_all(&root).expect("should remove temp root");
}

#[test]
fn rejects_config_non_key_private_helper() {
    let root = temp_dir("config_non_key_helper");
    fs::create_dir_all(&root).expect("should create root dir");
    let config_path = root.join(settings::CONFIG_FILE_NAME);

    fs::write(&config_path, "helper #= \"src\"\nentry_root #= helper\n")
        .expect("should write config");

    let mut config = Config::new(root.clone());
    let style_directives = test_style_directives();
    let messages = parse_project_config_for_test(&mut config, &config_path, &style_directives)
        .expect_err("config should fail");

    let diagnostic = first_error_diagnostic(&messages);
    assert!(
        matches!(
            &diagnostic.payload,
            DiagnosticPayload::InvalidConfig {
                reason: InvalidConfigReason::UnknownKey { .. },
                ..
            }
        ),
        "expected unknown key diagnostic for non-key helper, got: {:?}",
        diagnostic.payload
    );

    fs::remove_dir_all(&root).expect("should remove temp root");
}

#[test]
fn rejects_config_runtime_call_in_value() {
    let root = temp_dir("config_runtime_call");
    fs::create_dir_all(&root).expect("should create root dir");
    let config_path = root.join(settings::CONFIG_FILE_NAME);

    fs::write(&config_path, "project #= io.line([: [\"hello\"]])\n").expect("should write config");

    let mut config = Config::new(root.clone());
    let style_directives = test_style_directives();
    let messages = parse_project_config_for_test(&mut config, &config_path, &style_directives)
        .expect_err("config should fail");

    let diagnostic = first_error_diagnostic(&messages);
    assert!(
        matches!(
            &diagnostic.payload,
            DiagnosticPayload::CompileTimeEvaluationError {
                reason: CompileTimeEvaluationErrorReason::ExternalFunctionCallInConstantContext,
                ..
            }
        ),
        "expected external-function-call-in-constant-context diagnostic for runtime call, got: {:?}",
        diagnostic.payload
    );

    fs::remove_dir_all(&root).expect("should remove temp root");
}

// ── Config value shape enforcement tests ──────────────────────────────────────

#[test]
fn accepts_valid_bool_config_keys() {
    let root = temp_dir("config_bool_shape_ok");
    fs::create_dir_all(&root).expect("should create root dir");
    let config_path = root.join(settings::CONFIG_FILE_NAME);

    fs::write(
        &config_path,
        "redirect_index_html #= false\nhtml_inject_core_css #= true\n",
    )
    .expect("should write config");

    let mut config = Config::new(root.clone());
    let style_directives = test_style_directives();
    parse_project_config_for_test_with_html_keys(&mut config, &config_path, &style_directives)
        .expect("valid boolean config values should parse");

    assert_eq!(
        config.settings.get("redirect_index_html"),
        Some(&"false".to_string())
    );
    assert_eq!(
        config.settings.get("html_inject_core_css"),
        Some(&"true".to_string())
    );

    fs::remove_dir_all(&root).expect("should remove temp root");
}

#[test]
fn rejects_core_string_key_with_bool_value() {
    let root = temp_dir("config_string_shape_bool_rejected");
    fs::create_dir_all(&root).expect("should create root dir");
    let config_path = root.join(settings::CONFIG_FILE_NAME);

    fs::write(&config_path, "entry_root #= true\n").expect("should write config");

    let mut config = Config::new(root.clone());
    let style_directives = test_style_directives();
    let messages = parse_project_config_for_test(&mut config, &config_path, &style_directives)
        .expect_err("config should fail");

    let diagnostic = first_error_diagnostic(&messages);
    assert!(
        matches!(
            &diagnostic.payload,
            DiagnosticPayload::InvalidConfig {
                reason: InvalidConfigReason::InvalidConfigValueShape { .. },
                ..
            }
        ),
        "unexpected diagnostic payload: {:?}",
        diagnostic.payload
    );

    fs::remove_dir_all(&root).expect("should remove temp root");
}

#[test]
fn rejects_core_string_key_with_int_value() {
    let root = temp_dir("config_string_shape_int_rejected");
    fs::create_dir_all(&root).expect("should create root dir");
    let config_path = root.join(settings::CONFIG_FILE_NAME);

    fs::write(&config_path, "dev_folder #= 123\n").expect("should write config");

    let mut config = Config::new(root.clone());
    let style_directives = test_style_directives();
    let messages = parse_project_config_for_test(&mut config, &config_path, &style_directives)
        .expect_err("config should fail");

    let diagnostic = first_error_diagnostic(&messages);
    assert!(
        matches!(
            &diagnostic.payload,
            DiagnosticPayload::InvalidConfig {
                reason: InvalidConfigReason::InvalidConfigValueShape { .. },
                ..
            }
        ),
        "unexpected diagnostic payload: {:?}",
        diagnostic.payload
    );

    fs::remove_dir_all(&root).expect("should remove temp root");
}

#[test]
fn rejects_core_string_key_with_char_value() {
    let root = temp_dir("config_string_shape_char_rejected");
    fs::create_dir_all(&root).expect("should create root dir");
    let config_path = root.join(settings::CONFIG_FILE_NAME);

    fs::write(&config_path, "output_folder #= 'x'\n").expect("should write config");

    let mut config = Config::new(root.clone());
    let style_directives = test_style_directives();
    let messages = parse_project_config_for_test(&mut config, &config_path, &style_directives)
        .expect_err("config should fail");

    let diagnostic = first_error_diagnostic(&messages);
    assert!(
        matches!(
            &diagnostic.payload,
            DiagnosticPayload::InvalidConfig {
                reason: InvalidConfigReason::InvalidConfigValueShape { .. },
                ..
            }
        ),
        "unexpected diagnostic payload: {:?}",
        diagnostic.payload
    );

    fs::remove_dir_all(&root).expect("should remove temp root");
}

#[test]
fn rejects_backend_bool_key_with_string_value() {
    let root = temp_dir("config_bool_shape_string_rejected");
    fs::create_dir_all(&root).expect("should create root dir");
    let config_path = root.join(settings::CONFIG_FILE_NAME);

    fs::write(&config_path, "redirect_index_html #= \"false\"\n").expect("should write config");

    let mut config = Config::new(root.clone());
    let style_directives = test_style_directives();
    let messages =
        parse_project_config_for_test_with_html_keys(&mut config, &config_path, &style_directives)
            .expect_err("config should fail");

    let diagnostic = first_error_diagnostic(&messages);
    assert!(
        matches!(
            &diagnostic.payload,
            DiagnosticPayload::InvalidConfig {
                reason: InvalidConfigReason::InvalidConfigValueShape { .. },
                ..
            }
        ),
        "unexpected diagnostic payload: {:?}",
        diagnostic.payload
    );

    fs::remove_dir_all(&root).expect("should remove temp root");
}

#[test]
fn rejects_backend_bool_key_with_int_value() {
    let root = temp_dir("config_bool_shape_int_rejected");
    fs::create_dir_all(&root).expect("should create root dir");
    let config_path = root.join(settings::CONFIG_FILE_NAME);

    fs::write(&config_path, "html_inject_core_css #= 1\n").expect("should write config");

    let mut config = Config::new(root.clone());
    let style_directives = test_style_directives();
    let messages =
        parse_project_config_for_test_with_html_keys(&mut config, &config_path, &style_directives)
            .expect_err("config should fail");

    let diagnostic = first_error_diagnostic(&messages);
    assert!(
        matches!(
            &diagnostic.payload,
            DiagnosticPayload::InvalidConfig {
                reason: InvalidConfigReason::InvalidConfigValueShape { .. },
                ..
            }
        ),
        "unexpected diagnostic payload: {:?}",
        diagnostic.payload
    );

    fs::remove_dir_all(&root).expect("should remove temp root");
}

#[test]
fn rejects_backend_string_key_with_bool_value() {
    let root = temp_dir("config_backend_string_shape_bool_rejected");
    fs::create_dir_all(&root).expect("should create root dir");
    let config_path = root.join(settings::CONFIG_FILE_NAME);

    fs::write(&config_path, "html_lang #= false\n").expect("should write config");

    let mut config = Config::new(root.clone());
    let style_directives = test_style_directives();
    let messages =
        parse_project_config_for_test_with_html_keys(&mut config, &config_path, &style_directives)
            .expect_err("config should fail");

    let diagnostic = first_error_diagnostic(&messages);
    assert!(
        matches!(
            &diagnostic.payload,
            DiagnosticPayload::InvalidConfig {
                reason: InvalidConfigReason::InvalidConfigValueShape { .. },
                ..
            }
        ),
        "unexpected diagnostic payload: {:?}",
        diagnostic.payload
    );

    fs::remove_dir_all(&root).expect("should remove temp root");
}

#[test]
fn rejects_library_folders_with_bool_value() {
    let root = temp_dir("config_library_folders_bool_rejected");
    fs::create_dir_all(&root).expect("should create root dir");
    let config_path = root.join(settings::CONFIG_FILE_NAME);

    fs::write(&config_path, "library_folders #= true\n").expect("should write config");

    let mut config = Config::new(root.clone());
    let style_directives = test_style_directives();
    let messages = parse_project_config_for_test(&mut config, &config_path, &style_directives)
        .expect_err("config should fail");

    let diagnostic = first_error_diagnostic(&messages);
    assert!(
        matches!(
            &diagnostic.payload,
            DiagnosticPayload::InvalidConfig {
                reason: InvalidConfigReason::UnsupportedLibraryFoldersValue,
                ..
            }
        ),
        "unexpected diagnostic payload: {:?}",
        diagnostic.payload
    );

    fs::remove_dir_all(&root).expect("should remove temp root");
}

#[test]
fn rejects_library_folders_with_mixed_collection() {
    let root = temp_dir("config_library_folders_mixed_rejected");
    fs::create_dir_all(&root).expect("should create root dir");
    let config_path = root.join(settings::CONFIG_FILE_NAME);

    fs::write(&config_path, "library_folders #= { \"lib\", 1 }\n").expect("should write config");

    let mut config = Config::new(root.clone());
    let style_directives = test_style_directives();
    // A mixed collection fails during AST type checking before config shape validation.
    // The important behavior is that it is rejected; the exact stage is an implementation detail.
    let messages = parse_project_config_for_test(&mut config, &config_path, &style_directives)
        .expect_err("config should fail");

    assert!(
        messages.error_diagnostics().next().is_some(),
        "expected at least one diagnostic"
    );

    fs::remove_dir_all(&root).expect("should remove temp root");
}

#[test]
fn accepts_library_folders_single_string() {
    let root = temp_dir("config_library_folders_single_string");
    fs::create_dir_all(&root).expect("should create root dir");
    let config_path = root.join(settings::CONFIG_FILE_NAME);

    fs::write(&config_path, "library_folders #= \"lib\"\n").expect("should write config");

    let mut config = Config::new(root.clone());
    let style_directives = test_style_directives();
    parse_project_config_for_test(&mut config, &config_path, &style_directives)
        .expect("single-string library_folders should parse");

    assert_eq!(config.library_folders, vec![PathBuf::from("lib")]);
    assert!(config.has_explicit_library_folders);

    fs::remove_dir_all(&root).expect("should remove temp root");
}

#[test]
fn rejects_closed_string_set_config_key_with_unsupported_value() {
    let root = temp_dir("config_closed_string_set_rejected");
    fs::create_dir_all(&root).expect("should create root dir");
    let config_path = root.join(settings::CONFIG_FILE_NAME);

    fs::write(&config_path, "project #= \"html_wasm\"\n").expect("should write config");

    let mut config = Config::new(root.clone());
    let style_directives = test_style_directives();
    let messages = parse_project_config_for_test(&mut config, &config_path, &style_directives)
        .expect_err("config should fail");

    let diagnostic = first_error_diagnostic(&messages);
    assert!(
        matches!(
            &diagnostic.payload,
            DiagnosticPayload::InvalidConfig {
                reason: InvalidConfigReason::InvalidConfigValueShape { .. },
                ..
            }
        ),
        "unexpected diagnostic payload: {:?}",
        diagnostic.payload
    );

    fs::remove_dir_all(&root).expect("should remove temp root");
}

#[test]
fn deprecated_and_replaced_keys_still_diagnosed_before_shape_check() {
    // WHY: validation ordering must stay stable — deprecated/replaced keys are rejected
    // before shape extraction runs.
    let root = temp_dir("config_deprecated_before_shape");
    fs::create_dir_all(&root).expect("should create root dir");
    let config_path = root.join(settings::CONFIG_FILE_NAME);

    fs::write(&config_path, "src #= true\nlibraries #= 123\n").expect("should write config");

    let mut config = Config::new(root.clone());
    let style_directives = test_style_directives();
    let messages = parse_project_config_for_test(&mut config, &config_path, &style_directives)
        .expect_err("config should fail");

    let diagnostics: Vec<_> = messages.error_diagnostics().collect();
    assert_eq!(diagnostics.len(), 2, "expected two errors");

    assert!(
        matches!(
            &diagnostics[0].payload,
            DiagnosticPayload::InvalidConfig {
                reason: InvalidConfigReason::DeprecatedSrcKey,
                ..
            }
        ),
        "first error should be deprecated key: {:?}",
        diagnostics[0].payload
    );
    assert!(
        matches!(
            &diagnostics[1].payload,
            DiagnosticPayload::InvalidConfig {
                reason: InvalidConfigReason::ReplacedLibrariesKey,
                ..
            }
        ),
        "second error should be replaced key: {:?}",
        diagnostics[1].payload
    );

    fs::remove_dir_all(&root).expect("should remove temp root");
}

#[test]
fn accepts_config_local_reference_after_shape_enforcement() {
    let root = temp_dir("config_local_ref_after_shape");
    fs::create_dir_all(&root).expect("should create root dir");
    let config_path = root.join(settings::CONFIG_FILE_NAME);

    fs::write(
        &config_path,
        "entry_root #= \"src\"\ndev_folder #= entry_root\n",
    )
    .expect("should write config");

    let mut config = Config::new(root.clone());
    let style_directives = test_style_directives();
    parse_project_config_for_test(&mut config, &config_path, &style_directives)
        .expect("config with local const reference should succeed");

    assert_eq!(
        config.entry_root,
        PathBuf::from("src"),
        "entry_root should be set"
    );
    assert_eq!(
        config.dev_folder,
        PathBuf::from("src"),
        "dev_folder should resolve through private const reference"
    );

    fs::remove_dir_all(&root).expect("should remove temp root");
}

#[test]
fn detects_duplicate_top_level_config_constants() {
    let root = temp_dir("config_duplicate_top_level_constants");
    fs::create_dir_all(&root).expect("should create root dir");
    let config_path = root.join(settings::CONFIG_FILE_NAME);

    fs::write(
        &config_path,
        "entry_root #= \"other\"\nentry_root #= \"src\"\n",
    )
    .expect("should write config");

    let mut config = Config::new(root.clone());
    let style_directives = test_style_directives();
    let messages = parse_project_config_for_test(&mut config, &config_path, &style_directives)
        .expect_err("config should fail");

    let diagnostic = first_error_diagnostic(&messages);
    assert!(
        matches!(
            &diagnostic.payload,
            DiagnosticPayload::InvalidConfig {
                reason: InvalidConfigReason::DuplicateKey,
                ..
            }
        ),
        "unexpected diagnostic payload: {:?}",
        diagnostic.payload
    );

    fs::remove_dir_all(&root).expect("should remove temp root");
}

#[test]
fn project_local_lib_directory_is_discovered_as_source_library_root() {
    let root = temp_dir("project_local_lib");
    fs::create_dir_all(&root).expect("should create root dir");
    fs::create_dir_all(root.join("lib/helper")).expect("should create lib/helper");
    fs::create_dir_all(root.join("src")).expect("should create src");
    fs::write(root.join("src/#page.bst"), "x ~= 1\n").expect("should write entry");
    fs::write(root.join("lib/helper/#mod.bst"), "foo #= 1\n").expect("should write facade");
    fs::write(root.join("lib/helper/utils.bst"), "bar #= 2\n").expect("should write lib file");
    fs::write(root.join("config.bst"), "").expect("should write config");

    let config = Config::new(root.clone());
    let mut string_table = StringTable::new();
    let resolver = super::project_roots::build_project_path_resolver(
        &config,
        &crate::libraries::SourceLibraryRegistry::default(),
        &crate::libraries::SourceFileKindRegistry::default(),
        &mut string_table,
    )
    .expect("resolver should build");

    // Import path `@helper/utils` should resolve to the project-local lib root.
    let mut path = crate::compiler_frontend::symbols::interned_path::InternedPath::new();
    path.push_str("helper", &mut string_table);
    path.push_str("utils", &mut string_table);

    let importer = root.join("src/#page.bst");
    let resolved = resolver
        .resolve_import_to_source_file(&path, &importer, &mut string_table)
        .expect("should resolve source library import")
        .path;

    assert_eq!(
        resolved,
        fs::canonicalize(root.join("lib/helper/utils.bst")).unwrap(),
        "should resolve to project-local lib directory"
    );

    fs::remove_dir_all(&root).expect("should remove temp root");
}

#[test]
fn library_prefix_collision_with_builder_library_rejected() {
    let root = temp_dir("lib_collision");
    fs::create_dir_all(&root).expect("should create root dir");
    fs::create_dir_all(root.join("lib/html")).expect("should create lib/html");
    fs::create_dir_all(root.join("src")).expect("should create src");
    fs::write(root.join("src/#page.bst"), "x ~= 1\n").expect("should write entry");
    fs::write(root.join("lib/html/#mod.bst"), "foo #= 1\n").expect("should write facade");
    fs::write(root.join("config.bst"), "").expect("should write config");

    let config = Config::new(root.clone());
    let mut string_table = StringTable::new();

    let mut builder_libraries = crate::libraries::SourceLibraryRegistry::new();
    builder_libraries.register_filesystem_root("html", root.join("builder/html"));

    let result = super::project_roots::build_project_path_resolver(
        &config,
        &builder_libraries,
        &crate::libraries::SourceFileKindRegistry::default(),
        &mut string_table,
    );

    assert!(
        result.is_err(),
        "should fail when builder-provided and project-local libraries collide"
    );
    let messages = result.expect_err("checked above");
    let error_text = rendered_first_error(&messages);
    assert!(
        error_text.contains("collide") || error_text.contains("html"),
        "error should mention collision: {error_text}"
    );
    assert_has_config_error(&messages);
    assert!(matches!(
        first_invalid_config_reason(&messages),
        InvalidConfigReason::SourceLibraryBuilderPrefixCollision { .. }
    ));

    fs::remove_dir_all(&root).expect("should remove temp root");
}

#[test]
fn configured_library_folder_is_discovered_as_source_library_root() {
    let root = temp_dir("project_local_custom_library_folder");
    fs::create_dir_all(&root).expect("should create root dir");
    fs::create_dir_all(root.join("packages/helper")).expect("should create packages/helper");
    fs::create_dir_all(root.join("src")).expect("should create src");
    fs::write(root.join("src/#page.bst"), "x ~= 1\n").expect("should write entry");
    fs::write(root.join("packages/helper/#mod.bst"), "foo #= 1\n").expect("should write facade");
    fs::write(root.join("packages/helper/utils.bst"), "bar #= 2\n").expect("should write lib file");
    fs::write(
        root.join("config.bst"),
        "library_folders #= { \"packages\" }\n",
    )
    .expect("should write config");

    let mut config = Config::new(root.clone());
    let style_directives = test_style_directives();
    parse_project_config_for_test(
        &mut config,
        &root.join(settings::CONFIG_FILE_NAME),
        &style_directives,
    )
    .expect("config should parse");

    let mut string_table = StringTable::new();
    let resolver = super::project_roots::build_project_path_resolver(
        &config,
        &crate::libraries::SourceLibraryRegistry::default(),
        &crate::libraries::SourceFileKindRegistry::default(),
        &mut string_table,
    )
    .expect("resolver should build");

    let mut path = crate::compiler_frontend::symbols::interned_path::InternedPath::new();
    path.push_str("helper", &mut string_table);
    path.push_str("utils", &mut string_table);

    let importer = root.join("src/#page.bst");
    let resolved = resolver
        .resolve_import_to_source_file(&path, &importer, &mut string_table)
        .expect("should resolve source library import")
        .path;

    assert_eq!(
        resolved,
        fs::canonicalize(root.join("packages/helper/utils.bst")).unwrap(),
        "should resolve to configured library folder"
    );

    fs::remove_dir_all(&root).expect("should remove temp root");
}

#[test]
fn missing_explicit_library_folder_is_error() {
    let root = temp_dir("missing_explicit_library_folder");
    fs::create_dir_all(root.join("src")).expect("should create src");
    fs::write(root.join("src/#page.bst"), "x ~= 1\n").expect("should write entry");
    fs::write(
        root.join("config.bst"),
        "library_folders #= { \"packages\" }\n",
    )
    .expect("should write config");

    let mut config = Config::new(root.clone());
    let style_directives = test_style_directives();
    parse_project_config_for_test(
        &mut config,
        &root.join(settings::CONFIG_FILE_NAME),
        &style_directives,
    )
    .expect("config should parse");

    let mut string_table = StringTable::new();
    let result = super::project_roots::build_project_path_resolver(
        &config,
        &crate::libraries::SourceLibraryRegistry::default(),
        &crate::libraries::SourceFileKindRegistry::default(),
        &mut string_table,
    );

    assert!(
        result.is_err(),
        "missing explicitly configured library folder should fail"
    );
    let messages = result.expect_err("checked above");
    let error_text = rendered_first_error(&messages);
    assert!(
        error_text.contains("Configured library folder 'packages' does not exist"),
        "unexpected error message: {error_text}"
    );
    assert_has_config_error(&messages);
    assert!(matches!(
        first_invalid_config_reason(&messages),
        InvalidConfigReason::ConfiguredLibraryFolderMissing { .. }
    ));

    fs::remove_dir_all(&root).expect("should remove temp root");
}

#[test]
fn explicit_library_folder_must_be_directory() {
    let root = temp_dir("library_folder_not_directory");
    fs::create_dir_all(root.join("src")).expect("should create src");
    fs::write(root.join("src/#page.bst"), "x ~= 1\n").expect("should write entry");
    fs::write(root.join("packages"), "").expect("should write file in place of folder");
    fs::write(
        root.join("config.bst"),
        "library_folders #= { \"packages\" }\n",
    )
    .expect("should write config");

    let mut config = Config::new(root.clone());
    let style_directives = test_style_directives();
    parse_project_config_for_test(
        &mut config,
        &root.join(settings::CONFIG_FILE_NAME),
        &style_directives,
    )
    .expect("config should parse");

    let mut string_table = StringTable::new();
    let result = super::project_roots::build_project_path_resolver(
        &config,
        &crate::libraries::SourceLibraryRegistry::default(),
        &crate::libraries::SourceFileKindRegistry::default(),
        &mut string_table,
    );

    let messages = result.expect_err("library scan root file should fail");
    assert!(matches!(
        first_invalid_config_reason(&messages),
        InvalidConfigReason::ConfiguredLibraryFolderNotDirectory { .. }
    ));

    fs::remove_dir_all(&root).expect("should remove temp root");
}

#[test]
fn source_library_requires_one_generic_hash_root() {
    let root = temp_dir("source_library_missing_root");
    fs::create_dir_all(root.join("src")).expect("should create src");
    fs::create_dir_all(root.join("lib/helper")).expect("should create lib/helper");
    fs::write(root.join("src/#page.bst"), "x ~= 1\n").expect("should write entry");
    fs::write(root.join("config.bst"), "entry_root #= \"src\"\n").expect("should write config");

    let mut config = Config::new(root.clone());
    let style_directives = test_style_directives();
    parse_project_config_for_test(
        &mut config,
        &root.join(settings::CONFIG_FILE_NAME),
        &style_directives,
    )
    .expect("config should parse");

    let mut string_table = StringTable::new();
    let result = super::project_roots::build_project_path_resolver(
        &config,
        &crate::libraries::SourceLibraryRegistry::default(),
        &crate::libraries::SourceFileKindRegistry::default(),
        &mut string_table,
    );

    let messages = result.expect_err("source library without a hash root should fail");
    assert!(matches!(
        first_invalid_config_reason(&messages),
        InvalidConfigReason::SourceLibraryMissingRoot { .. }
    ));
    let error_text = rendered_first_error(&messages);
    assert!(error_text.contains("#*.bst"));
    assert!(!error_text.contains("#mod.bst"));

    fs::remove_dir_all(&root).expect("should remove temp root");
}

#[test]
fn source_library_accepts_cosmetic_hash_root_name() {
    let root = temp_dir("source_library_cosmetic_root");
    fs::create_dir_all(root.join("src")).expect("should create src");
    fs::create_dir_all(root.join("lib/helper")).expect("should create lib/helper");
    fs::write(root.join("src/#page.bst"), "x ~= 1\n").expect("should write entry");
    fs::write(root.join("lib/helper/#library.bst"), "foo #= 1\n")
        .expect("should write cosmetic root");
    fs::write(root.join("lib/helper/utils.bst"), "bar #= 2\n")
        .expect("should write library source");
    fs::write(root.join("config.bst"), "entry_root #= \"src\"\n").expect("should write config");

    let mut config = Config::new(root.clone());
    let style_directives = test_style_directives();
    parse_project_config_for_test(
        &mut config,
        &root.join(settings::CONFIG_FILE_NAME),
        &style_directives,
    )
    .expect("config should parse");

    let mut string_table = StringTable::new();
    let resolver = super::project_roots::build_project_path_resolver(
        &config,
        &crate::libraries::SourceLibraryRegistry::default(),
        &crate::libraries::SourceFileKindRegistry::default(),
        &mut string_table,
    )
    .expect("cosmetic source-library root should pass Stage 0 preflight");

    let mut path = crate::compiler_frontend::symbols::interned_path::InternedPath::new();
    path.push_str("helper", &mut string_table);
    let importer = root.join("src/#page.bst");
    let resolved = resolver
        .resolve_import_to_source_file_with_facade_fallback(&path, &importer, &mut string_table)
        .expect("source-library folder import should resolve through the facade pipeline");

    assert_eq!(
        resolved.path,
        fs::canonicalize(root.join("lib/helper/#library.bst")).unwrap()
    );

    fs::remove_dir_all(&root).expect("should remove temp root");
}

#[test]
fn source_library_rejects_multiple_generic_hash_roots() {
    let root = temp_dir("source_library_multiple_roots");
    fs::create_dir_all(root.join("src")).expect("should create src");
    fs::create_dir_all(root.join("lib/helper")).expect("should create lib/helper");
    fs::write(root.join("src/#page.bst"), "x ~= 1\n").expect("should write entry");
    fs::write(root.join("lib/helper/#first.bst"), "foo #= 1\n").expect("should write first root");
    fs::write(root.join("lib/helper/#second.bst"), "bar #= 2\n").expect("should write second root");
    fs::write(root.join("config.bst"), "entry_root #= \"src\"\n").expect("should write config");

    let mut config = Config::new(root.clone());
    let style_directives = test_style_directives();
    parse_project_config_for_test(
        &mut config,
        &root.join(settings::CONFIG_FILE_NAME),
        &style_directives,
    )
    .expect("config should parse");

    let mut string_table = StringTable::new();
    let result = super::project_roots::build_project_path_resolver(
        &config,
        &crate::libraries::SourceLibraryRegistry::default(),
        &crate::libraries::SourceFileKindRegistry::default(),
        &mut string_table,
    );

    let messages = result.expect_err("multiple source-library roots should fail preflight");
    assert!(matches!(
        first_invalid_config_reason(&messages),
        InvalidConfigReason::SourceLibraryMultipleRoots { .. }
    ));
    let error_text = rendered_first_error(&messages);
    assert!(error_text.contains("#first.bst"));
    assert!(error_text.contains("#second.bst"));
    assert!(!error_text.contains("#mod.bst facade"));

    fs::remove_dir_all(&root).expect("should remove temp root");
}

#[test]
fn library_prefix_collision_across_scan_roots_rejected() {
    let root = temp_dir("duplicate_library_prefixes");
    fs::create_dir_all(root.join("lib/helper")).expect("should create lib/helper");
    fs::create_dir_all(root.join("vendor/helper")).expect("should create vendor/helper");
    fs::create_dir_all(root.join("src")).expect("should create src");
    fs::write(root.join("src/#page.bst"), "x ~= 1\n").expect("should write entry");
    fs::write(root.join("lib/helper/#mod.bst"), "foo #= 1\n").expect("should write facade");
    fs::write(root.join("vendor/helper/#mod.bst"), "bar #= 2\n").expect("should write facade");
    fs::write(
        root.join("config.bst"),
        "library_folders #= { \"lib\", \"vendor\" }\n",
    )
    .expect("should write config");

    let mut config = Config::new(root.clone());
    let style_directives = test_style_directives();
    parse_project_config_for_test(
        &mut config,
        &root.join(settings::CONFIG_FILE_NAME),
        &style_directives,
    )
    .expect("config should parse");

    let mut string_table = StringTable::new();
    let result = super::project_roots::build_project_path_resolver(
        &config,
        &crate::libraries::SourceLibraryRegistry::default(),
        &crate::libraries::SourceFileKindRegistry::default(),
        &mut string_table,
    );

    assert!(
        result.is_err(),
        "same source-library prefix discovered from two configured folders should fail"
    );
    let messages = result.expect_err("checked above");
    let error_text = rendered_first_error(&messages);
    assert!(
        error_text.contains("Configured library folder collision"),
        "unexpected error message: {error_text}"
    );
    assert_has_config_error(&messages);
    assert!(matches!(
        first_invalid_config_reason(&messages),
        InvalidConfigReason::SourceLibraryPrefixCollision { .. }
    ));

    fs::remove_dir_all(&root).expect("should remove temp root");
}

#[test]
fn entry_root_requires_at_least_one_root_entry_file() {
    let root = temp_dir("entry_root_without_entries");
    fs::create_dir_all(root.join("src")).expect("should create src");
    fs::write(root.join("config.bst"), "entry_root #= \"src\"\n").expect("should write config");

    let mut config = Config::new(root.clone());
    let style_directives = test_style_directives();
    parse_project_config_for_test(
        &mut config,
        &root.join(settings::CONFIG_FILE_NAME),
        &style_directives,
    )
    .expect("config should parse");

    let resolver = configured_resolver(&config);
    let Err(messages) = discover_modules_for_test(&config, &resolver, &style_directives) else {
        panic!("entry root without #*.bst entries should fail");
    };

    assert!(matches!(
        first_invalid_config_reason(&messages),
        InvalidConfigReason::NoRootModuleEntries { .. }
    ));

    fs::remove_dir_all(&root).expect("should remove temp root");
}

// ── Phase 4 project-structure collision tests ─────────────────────────────────

#[test]
fn rejects_bst_file_and_folder_collision_in_same_directory() {
    let root = temp_dir("bst_folder_collision");
    fs::create_dir_all(root.join("src/ui")).expect("should create src/ui");
    fs::write(root.join("src/ui/#page.bst"), "x ~= 1\n").expect("should write entry");
    fs::write(root.join("src/ui.bst"), "y ~= 2\n").expect("should write colliding file");
    fs::write(root.join("config.bst"), "entry_root #= \"src\"\n").expect("should write config");

    let mut config = Config::new(root.clone());
    let style_directives = test_style_directives();
    parse_project_config_for_test(
        &mut config,
        &root.join(settings::CONFIG_FILE_NAME),
        &style_directives,
    )
    .expect("config should parse");

    let mut string_table = StringTable::new();
    let result = super::project_roots::build_project_path_resolver(
        &config,
        &crate::libraries::SourceLibraryRegistry::default(),
        &crate::libraries::SourceFileKindRegistry::default(),
        &mut string_table,
    );

    assert!(result.is_err(), "ui.bst + ui/ collision should be rejected");
    let messages = result.expect_err("checked above");
    assert_has_config_error(&messages);
    assert!(matches!(
        first_invalid_config_reason(&messages),
        InvalidConfigReason::BstFileFolderCollision { .. }
    ));

    fs::remove_dir_all(&root).expect("should remove temp root");
}

#[test]
fn allows_same_stem_in_different_directories() {
    let root = temp_dir("same_stem_different_dirs");
    fs::create_dir_all(root.join("src/components")).expect("should create src/components");
    fs::create_dir_all(root.join("src/pages")).expect("should create src/pages");
    fs::write(root.join("src/components/card.bst"), "x ~= 1\n").expect("should write card");
    fs::write(root.join("src/pages/card.bst"), "y ~= 2\n").expect("should write another card");
    fs::write(root.join("src/#page.bst"), "z ~= 3\n").expect("should write entry");
    fs::write(root.join("config.bst"), "entry_root #= \"src\"\n").expect("should write config");

    let mut config = Config::new(root.clone());
    let style_directives = test_style_directives();
    parse_project_config_for_test(
        &mut config,
        &root.join(settings::CONFIG_FILE_NAME),
        &style_directives,
    )
    .expect("config should parse");

    let mut string_table = StringTable::new();
    super::project_roots::build_project_path_resolver(
        &config,
        &crate::libraries::SourceLibraryRegistry::default(),
        &crate::libraries::SourceFileKindRegistry::default(),
        &mut string_table,
    )
    .expect("same stem in different directories should be allowed");

    fs::remove_dir_all(&root).expect("should remove temp root");
}

#[test]
fn rejects_collision_with_empty_folder() {
    let root = temp_dir("collision_empty_folder");
    fs::create_dir_all(root.join("src/helper")).expect("should create src/helper");
    fs::write(root.join("src/helper.bst"), "x ~= 1\n").expect("should write colliding file");
    fs::write(root.join("src/#page.bst"), "y ~= 2\n").expect("should write entry");
    fs::write(root.join("config.bst"), "entry_root #= \"src\"\n").expect("should write config");

    let mut config = Config::new(root.clone());
    let style_directives = test_style_directives();
    parse_project_config_for_test(
        &mut config,
        &root.join(settings::CONFIG_FILE_NAME),
        &style_directives,
    )
    .expect("config should parse");

    let mut string_table = StringTable::new();
    let result = super::project_roots::build_project_path_resolver(
        &config,
        &crate::libraries::SourceLibraryRegistry::default(),
        &crate::libraries::SourceFileKindRegistry::default(),
        &mut string_table,
    );

    assert!(
        result.is_err(),
        "collision with an empty folder should be rejected"
    );
    let messages = result.expect_err("checked above");
    assert_has_config_error(&messages);
    assert!(matches!(
        first_invalid_config_reason(&messages),
        InvalidConfigReason::BstFileFolderCollision { .. }
    ));

    fs::remove_dir_all(&root).expect("should remove temp root");
}

#[test]
fn js_file_with_same_stem_as_folder_does_not_trigger_collision() {
    let root = temp_dir("js_same_stem_no_collision");
    fs::create_dir_all(root.join("src/helper")).expect("should create src/helper");
    fs::write(root.join("src/helper.js"), "// js\n").expect("should write js file");
    fs::write(root.join("src/#page.bst"), "x ~= 1\n").expect("should write entry");
    fs::write(root.join("config.bst"), "entry_root #= \"src\"\n").expect("should write config");

    let mut config = Config::new(root.clone());
    let style_directives = test_style_directives();
    parse_project_config_for_test(
        &mut config,
        &root.join(settings::CONFIG_FILE_NAME),
        &style_directives,
    )
    .expect("config should parse");

    let mut string_table = StringTable::new();
    super::project_roots::build_project_path_resolver(
        &config,
        &crate::libraries::SourceLibraryRegistry::default(),
        &crate::libraries::SourceFileKindRegistry::default(),
        &mut string_table,
    )
    .expect(".js file with same stem as folder should not trigger collision");

    fs::remove_dir_all(&root).expect("should remove temp root");
}

#[test]
fn rejects_bst_file_and_folder_collision_in_source_library() {
    let root = temp_dir("source_library_bst_folder_collision");
    fs::create_dir_all(root.join("src")).expect("should create src");
    fs::create_dir_all(root.join("lib/helper/ui")).expect("should create lib/helper/ui");
    fs::write(root.join("src/#page.bst"), "x ~= 1\n").expect("should write entry");
    fs::write(root.join("lib/helper/#mod.bst"), "value #= 1\n").expect("should write facade");
    fs::write(root.join("lib/helper/ui.bst"), "value #= 2\n")
        .expect("should write colliding library file");
    fs::write(
        root.join("config.bst"),
        "entry_root #= \"src\"\nlibrary_folders #= { \"lib\" }\n",
    )
    .expect("should write config");

    let mut config = Config::new(root.clone());
    let style_directives = test_style_directives();
    parse_project_config_for_test(
        &mut config,
        &root.join(settings::CONFIG_FILE_NAME),
        &style_directives,
    )
    .expect("config should parse");

    let mut string_table = StringTable::new();
    let result = super::project_roots::build_project_path_resolver(
        &config,
        &crate::libraries::SourceLibraryRegistry::default(),
        &crate::libraries::SourceFileKindRegistry::default(),
        &mut string_table,
    );

    assert!(
        result.is_err(),
        "source library ui.bst + ui/ collision should be rejected"
    );
    let messages = result.expect_err("checked above");
    assert_has_config_error(&messages);
    assert!(matches!(
        first_invalid_config_reason(&messages),
        InvalidConfigReason::BstFileFolderCollision { .. }
    ));

    fs::remove_dir_all(&root).expect("should remove temp root");
}

#[test]
fn unsupported_js_import_without_provider_reports_bst_import_0021() {
    let root = temp_dir("unsupported_js_import");
    let src = root.join("src");
    fs::create_dir_all(&src).expect("should create src dir");

    fs::write(
        root.join(settings::CONFIG_FILE_NAME),
        "entry_root #= \"src\"\n",
    )
    .expect("should write config");

    // Entry file imports a .js file explicitly.
    fs::write(src.join("#page.bst"), "import @./drawing.js\n#[:ok]\n").expect("should write entry");

    // The .js file actually exists on disk.
    fs::write(src.join("drawing.js"), "export function draw() {}\n").expect("should write js file");

    let mut config = Config::new(root.clone());
    let style_directives = test_style_directives();
    parse_project_config_for_test(
        &mut config,
        &root.join(settings::CONFIG_FILE_NAME),
        &style_directives,
    )
    .expect("config should parse");
    let resolver = configured_resolver(&config);

    let messages = match discover_modules_for_test(&config, &resolver, &style_directives) {
        Ok(_) => panic!("unsupported .js import should fail discovery"),
        Err(messages) => messages,
    };

    let diagnostic = first_error_diagnostic(&messages);
    assert_eq!(
        diagnostic.kind.code(),
        "BST-IMPORT-0021",
        "expected unsupported external extension diagnostic, got {:?}",
        diagnostic
    );
    if let DiagnosticPayload::UnsupportedExternalExtension { path, extension } = &diagnostic.payload
    {
        let path_text = path.to_portable_string(&messages.string_table);
        assert_eq!(path_text, "./drawing.js", "unexpected path in diagnostic");
        assert_eq!(
            messages.string_table.resolve(*extension),
            "js",
            "unexpected extension in diagnostic"
        );
    } else {
        panic!(
            "expected UnsupportedExternalExtension payload, got {:?}",
            diagnostic.payload
        );
    }

    fs::remove_dir_all(&root).expect("should remove temp root");
}

#[test]
fn explicit_bst_extension_still_reports_bst_import_0020() {
    let root = temp_dir("explicit_bst_extension");
    let src = root.join("src");
    fs::create_dir_all(&src).expect("should create src dir");

    fs::write(
        root.join(settings::CONFIG_FILE_NAME),
        "entry_root #= \"src\"\n",
    )
    .expect("should write config");

    fs::write(src.join("#page.bst"), "import @./helper.bst\n#[:ok]\n").expect("should write entry");

    fs::write(
        src.join("helper.bst"),
        "greet || -> String:\n    return \"hi\"\n;\n",
    )
    .expect("should write helper");

    let mut config = Config::new(root.clone());
    let style_directives = test_style_directives();
    parse_project_config_for_test(
        &mut config,
        &root.join(settings::CONFIG_FILE_NAME),
        &style_directives,
    )
    .expect("config should parse");
    let resolver = configured_resolver(&config);

    let messages = match discover_modules_for_test(&config, &resolver, &style_directives) {
        Ok(_) => panic!("explicit .bst extension should fail discovery"),
        Err(messages) => messages,
    };

    let diagnostic = first_error_diagnostic(&messages);
    assert_eq!(
        diagnostic.kind.code(),
        "BST-IMPORT-0020",
        "expected explicit .bst extension diagnostic, got {:?}",
        diagnostic
    );
    assert!(
        matches!(
            &diagnostic.payload,
            DiagnosticPayload::ExplicitBstExtension { .. }
        ),
        "unexpected diagnostic payload: {:?}",
        diagnostic.payload
    );

    fs::remove_dir_all(&root).expect("should remove temp root");
}

#[test]
fn unsupported_beandown_import_without_builder_support_reports_bst_import_0025() {
    let root = temp_dir("unsupported_beandown_import");
    let src = root.join("src");
    fs::create_dir_all(&src).expect("should create src dir");

    fs::write(
        root.join(settings::CONFIG_FILE_NAME),
        "entry_root #= \"src\"\n",
    )
    .expect("should write config");

    fs::write(src.join("#page.bst"), "import @./intro\n#[:ok]\n").expect("should write entry");
    fs::write(src.join("intro.bd"), "hello\n").expect("should write beandown file");

    let mut config = Config::new(root.clone());
    let style_directives = test_style_directives();
    parse_project_config_for_test(
        &mut config,
        &root.join(settings::CONFIG_FILE_NAME),
        &style_directives,
    )
    .expect("config should parse");
    let resolver = configured_resolver(&config);

    let messages = match discover_modules_for_test(&config, &resolver, &style_directives) {
        Ok(_) => panic!("unsupported .bd import should fail discovery"),
        Err(messages) => messages,
    };

    let diagnostic = first_error_diagnostic(&messages);
    assert_eq!(
        diagnostic.kind.code(),
        "BST-IMPORT-0025",
        "expected unsupported source file kind diagnostic, got {:?}",
        diagnostic
    );
    assert!(matches!(
        &diagnostic.payload,
        DiagnosticPayload::UnsupportedSourceFileKind { .. }
    ));

    fs::remove_dir_all(&root).expect("should remove temp root");
}

#[test]
fn direct_beandown_extension_import_reports_bst_import_0024() {
    let root = temp_dir("direct_beandown_extension");
    let src = root.join("src");
    fs::create_dir_all(&src).expect("should create src dir");

    fs::write(
        root.join(settings::CONFIG_FILE_NAME),
        "entry_root #= \"src\"\n",
    )
    .expect("should write config");

    fs::write(src.join("#page.bst"), "import @./intro.bd\n#[:ok]\n").expect("should write entry");
    fs::write(src.join("intro.bd"), "hello\n").expect("should write beandown file");

    let mut config = Config::new(root.clone());
    let style_directives = test_style_directives();
    parse_project_config_for_test(
        &mut config,
        &root.join(settings::CONFIG_FILE_NAME),
        &style_directives,
    )
    .expect("config should parse");

    let mut source_file_kinds = crate::libraries::SourceFileKindRegistry::new();
    source_file_kinds.register("bd", crate::libraries::SourceFileKind::Beandown);
    let resolver = configured_resolver_with_source_file_kinds(&config, &source_file_kinds);

    let messages = match discover_modules_for_test(&config, &resolver, &style_directives) {
        Ok(_) => panic!("direct .bd import should fail discovery"),
        Err(messages) => messages,
    };

    let diagnostic = first_error_diagnostic(&messages);
    assert_eq!(
        diagnostic.kind.code(),
        "BST-IMPORT-0024",
        "expected explicit source extension diagnostic, got {:?}",
        diagnostic
    );
    assert!(matches!(
        &diagnostic.payload,
        DiagnosticPayload::ExplicitSourceExtension { .. }
    ));

    fs::remove_dir_all(&root).expect("should remove temp root");
}

#[test]
fn beandown_files_are_reachable_without_import_scanning() {
    let root = temp_dir("beandown_no_import_scanning");
    let src = root.join("src");
    fs::create_dir_all(&src).expect("should create src dir");

    fs::write(
        root.join(settings::CONFIG_FILE_NAME),
        "entry_root #= \"src\"\n",
    )
    .expect("should write config");

    fs::write(src.join("#page.bst"), "import @./intro\n#[:ok]\n").expect("should write entry");
    fs::write(src.join("intro.bd"), "import @./missing\n").expect("should write beandown file");

    let mut config = Config::new(root.clone());
    let style_directives = test_style_directives();
    parse_project_config_for_test(
        &mut config,
        &root.join(settings::CONFIG_FILE_NAME),
        &style_directives,
    )
    .expect("config should parse");

    let mut source_file_kinds = crate::libraries::SourceFileKindRegistry::new();
    source_file_kinds.register("bd", crate::libraries::SourceFileKind::Beandown);
    let resolver = configured_resolver_with_source_file_kinds(&config, &source_file_kinds);

    let modules = discover_modules_for_test(&config, &resolver, &style_directives)
        .expect(".bd body text must not be scanned for imports");

    let input_paths: HashSet<_> = modules[0]
        .input_files
        .iter()
        .map(|input| input.source_path.file_name().unwrap().to_owned())
        .collect();
    assert!(input_paths.contains(OsStr::new("#page.bst")));
    assert!(input_paths.contains(OsStr::new("intro.bd")));

    let beandown_input = modules[0]
        .input_files
        .iter()
        .find(|input| input.source_path.file_name() == Some(OsStr::new("intro.bd")))
        .expect("intro.bd should be in discovered inputs");
    assert_eq!(
        beandown_input.source_kind,
        crate::libraries::SourceFileKind::Beandown
    );

    fs::remove_dir_all(&root).expect("should remove temp root");
}

#[test]
fn reachable_beandown_queues_same_directory_mod_file() {
    let root = temp_dir("beandown_same_directory_facade");
    let src = root.join("src");
    let docs = src.join("docs");
    fs::create_dir_all(&docs).expect("should create docs dir");

    fs::write(
        root.join(settings::CONFIG_FILE_NAME),
        "entry_root #= \"src\"\n",
    )
    .expect("should write config");

    fs::write(src.join("#page.bst"), "import @docs/intro\n#[:ok]\n").expect("should write entry");
    fs::write(docs.join("intro.bd"), "hello\n").expect("should write beandown file");
    fs::write(docs.join("#mod.bst"), "title #= \"Docs\"\n").expect("should write facade");

    let mut config = Config::new(root.clone());
    let style_directives = test_style_directives();
    parse_project_config_for_test(
        &mut config,
        &root.join(settings::CONFIG_FILE_NAME),
        &style_directives,
    )
    .expect("config should parse");

    let mut source_file_kinds = crate::libraries::SourceFileKindRegistry::new();
    source_file_kinds.register("bd", crate::libraries::SourceFileKind::Beandown);
    let resolver = configured_resolver_with_source_file_kinds(&config, &source_file_kinds);

    let modules = discover_modules_for_test(&config, &resolver, &style_directives)
        .expect("reachable .bd should discover same-directory #mod.bst");

    let input_paths: HashSet<_> = modules[0]
        .input_files
        .iter()
        .map(|input| input.source_path.file_name().unwrap().to_owned())
        .collect();
    assert!(input_paths.contains(OsStr::new("#page.bst")));
    assert!(input_paths.contains(OsStr::new("intro.bd")));
    assert!(input_paths.contains(OsStr::new("#mod.bst")));

    let beandown_input = modules[0]
        .input_files
        .iter()
        .find(|input| input.source_path.file_name() == Some(OsStr::new("intro.bd")))
        .expect("intro.bd should be in discovered inputs");
    assert_eq!(
        beandown_input.source_kind,
        crate::libraries::SourceFileKind::Beandown
    );

    let facade_input = modules[0]
        .input_files
        .iter()
        .find(|input| input.source_path.file_name() == Some(OsStr::new("#mod.bst")))
        .expect("#mod.bst should be in discovered inputs");
    assert_eq!(
        facade_input.source_kind,
        crate::libraries::SourceFileKind::Beanstalk
    );

    fs::remove_dir_all(&root).expect("should remove temp root");
}

#[test]
fn unimported_beandown_file_under_entry_root_is_ignored() {
    let root = temp_dir("unimported_beandown_ignored");
    let src = root.join("src");
    fs::create_dir_all(&src).expect("should create src dir");

    fs::write(
        root.join(settings::CONFIG_FILE_NAME),
        "entry_root #= \"src\"\n",
    )
    .expect("should write config");

    fs::write(src.join("#page.bst"), "#[:ok]\n").expect("should write entry");
    fs::write(src.join("intro.bd"), "hello\n").expect("should write beandown file");

    let mut config = Config::new(root.clone());
    let style_directives = test_style_directives();
    parse_project_config_for_test(
        &mut config,
        &root.join(settings::CONFIG_FILE_NAME),
        &style_directives,
    )
    .expect("config should parse");

    let mut source_file_kinds = crate::libraries::SourceFileKindRegistry::new();
    source_file_kinds.register("bd", crate::libraries::SourceFileKind::Beandown);
    let resolver = configured_resolver_with_source_file_kinds(&config, &source_file_kinds);

    let modules = discover_modules_for_test(&config, &resolver, &style_directives)
        .expect("unimported .bd file should not affect discovery");

    assert_eq!(modules[0].input_files.len(), 1);
    assert_eq!(
        modules[0].input_files[0].source_path.file_name().unwrap(),
        OsStr::new("#page.bst")
    );
    assert_eq!(
        modules[0].input_files[0].source_kind,
        crate::libraries::SourceFileKind::Beanstalk
    );

    fs::remove_dir_all(&root).expect("should remove temp root");
}

#[test]
fn extensionless_bst_import_and_virtual_package_import_still_work() {
    let root = temp_dir("extensionless_and_virtual");
    let src = root.join("src");
    fs::create_dir_all(&src).expect("should create src dir");

    fs::write(
        root.join(settings::CONFIG_FILE_NAME),
        "entry_root #= \"src\"\n",
    )
    .expect("should write config");

    // Normal extensionless imports still resolve as Beanstalk source files, while virtual package
    // imports continue to stay out of Stage 0 filesystem traversal.
    fs::write(
        src.join("#page.bst"),
        "import @./helper\nimport @core/io { line }\n#[:ok]\n",
    )
    .expect("should write entry");

    fs::write(
        src.join("helper.bst"),
        "greet || -> String:\n    return \"hi\"\n;\n",
    )
    .expect("should write helper");

    let mut config = Config::new(root.clone());
    let style_directives = test_style_directives();
    parse_project_config_for_test(
        &mut config,
        &root.join(settings::CONFIG_FILE_NAME),
        &style_directives,
    )
    .expect("config should parse");
    let resolver = configured_resolver(&config);

    let modules = discover_modules_for_test(&config, &resolver, &style_directives)
        .expect("module discovery should pass");
    assert_eq!(modules.len(), 1);

    let discovered = modules[0]
        .input_files
        .iter()
        .map(|file| {
            file.source_path
                .file_name()
                .and_then(OsStr::to_str)
                .unwrap_or_default()
                .to_string()
        })
        .collect::<HashSet<_>>();

    assert!(discovered.contains("#page.bst"));
    assert!(discovered.contains("helper.bst"));

    fs::remove_dir_all(&root).expect("should remove temp root");
}

#[test]
fn reachable_file_discovery_markdown_files_are_reachable_without_import_scanning() {
    let root = temp_dir("markdown_no_import_scanning");
    let src = root.join("src");
    fs::create_dir_all(&src).expect("should create src dir");

    fs::write(
        root.join(settings::CONFIG_FILE_NAME),
        "entry_root #= \"src\"\n",
    )
    .expect("should write config");

    fs::write(src.join("#page.bst"), "import @./intro\n#[:ok]\n").expect("should write entry");
    fs::write(src.join("intro.md"), "import @./missing\n").expect("should write markdown file");

    let mut config = Config::new(root.clone());
    let style_directives = test_style_directives();
    parse_project_config_for_test(
        &mut config,
        &root.join(settings::CONFIG_FILE_NAME),
        &style_directives,
    )
    .expect("config should parse");

    let mut source_file_kinds = crate::libraries::SourceFileKindRegistry::new();
    source_file_kinds.register("bd", crate::libraries::SourceFileKind::Beandown);
    source_file_kinds.register("md", crate::libraries::SourceFileKind::PlainMarkdown);
    let resolver = configured_resolver_with_source_file_kinds(&config, &source_file_kinds);

    let modules = discover_modules_for_test(&config, &resolver, &style_directives)
        .expect(".md body text must not be scanned for imports");

    let input_paths: HashSet<_> = modules[0]
        .input_files
        .iter()
        .map(|input| input.source_path.file_name().unwrap().to_owned())
        .collect();
    assert!(input_paths.contains(OsStr::new("#page.bst")));
    assert!(input_paths.contains(OsStr::new("intro.md")));

    let markdown_input = modules[0]
        .input_files
        .iter()
        .find(|input| input.source_path.file_name() == Some(OsStr::new("intro.md")))
        .expect("intro.md should be in discovered inputs");
    assert_eq!(
        markdown_input.source_kind,
        crate::libraries::SourceFileKind::PlainMarkdown
    );

    fs::remove_dir_all(&root).expect("should remove temp root");
}

#[test]
fn reachable_file_discovery_markdown_does_not_queue_same_directory_mod_file() {
    let root = temp_dir("markdown_no_same_directory_facade");
    let src = root.join("src");
    fs::create_dir_all(src.join("other")).expect("should create other module dir");

    fs::write(
        root.join(settings::CONFIG_FILE_NAME),
        "entry_root #= \"src\"\n",
    )
    .expect("should write config");

    fs::write(src.join("#page.bst"), "import @./intro\n#[:ok]\n").expect("should write entry");
    fs::write(src.join("intro.md"), "hello\n").expect("should write markdown file");
    fs::write(src.join("other/#mod.bst"), "export x #= 1\n")
        .expect("should write other module root");

    let mut config = Config::new(root.clone());
    let style_directives = test_style_directives();
    parse_project_config_for_test(
        &mut config,
        &root.join(settings::CONFIG_FILE_NAME),
        &style_directives,
    )
    .expect("config should parse");

    let mut source_file_kinds = crate::libraries::SourceFileKindRegistry::new();
    source_file_kinds.register("bd", crate::libraries::SourceFileKind::Beandown);
    source_file_kinds.register("md", crate::libraries::SourceFileKind::PlainMarkdown);
    let resolver = configured_resolver_with_source_file_kinds(&config, &source_file_kinds);

    let modules = discover_modules_for_test(&config, &resolver, &style_directives)
        .expect("reachable .md should not discover same-directory #mod.bst");

    let input_paths: HashSet<_> = modules[0]
        .input_files
        .iter()
        .map(|input| input.source_path.file_name().unwrap().to_owned())
        .collect();
    assert!(input_paths.contains(OsStr::new("#page.bst")));
    assert!(input_paths.contains(OsStr::new("intro.md")));
    assert!(!input_paths.contains(OsStr::new("#mod.bst")));

    let markdown_input = modules[0]
        .input_files
        .iter()
        .find(|input| input.source_path.file_name() == Some(OsStr::new("intro.md")))
        .expect("intro.md should be in discovered inputs");
    assert_eq!(
        markdown_input.source_kind,
        crate::libraries::SourceFileKind::PlainMarkdown
    );

    fs::remove_dir_all(&root).expect("should remove temp root");
}

#[test]
fn reachable_file_discovery_unimported_markdown_file_is_ignored() {
    let root = temp_dir("unimported_markdown_ignored");
    let src = root.join("src");
    fs::create_dir_all(&src).expect("should create src dir");

    fs::write(
        root.join(settings::CONFIG_FILE_NAME),
        "entry_root #= \"src\"\n",
    )
    .expect("should write config");

    fs::write(src.join("#page.bst"), "#[:ok]\n").expect("should write entry");
    fs::write(src.join("intro.md"), "hello\n").expect("should write markdown file");

    let mut config = Config::new(root.clone());
    let style_directives = test_style_directives();
    parse_project_config_for_test(
        &mut config,
        &root.join(settings::CONFIG_FILE_NAME),
        &style_directives,
    )
    .expect("config should parse");

    let mut source_file_kinds = crate::libraries::SourceFileKindRegistry::new();
    source_file_kinds.register("bd", crate::libraries::SourceFileKind::Beandown);
    source_file_kinds.register("md", crate::libraries::SourceFileKind::PlainMarkdown);
    let resolver = configured_resolver_with_source_file_kinds(&config, &source_file_kinds);

    let modules = discover_modules_for_test(&config, &resolver, &style_directives)
        .expect("unimported .md file should not affect discovery");

    assert_eq!(modules[0].input_files.len(), 1);
    assert_eq!(
        modules[0].input_files[0].source_path.file_name().unwrap(),
        OsStr::new("#page.bst")
    );
    assert_eq!(
        modules[0].input_files[0].source_kind,
        crate::libraries::SourceFileKind::Beanstalk
    );

    fs::remove_dir_all(&root).expect("should remove temp root");
}

#[test]
fn reachable_file_discovery_direct_markdown_extension_import_reports_bst_import_0024() {
    let root = temp_dir("direct_markdown_extension");
    let src = root.join("src");
    fs::create_dir_all(&src).expect("should create src dir");

    fs::write(
        root.join(settings::CONFIG_FILE_NAME),
        "entry_root #= \"src\"\n",
    )
    .expect("should write config");

    fs::write(src.join("#page.bst"), "import @./intro.md\n#[:ok]\n").expect("should write entry");
    fs::write(src.join("intro.md"), "hello\n").expect("should write markdown file");

    let mut config = Config::new(root.clone());
    let style_directives = test_style_directives();
    parse_project_config_for_test(
        &mut config,
        &root.join(settings::CONFIG_FILE_NAME),
        &style_directives,
    )
    .expect("config should parse");

    let mut source_file_kinds = crate::libraries::SourceFileKindRegistry::new();
    source_file_kinds.register("md", crate::libraries::SourceFileKind::PlainMarkdown);
    let resolver = configured_resolver_with_source_file_kinds(&config, &source_file_kinds);

    let messages = match discover_modules_for_test(&config, &resolver, &style_directives) {
        Ok(_) => panic!("direct .md import should fail discovery"),
        Err(messages) => messages,
    };

    let diagnostic = first_error_diagnostic(&messages);
    assert_eq!(
        diagnostic.kind.code(),
        "BST-IMPORT-0024",
        "expected explicit source extension diagnostic, got {:?}",
        diagnostic
    );
    assert!(matches!(
        &diagnostic.payload,
        DiagnosticPayload::ExplicitSourceExtension { .. }
    ));

    fs::remove_dir_all(&root).expect("should remove temp root");
}

#[test]
fn reachable_file_discovery_unsupported_markdown_import_reports_bst_import_0025() {
    let root = temp_dir("unsupported_markdown_import");
    let src = root.join("src");
    fs::create_dir_all(&src).expect("should create src dir");

    fs::write(
        root.join(settings::CONFIG_FILE_NAME),
        "entry_root #= \"src\"\n",
    )
    .expect("should write config");

    fs::write(src.join("#page.bst"), "import @./intro\n#[:ok]\n").expect("should write entry");
    fs::write(src.join("intro.md"), "hello\n").expect("should write markdown file");

    let mut config = Config::new(root.clone());
    let style_directives = test_style_directives();
    parse_project_config_for_test(
        &mut config,
        &root.join(settings::CONFIG_FILE_NAME),
        &style_directives,
    )
    .expect("config should parse");
    let resolver = configured_resolver(&config);

    let messages = match discover_modules_for_test(&config, &resolver, &style_directives) {
        Ok(_) => panic!("unsupported .md import should fail discovery"),
        Err(messages) => messages,
    };

    let diagnostic = first_error_diagnostic(&messages);
    assert_eq!(
        diagnostic.kind.code(),
        "BST-IMPORT-0025",
        "expected unsupported source file kind diagnostic, got {:?}",
        diagnostic
    );
    assert!(matches!(
        &diagnostic.payload,
        DiagnosticPayload::UnsupportedSourceFileKind { .. }
    ));

    fs::remove_dir_all(&root).expect("should remove temp root");
}

#[test]
fn stage0_reuses_scanned_bst_source_when_assembling_input_files() {
    let root = temp_dir("stage0_reuses_scanned_bst_source");
    let src = root.join("src");
    fs::create_dir_all(&src).expect("should create src dir");

    fs::write(
        root.join(settings::CONFIG_FILE_NAME),
        "entry_root #= \"src\"\n",
    )
    .expect("should write config");
    fs::write(src.join("#page.bst"), "import @./helper\n#[:entry]\n").expect("should write entry");
    fs::write(src.join("helper.bst"), "message #= \"helper\"\n").expect("should write helper");

    let mut config = Config::new(root.clone());
    let style_directives = test_style_directives();
    parse_project_config_for_test(
        &mut config,
        &root.join(settings::CONFIG_FILE_NAME),
        &style_directives,
    )
    .expect("config should parse");
    let resolver = configured_resolver(&config);

    let canonical_root = fs::canonicalize(&root).expect("test root should canonicalize");
    super::source_loading::reset_source_read_count_for_test(&canonical_root);
    let modules = discover_modules_for_test(&config, &resolver, &style_directives)
        .expect("module discovery should pass");

    assert_eq!(modules.len(), 1);
    assert_eq!(
        super::source_loading::source_read_count_for_test(),
        2,
        "entry and helper .bst files should each be read once during import scanning"
    );
    assert_eq!(modules[0].input_files.len(), 2);
    assert!(
        modules[0]
            .input_files
            .iter()
            .any(|input| input.source_code.contains("#[:entry]"))
    );
    assert!(
        modules[0]
            .input_files
            .iter()
            .any(|input| input.source_code.contains("message #="))
    );

    fs::remove_dir_all(&root).expect("should remove temp root");
}

#[test]
fn stage0_loads_asset_sources_and_preserves_deterministic_input_order() {
    let root = temp_dir("stage0_asset_source_loading_order");
    let src = root.join("src");
    fs::create_dir_all(&src).expect("should create src dir");

    fs::write(
        root.join(settings::CONFIG_FILE_NAME),
        "entry_root #= \"src\"\n",
    )
    .expect("should write config");
    fs::write(
        src.join("#page.bst"),
        "import @./intro\nimport @./notes\n#[:entry]\n",
    )
    .expect("should write entry");
    fs::write(src.join("intro.bd"), "beandown body\n").expect("should write beandown");
    fs::write(src.join("notes.md"), "# Markdown body\n").expect("should write markdown");

    let mut config = Config::new(root.clone());
    let style_directives = test_style_directives();
    parse_project_config_for_test(
        &mut config,
        &root.join(settings::CONFIG_FILE_NAME),
        &style_directives,
    )
    .expect("config should parse");

    let mut source_file_kinds = crate::libraries::SourceFileKindRegistry::new();
    source_file_kinds.register("bd", crate::libraries::SourceFileKind::Beandown);
    source_file_kinds.register("md", crate::libraries::SourceFileKind::PlainMarkdown);
    let resolver = configured_resolver_with_source_file_kinds(&config, &source_file_kinds);

    let modules = discover_modules_for_test(&config, &resolver, &style_directives)
        .expect("asset source discovery should pass");
    let input_files = &modules[0].input_files;
    let input_names = input_files
        .iter()
        .map(|input| {
            input
                .source_path
                .file_name()
                .and_then(OsStr::to_str)
                .unwrap_or_default()
                .to_owned()
        })
        .collect::<Vec<_>>();

    assert_eq!(input_names, vec!["#page.bst", "intro.bd", "notes.md"]);
    assert_eq!(
        input_files[1].source_kind,
        crate::libraries::SourceFileKind::Beandown
    );
    assert_eq!(input_files[1].source_code, "beandown body\n");
    assert_eq!(
        input_files[2].source_kind,
        crate::libraries::SourceFileKind::PlainMarkdown
    );
    assert_eq!(input_files[2].source_code, "# Markdown body\n");

    fs::remove_dir_all(&root).expect("should remove temp root");
}

#[test]
fn stage0_parallel_missing_source_loading_preserves_input_order() {
    let root = temp_dir("stage0_parallel_missing_source_order");
    fs::create_dir_all(&root).expect("should create root dir");

    let source_paths = (0..super::reachable_file_discovery::STAGE0_PARALLEL_SOURCE_LOAD_MIN_FILES)
        .map(|index| {
            let path = root.join(format!("asset_{index}.md"));
            fs::write(&path, format!("# Asset {index}\n")).expect("should write markdown asset");
            path
        })
        .collect::<Vec<_>>();
    let mut string_table = StringTable::new();

    let input_files = super::reachable_file_discovery::load_missing_source_paths_for_test(
        source_paths,
        crate::libraries::SourceFileKind::PlainMarkdown,
        &mut string_table,
    )
    .expect("parallel missing source loading should pass");

    let loaded_names = input_files
        .iter()
        .map(|input| {
            input
                .source_path
                .file_name()
                .and_then(OsStr::to_str)
                .unwrap_or_default()
                .to_owned()
        })
        .collect::<Vec<_>>();
    let expected_names = (0
        ..super::reachable_file_discovery::STAGE0_PARALLEL_SOURCE_LOAD_MIN_FILES)
        .map(|index| format!("asset_{index}.md"))
        .collect::<Vec<_>>();

    assert_eq!(loaded_names, expected_names);
    for (index, input_file) in input_files.iter().enumerate() {
        assert_eq!(input_file.source_code, format!("# Asset {index}\n"));
        assert_eq!(
            input_file.source_kind,
            crate::libraries::SourceFileKind::PlainMarkdown
        );
    }

    fs::remove_dir_all(&root).expect("should remove temp root");
}

#[test]
fn stage0_missing_source_load_preserves_file_error_shape() {
    let root = temp_dir("stage0_missing_source_load_error");
    fs::create_dir_all(&root).expect("should create root dir");
    let missing_source = root.join("missing.md");
    let mut string_table = StringTable::new();

    let messages = super::reachable_file_discovery::load_missing_source_path_for_test(
        missing_source.clone(),
        crate::libraries::SourceFileKind::PlainMarkdown,
        &mut string_table,
    )
    .expect_err("missing source read should fail");

    let (_error_type, message, location) = messages
        .first_infrastructure_error_for_tests()
        .expect("expected infrastructure file error");
    assert!(
        message.contains("Error reading file when adding new bst files to parse"),
        "unexpected infrastructure message: {message}"
    );
    assert!(
        location
            .scope
            .to_portable_string(&messages.string_table)
            .contains("missing.md"),
        "missing source path should be preserved in the diagnostic location"
    );

    fs::remove_dir_all(&root).expect("should remove temp root");
}

#[test]
fn provider_backed_imports_are_resolved_without_becoming_source_inputs() {
    let root = temp_dir("provider_imports_not_source_inputs");
    let src = root.join("src");
    fs::create_dir_all(&src).expect("should create src dir");

    fs::write(
        root.join(settings::CONFIG_FILE_NAME),
        "entry_root #= \"src\"\n",
    )
    .expect("should write config");
    fs::write(src.join("#page.bst"), "import @./drawing.js\n#[:entry]\n")
        .expect("should write entry");
    fs::write(src.join("drawing.js"), "export function draw() {}\n").expect("should write js");

    let mut config = Config::new(root.clone());
    let style_directives = test_style_directives();
    parse_project_config_for_test(
        &mut config,
        &root.join(settings::CONFIG_FILE_NAME),
        &style_directives,
    )
    .expect("config should parse");
    let resolver = configured_resolver(&config);

    let calls = Arc::new(AtomicUsize::new(0));
    let mut providers = ExternalImportProviderRegistry::empty();
    providers.register(Arc::new(CountingExternalImportProvider::new(Arc::clone(
        &calls,
    ))));

    let modules =
        discover_modules_for_test_with_providers(&config, &resolver, &style_directives, &providers)
            .expect("provider-backed import should resolve during discovery");

    assert_eq!(calls.load(Ordering::Relaxed), 1);
    assert_eq!(modules[0].input_files.len(), 1);
    assert_eq!(
        modules[0].input_files[0].source_path.file_name().unwrap(),
        OsStr::new("#page.bst")
    );

    fs::remove_dir_all(&root).expect("should remove temp root");
}

#[test]
fn provider_free_multi_entry_discovery_is_deterministic_and_uses_parallel_path() {
    let root = temp_dir("provider_free_multi_entry_deterministic");
    let src = root.join("src");
    fs::create_dir_all(src.join("page_a")).expect("should create page_a module");
    fs::create_dir_all(src.join("page_b")).expect("should create page_b module");
    fs::create_dir_all(src.join("shared")).expect("should create shared dir");

    fs::write(
        root.join(settings::CONFIG_FILE_NAME),
        "entry_root #= \"src\"\n",
    )
    .expect("should write config");

    // Two entry points with overlapping and distinct dependency trees.
    fs::write(
        src.join("page_a/#pageA.bst"),
        "import @shared/helper\nimport @a_only\n#[:pageA]\n",
    )
    .expect("should write pageA");
    fs::write(
        src.join("page_b/#pageB.bst"),
        "import @shared/helper\nimport @b_only\n#[:pageB]\n",
    )
    .expect("should write pageB");
    fs::write(src.join("shared/helper.bst"), "helper #= 1\n").expect("should write helper");
    fs::write(src.join("a_only.bst"), "a #= 1\n").expect("should write a_only");
    fs::write(src.join("b_only.bst"), "b #= 1\n").expect("should write b_only");

    let mut config = Config::new(root.clone());
    let style_directives = test_style_directives();
    parse_project_config_for_test(
        &mut config,
        &root.join(settings::CONFIG_FILE_NAME),
        &style_directives,
    )
    .expect("config should parse");
    let resolver = configured_resolver(&config);

    let canonical_root = fs::canonicalize(&root).expect("test root should canonicalize");
    super::source_loading::reset_source_read_count_for_test(&canonical_root);

    let modules = discover_modules_for_test(&config, &resolver, &style_directives)
        .expect("provider-free multi-entry discovery should pass");

    assert_eq!(
        super::source_loading::source_read_count_for_test(),
        5,
        "provider-free classification should read each unique Beanstalk source once and share the source cache with module discovery"
    );
    assert_eq!(modules.len(), 2, "expected two discovered modules");

    // Module order must follow deterministic entry-point order.
    let module_names: Vec<_> = modules
        .iter()
        .map(|module| {
            module
                .entry_point
                .file_name()
                .and_then(OsStr::to_str)
                .unwrap_or_default()
                .to_string()
        })
        .collect();
    assert_eq!(module_names, vec!["#pageA.bst", "#pageB.bst"]);

    // Per-module input order must be deterministic.
    let module_a_inputs = modules[0]
        .input_files
        .iter()
        .map(|input| {
            input
                .source_path
                .file_name()
                .and_then(OsStr::to_str)
                .unwrap_or_default()
                .to_string()
        })
        .collect::<Vec<_>>();
    let module_b_inputs = modules[1]
        .input_files
        .iter()
        .map(|input| {
            input
                .source_path
                .file_name()
                .and_then(OsStr::to_str)
                .unwrap_or_default()
                .to_string()
        })
        .collect::<Vec<_>>();

    // Reachable files are collected into a `BTreeSet`, so per-module order is deterministic by
    // canonical path (file name within this test).
    assert_eq!(
        module_a_inputs,
        vec!["a_only.bst", "#pageA.bst", "helper.bst"]
    );
    assert_eq!(
        module_b_inputs,
        vec!["b_only.bst", "#pageB.bst", "helper.bst"]
    );

    fs::remove_dir_all(&root).expect("should remove temp root");
}

#[test]
fn provider_backed_import_in_multi_entry_falls_back_to_serial_and_calls_provider() {
    let root = temp_dir("provider_backed_multi_entry_fallback");
    let src = root.join("src");
    fs::create_dir_all(src.join("page_a")).expect("should create page_a module");
    fs::create_dir_all(src.join("page_b")).expect("should create page_b module");

    fs::write(
        root.join(settings::CONFIG_FILE_NAME),
        "entry_root #= \"src\"\n",
    )
    .expect("should write config");

    // Entry A is plain provider-free; entry B imports a .js file.
    fs::write(src.join("page_a/#pageA.bst"), "a #= 1\n").expect("should write pageA");
    fs::write(
        src.join("page_b/#pageB.bst"),
        "import @./drawing.js\n#[:pageB]\n",
    )
    .expect("should write pageB");
    fs::write(src.join("page_b/drawing.js"), "export function draw() {}\n")
        .expect("should write js");

    let mut config = Config::new(root.clone());
    let style_directives = test_style_directives();
    parse_project_config_for_test(
        &mut config,
        &root.join(settings::CONFIG_FILE_NAME),
        &style_directives,
    )
    .expect("config should parse");
    let resolver = configured_resolver(&config);

    let calls = Arc::new(AtomicUsize::new(0));
    let mut providers = ExternalImportProviderRegistry::empty();
    providers.register(Arc::new(CountingExternalImportProvider::new(Arc::clone(
        &calls,
    ))));

    let modules =
        discover_modules_for_test_with_providers(&config, &resolver, &style_directives, &providers)
            .expect("provider-backed multi-entry discovery should fall back and succeed");

    assert_eq!(
        calls.load(Ordering::Relaxed),
        1,
        "provider should be called once"
    );
    assert_eq!(modules.len(), 2);

    // Module A has its own input; module B should only contain the Beanstalk entry, not the .js.
    assert_eq!(modules[0].input_files.len(), 1);
    assert_eq!(modules[1].input_files.len(), 1);
    assert_eq!(
        modules[1].input_files[0].source_path.file_name().unwrap(),
        OsStr::new("#pageB.bst")
    );

    fs::remove_dir_all(&root).expect("should remove temp root");
}

#[test]
fn unsupported_external_extension_in_multi_entry_preserves_diagnostic_shape() {
    let root = temp_dir("unsupported_extension_multi_entry");
    let src = root.join("src");
    fs::create_dir_all(src.join("page_a")).expect("should create page_a module");
    fs::create_dir_all(src.join("page_b")).expect("should create page_b module");

    fs::write(
        root.join(settings::CONFIG_FILE_NAME),
        "entry_root #= \"src\"\n",
    )
    .expect("should write config");

    fs::write(src.join("page_a/#pageA.bst"), "a #= 1\n").expect("should write pageA");
    fs::write(
        src.join("page_b/#pageB.bst"),
        "import @./drawing.js\n#[:pageB]\n",
    )
    .expect("should write pageB");
    fs::write(src.join("page_b/drawing.js"), "export function draw() {}\n")
        .expect("should write js");

    let mut config = Config::new(root.clone());
    let style_directives = test_style_directives();
    parse_project_config_for_test(
        &mut config,
        &root.join(settings::CONFIG_FILE_NAME),
        &style_directives,
    )
    .expect("config should parse");
    let resolver = configured_resolver(&config);

    let messages = match discover_modules_for_test(&config, &resolver, &style_directives) {
        Ok(_) => panic!("unsupported .js import should fail discovery"),
        Err(messages) => messages,
    };

    let diagnostic = first_error_diagnostic(&messages);
    assert_eq!(
        diagnostic.kind.code(),
        "BST-IMPORT-0021",
        "expected unsupported external extension diagnostic, got {:?}",
        diagnostic
    );
    if let DiagnosticPayload::UnsupportedExternalExtension { path, extension } = &diagnostic.payload
    {
        let path_text = path.to_portable_string(&messages.string_table);
        assert_eq!(path_text, "./drawing.js", "unexpected path in diagnostic");
        assert_eq!(
            messages.string_table.resolve(*extension),
            "js",
            "unexpected extension in diagnostic"
        );
    } else {
        panic!(
            "expected UnsupportedExternalExtension payload, got {:?}",
            diagnostic.payload
        );
    }

    fs::remove_dir_all(&root).expect("should remove temp root");
}

#[test]
fn provider_free_parallel_preserves_cross_module_facade_queuing() {
    let root = temp_dir("provider_free_cross_module_facade");
    let src = root.join("src");
    let module_a = src.join("module_a");
    let module_b = src.join("module_b");
    fs::create_dir_all(&module_a).expect("should create module_a");
    fs::create_dir_all(&module_b).expect("should create module_b");

    fs::write(
        root.join(settings::CONFIG_FILE_NAME),
        "entry_root #= \"src\"\n",
    )
    .expect("should write config");

    // Two entry points; entry A imports an implementation file in module B, which should queue
    // module B's root.
    fs::write(
        module_a.join("#pageA.bst"),
        "import @module_b/impl\n#[:pageA]\n",
    )
    .expect("should write pageA");
    fs::write(module_b.join("#mod.bst"), "export b #= 1\n").expect("should write module_b root");
    fs::write(module_b.join("impl.bst"), "impl #= 1\n").expect("should write module_b impl");

    let mut config = Config::new(root.clone());
    let style_directives = test_style_directives();
    parse_project_config_for_test(
        &mut config,
        &root.join(settings::CONFIG_FILE_NAME),
        &style_directives,
    )
    .expect("config should parse");
    let resolver = configured_resolver(&config);

    let modules = discover_modules_for_test(&config, &resolver, &style_directives)
        .expect("cross-module facade discovery should pass");

    assert_eq!(modules.len(), 2);

    let module_a_inputs = modules[0]
        .input_files
        .iter()
        .map(|input| {
            input
                .source_path
                .file_name()
                .and_then(OsStr::to_str)
                .unwrap_or_default()
                .to_string()
        })
        .collect::<Vec<_>>();

    assert!(
        module_a_inputs.contains(&"#mod.bst".to_string()),
        "module B facade should be queued for cross-module import in provider-free parallel path"
    );
    assert!(
        module_a_inputs.contains(&"impl.bst".to_string()),
        "module B impl should be reachable"
    );

    fs::remove_dir_all(&root).expect("should remove temp root");
}
