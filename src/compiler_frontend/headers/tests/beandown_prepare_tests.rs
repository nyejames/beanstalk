//! Beandown synthetic-header preparation tests.
//!
//! WHAT: verifies that `.bd` files enter the frontend as one normal private
//! `content #String` constant with a structurally generated `$markdown` template initializer.

use super::prepare_beandown_file;
use crate::compiler_frontend::FrontendBuildProfile;
use crate::compiler_frontend::ast::ast_nodes::NodeKind;
use crate::compiler_frontend::ast::expressions::expression::ExpressionKind;
use crate::compiler_frontend::ast::{Ast, AstBuildContext, AstBuildInput};
use crate::compiler_frontend::compiler_messages::{
    CompileTimeEvaluationErrorReason, CompilerDiagnostic, DiagnosticBag, DiagnosticKind,
    DiagnosticPayload, SyntaxDiagnosticKind,
};
use crate::compiler_frontend::datatypes::parsed::ParsedTypeRef;
use crate::compiler_frontend::declaration_syntax::binding_mode::BindingMode;
use crate::compiler_frontend::external_packages::ExternalPackageRegistry;
use crate::compiler_frontend::headers::parse_file_headers::{
    FileFrontendPrepareOutput, HeaderKind, HeaderParseOptions, parse_headers,
    prepare_file_from_tokens,
};
use crate::compiler_frontend::headers::types::{FileRole, HeaderExportMode};
use crate::compiler_frontend::module_dependencies::resolve_module_dependencies;
use crate::compiler_frontend::paths::path_format::PathStringFormatConfig;
use crate::compiler_frontend::paths::path_resolution::ProjectPathResolver;
use crate::compiler_frontend::pipeline::{
    CompilerFrontend, FrontendFilePrepareContext, FrontendFilePrepareInput,
};
use crate::compiler_frontend::style_directives::StyleDirectiveRegistry;
use crate::compiler_frontend::symbols::identity::SourceFileTable;
use crate::compiler_frontend::symbols::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::lexer::tokenize;
use crate::compiler_frontend::tokenizer::tokens::{TokenKind, TokenizerEntryMode};
use crate::libraries::external_import_providers::resolution_table::ExternalImportResolutionTable;
use crate::libraries::{SourceFileKind, SourceFileKindRegistry, SourceLibraryRegistry};
use crate::projects::settings::DEFAULT_TEMPLATE_CONST_LOOP_ITERATIONS;
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

fn prepare_directly(source: &str) -> (FileFrontendPrepareOutput, StringTable) {
    let mut string_table = StringTable::new();
    let source_path = InternedPath::from_single_str("test.bd", &mut string_table);
    let style_directives = StyleDirectiveRegistry::built_ins();
    let file_tokens = tokenize(
        source,
        &source_path,
        TokenizerEntryMode::for_source_file_kind(SourceFileKind::Beandown)
            .expect("Beandown should tokenize"),
        &style_directives,
        &mut string_table,
        None,
    )
    .expect("Beandown body should tokenize");

    let output = prepare_beandown_file(file_tokens, &mut string_table);
    (output, string_table)
}

fn prepare_via_pipeline(
    source: &str,
) -> Result<
    FileFrontendPrepareOutput,
    crate::compiler_frontend::headers::parse_file_headers::FileFrontendPrepareError,
> {
    let source_files = SourceFileTable::empty();
    let style_directives = StyleDirectiveRegistry::built_ins();
    let external_package_registry = ExternalPackageRegistry::new();
    let entry_file_path = PathBuf::from("src/#page.bst");
    let options = HeaderParseOptions::default();
    let context = FrontendFilePrepareContext {
        source_files: &source_files,
        style_directives: &style_directives,
        external_package_registry: &external_package_registry,
        entry_file_path: entry_file_path.as_path(),
        options: &options,
    };
    let input_path = PathBuf::from("src/intro.bd");
    let input = FrontendFilePrepareInput {
        source_code: source,
        source_path: &input_path,
        source_kind: SourceFileKind::Beandown,
        const_template_offset: 0,
        runtime_fragment_offset: 0,
    };
    let mut string_table = StringTable::new();

    CompilerFrontend::prepare_file_frontend_local(&context, input, &mut string_table)
}

fn ast_from_beandown_source(source: &str) -> (Ast, StringTable) {
    let source_files = SourceFileTable::empty();
    let style_directives = StyleDirectiveRegistry::built_ins();
    let external_package_registry = ExternalPackageRegistry::new();
    let project_path = std::env::temp_dir();
    let project_path_resolver = ProjectPathResolver::new(
        project_path.clone(),
        project_path,
        &SourceLibraryRegistry::default(),
        &SourceFileKindRegistry::default(),
    )
    .expect("test project path resolver should build");
    let entry_file_path = PathBuf::from("src/#page.bst");
    let options = HeaderParseOptions {
        entry_file_id: None,
        project_path_resolver: Some(project_path_resolver.clone()),
    };
    let context = FrontendFilePrepareContext {
        source_files: &source_files,
        style_directives: &style_directives,
        external_package_registry: &external_package_registry,
        entry_file_path: entry_file_path.as_path(),
        options: &options,
    };
    let input_path = PathBuf::from("src/intro.bd");
    let input = FrontendFilePrepareInput {
        source_code: source,
        source_path: &input_path,
        source_kind: SourceFileKind::Beandown,
        const_template_offset: 0,
        runtime_fragment_offset: 0,
    };
    let mut string_table = StringTable::new();
    let prepared_file =
        CompilerFrontend::prepare_file_frontend_local(&context, input, &mut string_table)
            .expect("Beandown source should prepare");

    let headers = parse_headers(
        vec![prepared_file],
        &external_package_registry,
        &ExternalImportResolutionTable::default(),
        Some(&project_path_resolver),
        &mut string_table,
    )
    .expect("Beandown headers should parse");
    let sorted_headers =
        resolve_module_dependencies(headers, &mut string_table).expect("headers should sort");
    let entry_dir = InternedPath::from_single_str("src/#page.bst", &mut string_table);

    let ast = Ast::new(
        AstBuildInput {
            headers: sorted_headers.headers,
            module_symbols: sorted_headers.module_symbols,
            import_environment: sorted_headers.import_environment,
            top_level_const_fragments: sorted_headers.top_level_const_fragments,
        },
        AstBuildContext {
            external_package_registry: &external_package_registry,
            style_directives: &style_directives,
            string_table: &mut string_table,
            entry_dir,
            build_profile: FrontendBuildProfile::Dev,
            project_path_resolver: Some(project_path_resolver),
            path_format_config: PathStringFormatConfig::default(),
            template_const_loop_iteration_limit: DEFAULT_TEMPLATE_CONST_LOOP_ITERATIONS,
        },
    )
    .expect("Beandown content constant should build through AST");

    (ast, string_table)
}

struct BeandownScopeFixture {
    _temp_dir: TempDir,
    project_root: PathBuf,
    html_facade_path: PathBuf,
    entry_file_path: PathBuf,
    project_path_resolver: ProjectPathResolver,
    source_files: SourceFileTable,
    base_string_table: StringTable,
}

impl BeandownScopeFixture {
    fn new(files: &[(&str, &str)]) -> Self {
        let temp_dir = tempfile::tempdir().expect("test project root should be created");
        let project_root = temp_dir.path().join("project");
        let entry_root = project_root.join("src");
        let html_root = temp_dir.path().join("html_library");

        fs::create_dir_all(&entry_root).expect("entry root should be created");
        fs::create_dir_all(&html_root).expect("HTML source library should be created");
        let project_root =
            fs::canonicalize(project_root).expect("project root should canonicalize");
        let entry_root = fs::canonicalize(entry_root).expect("entry root should canonicalize");
        let html_root = fs::canonicalize(html_root).expect("HTML root should canonicalize");
        let html_facade_path = html_root.join("#mod.bst");

        // The miniature `@html` facade deliberately includes non-constant exports so the
        // Beandown implicit scope proves it is filtering by source declaration kind.
        fs::write(
            &html_facade_path,
            r#"export p #String = "<p>"
export collision #= "html"
export html_defaults #= HtmlDefaults(color = "green")
export HtmlDefaults = | color String |
export render_html || -> String:
    return "runtime"
;
"#,
        )
        .expect("HTML source library facade should be written");

        let mut canonical_files = vec![html_facade_path.clone()];
        for (relative_path, source) in files {
            let path = project_root.join(relative_path);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).expect("source parent should be created");
            }
            fs::write(&path, source).expect("source file should be written");
            canonical_files.push(fs::canonicalize(path).expect("source path should canonicalize"));
        }

        let mut source_libraries = SourceLibraryRegistry::new();
        source_libraries.register_filesystem_root("html", html_root.clone());

        let mut source_file_kinds = SourceFileKindRegistry::new();
        source_file_kinds.register("bd", SourceFileKind::Beandown);

        let project_path_resolver = ProjectPathResolver::new(
            project_root.clone(),
            entry_root.clone(),
            &source_libraries,
            &source_file_kinds,
        )
        .expect("test project path resolver should build");

        let mut string_table = StringTable::new();
        let entry_file_path = entry_root.join("#page.bst");
        let source_files = SourceFileTable::build(
            canonical_files.iter(),
            &entry_file_path,
            Some(&project_path_resolver),
            &mut string_table,
        )
        .expect("source file identities should build");

        Self {
            _temp_dir: temp_dir,
            project_root,
            html_facade_path,
            entry_file_path,
            project_path_resolver,
            source_files,
            base_string_table: string_table,
        }
    }

    fn compile_beandown_ast(
        &self,
        beandown_relative_path: &str,
        prepared_relative_paths: &[&str],
    ) -> Result<(Ast, StringTable), Box<CompilerDiagnostic>> {
        let (ast, string_table) = self.compile_module_ast(prepared_relative_paths)?;

        self.assert_ast_contains_beandown_content(&ast, &string_table, beandown_relative_path);

        Ok((ast, string_table))
    }

    fn compile_module_ast(
        &self,
        prepared_relative_paths: &[&str],
    ) -> Result<(Ast, StringTable), Box<CompilerDiagnostic>> {
        let (headers, mut string_table) = self.parse_headers_for(prepared_relative_paths)?;
        let sorted_headers = resolve_module_dependencies(headers, &mut string_table)
            .map_err(first_diagnostic_from_bag)?;
        let entry_dir = InternedPath::from_path_buf(&self.entry_file_path, &mut string_table);
        let style_directives = StyleDirectiveRegistry::built_ins();
        let external_package_registry = ExternalPackageRegistry::new();

        Ast::new(
            AstBuildInput {
                headers: sorted_headers.headers,
                module_symbols: sorted_headers.module_symbols,
                import_environment: sorted_headers.import_environment,
                top_level_const_fragments: sorted_headers.top_level_const_fragments,
            },
            AstBuildContext {
                external_package_registry: &external_package_registry,
                style_directives: &style_directives,
                string_table: &mut string_table,
                entry_dir,
                build_profile: FrontendBuildProfile::Dev,
                project_path_resolver: Some(self.project_path_resolver.clone()),
                path_format_config: PathStringFormatConfig::default(),
                template_const_loop_iteration_limit: DEFAULT_TEMPLATE_CONST_LOOP_ITERATIONS,
            },
        )
        .map_err(|messages| {
            messages
                .first_error()
                .cloned()
                .map(Box::new)
                .unwrap_or_else(|| panic!("AST failed without a diagnostic"))
        })
        .map(|ast| (ast, string_table))
    }

    fn assert_ast_contains_beandown_content(
        &self,
        ast: &Ast,
        string_table: &StringTable,
        beandown_relative_path: &str,
    ) {
        let logical_beandown_path = beandown_relative_path
            .strip_prefix("src/")
            .unwrap_or(beandown_relative_path);
        let content_suffix = format!("{logical_beandown_path}/content");
        assert!(
            ast.module_constants.iter().any(|constant| {
                constant.id.name_str(string_table) == Some("content")
                    && constant
                        .id
                        .to_portable_string(string_table)
                        .ends_with(&content_suffix)
            }),
            "compiled AST should include Beandown content for {beandown_relative_path}"
        );
    }

    fn parse_headers_for(
        &self,
        prepared_relative_paths: &[&str],
    ) -> Result<
        (
            crate::compiler_frontend::headers::parse_file_headers::Headers,
            StringTable,
        ),
        Box<CompilerDiagnostic>,
    > {
        let style_directives = StyleDirectiveRegistry::built_ins();
        let external_package_registry = ExternalPackageRegistry::new();
        let options = HeaderParseOptions {
            entry_file_id: None,
            project_path_resolver: Some(self.project_path_resolver.clone()),
        };
        let context = FrontendFilePrepareContext {
            source_files: &self.source_files,
            style_directives: &style_directives,
            external_package_registry: &external_package_registry,
            entry_file_path: self.entry_file_path.as_path(),
            options: &options,
        };
        let mut string_table = self.base_string_table.clone();
        let mut prepared_files = Vec::new();

        for relative_path in prepared_relative_paths {
            let source_path = self.source_path_for_fixture_path(relative_path);
            let source_code = fs::read_to_string(&source_path).expect("source should be readable");
            let source_kind = source_path
                .extension()
                .and_then(|extension| extension.to_str())
                .and_then(SourceFileKind::from_extension)
                .unwrap_or(SourceFileKind::Beanstalk);

            let input = FrontendFilePrepareInput {
                source_code: &source_code,
                source_path: &source_path,
                source_kind,
                const_template_offset: 0,
                runtime_fragment_offset: 0,
            };

            let output =
                CompilerFrontend::prepare_file_frontend_local(&context, input, &mut string_table)
                    .map_err(|error| error.diagnostic)?;
            prepared_files.push(output);
        }

        let headers = parse_headers(
            prepared_files,
            &external_package_registry,
            &ExternalImportResolutionTable::default(),
            Some(&self.project_path_resolver),
            &mut string_table,
        )
        .map_err(first_diagnostic_from_bag)?;

        Ok((headers, string_table))
    }

    fn compile_beandown_ast_ok(
        &self,
        beandown_relative_path: &str,
        prepared_relative_paths: &[&str],
    ) -> (Ast, StringTable) {
        self.compile_beandown_ast(beandown_relative_path, prepared_relative_paths)
            .expect("Beandown fixture should compile")
    }

    fn compile_beandown_diagnostic(
        &self,
        beandown_relative_path: &str,
        prepared_relative_paths: &[&str],
    ) -> CompilerDiagnostic {
        match self.compile_beandown_ast(beandown_relative_path, prepared_relative_paths) {
            Ok(_) => panic!("Beandown fixture should fail"),
            Err(diagnostic) => *diagnostic,
        }
    }

    fn project_root_path(&self) -> &Path {
        &self.project_root
    }

    fn source_path_for_fixture_path(&self, relative_path: &str) -> PathBuf {
        if relative_path == "@html/#mod.bst" {
            return self.html_facade_path.clone();
        }

        self.project_root_path().join(relative_path)
    }
}

fn first_diagnostic_from_bag(bag: DiagnosticBag) -> Box<CompilerDiagnostic> {
    Box::new(
        bag.into_diagnostics()
            .into_iter()
            .next()
            .expect("diagnostic bag should contain an error"),
    )
}

fn prepare_beanstalk_source(
    source: &str,
    file_path: &Path,
    entry_file_path: &Path,
    string_table: &mut StringTable,
) -> FileFrontendPrepareOutput {
    let source_path = InternedPath::from_path_buf(file_path, string_table);
    let style_directives = StyleDirectiveRegistry::built_ins();
    let file_tokens = tokenize(
        source,
        &source_path,
        TokenizerEntryMode::SourceFile,
        &style_directives,
        string_table,
        None,
    )
    .expect("Beanstalk source should tokenize");

    prepare_file_from_tokens(
        file_tokens,
        entry_file_path,
        &HeaderParseOptions::default(),
        &ExternalPackageRegistry::new(),
        string_table,
        0,
        0,
    )
    .expect("Beanstalk header preparation should succeed")
}

fn content_constant(
    output: &FileFrontendPrepareOutput,
) -> &crate::compiler_frontend::declaration_syntax::declaration_shell::DeclarationSyntax {
    assert_eq!(output.headers.len(), 1);

    let HeaderKind::Constant { declaration, .. } = &output.headers[0].kind else {
        panic!("Beandown should produce a constant header");
    };

    declaration
}

fn initializer_kinds(output: &FileFrontendPrepareOutput) -> Vec<&TokenKind> {
    content_constant(output)
        .initializer_tokens
        .iter()
        .map(|token| &token.kind)
        .collect()
}

fn folded_content_value(ast: &Ast, string_table: &StringTable) -> String {
    let content = ast
        .module_constants
        .iter()
        .find(|constant| constant.id.name_str(string_table) == Some("content"))
        .expect("Beandown content constant should exist");

    let ExpressionKind::StringSlice(value) = &content.value.kind else {
        panic!(
            "Beandown content should fold to a string slice, got {:?}",
            content.value.kind
        );
    };

    string_table.resolve(*value).to_owned()
}

fn folded_content_contains(ast: &Ast, string_table: &StringTable, expected: &str) {
    let content = folded_content_value(ast, string_table);
    assert!(
        content.contains(expected),
        "folded content should contain {expected:?}, got {content:?}"
    );
}

fn folded_constant_value(ast: &Ast, string_table: &StringTable, name: &str) -> String {
    let constant = ast
        .module_constants
        .iter()
        .find(|constant| constant.id.name_str(string_table) == Some(name))
        .unwrap_or_else(|| panic!("module constant {name} should exist"));

    let ExpressionKind::StringSlice(value) = &constant.value.kind else {
        panic!(
            "module constant {name} should fold to a string slice, got {:?}",
            constant.value.kind
        );
    };

    string_table.resolve(*value).to_owned()
}

#[test]
fn beandown_preparation_produces_private_content_constant() {
    let (output, string_table) = prepare_directly("# Heading");
    let header = &output.headers[0];
    let declaration = content_constant(&output);

    assert_eq!(output.file_role, FileRole::Normal);
    assert!(output.file_imports.is_empty());
    assert!(output.top_level_const_fragments.is_empty());
    assert_eq!(output.runtime_fragment_count, 0);
    assert_eq!(output.const_template_count, 0);
    assert_eq!(header.export_mode, HeaderExportMode::Private);
    assert_eq!(
        header.tokens.src_path.to_portable_string(&string_table),
        "test.bd/content"
    );
    assert_eq!(
        header.source_file.to_portable_string(&string_table),
        "test.bd"
    );
    assert_eq!(header.tokens.canonical_os_path, output.canonical_os_path);
    assert_eq!(declaration.binding_mode, BindingMode::CompileTimeConstant);
    assert!(matches!(
        declaration.type_annotation,
        ParsedTypeRef::BuiltinString { .. }
    ));
}

#[test]
fn empty_beandown_body_folds_to_empty_string() {
    let (ast, string_table) = ast_from_beandown_source("");

    assert_eq!(folded_content_value(&ast, &string_table), "");
}

#[test]
fn simple_markdown_body_folds_like_markdown_template() {
    let (ast, string_table) = ast_from_beandown_source("# Heading");

    assert_eq!(
        folded_content_value(&ast, &string_table),
        "<h1>Heading</h1>"
    );
}

#[test]
fn beandown_compile_time_if_folds_inside_content_constant() {
    let (ast, string_table) = ast_from_beandown_source("[if true: visible]");

    folded_content_contains(&ast, &string_table, "visible");
}

#[test]
fn beandown_compile_time_collection_loop_folds_inside_content_constant() {
    let (ast, string_table) = ast_from_beandown_source(r#"[loop {"one", "two"} |item|: [item] ]"#);

    let content = folded_content_value(&ast, &string_table);
    assert!(
        content.contains("one") && content.contains("two"),
        "folded loop content should contain both collection items, got {content:?}"
    );
}

#[test]
fn empty_beandown_body_generates_markdown_template_initializer() {
    let (output, string_table) = prepare_directly("");
    let kinds = initializer_kinds(&output);

    assert_eq!(kinds.len(), 4);
    assert!(matches!(kinds[0], TokenKind::TemplateHead));
    assert!(matches!(
        kinds[1],
        TokenKind::StyleDirective(id) if string_table.resolve(*id) == "markdown"
    ));
    assert!(matches!(kinds[2], TokenKind::StartTemplateBody));
    assert!(matches!(kinds[3], TokenKind::TemplateClose));
}

#[test]
fn simple_markdown_body_uses_original_body_token_location() {
    let (output, string_table) = prepare_directly("# Heading");
    let declaration = content_constant(&output);
    let body_token = declaration
        .initializer_tokens
        .iter()
        .find(|token| matches!(token.kind, TokenKind::StringSliceLiteral(_)))
        .expect("body text should be preserved as a string literal token");

    assert_eq!(
        body_token.location.scope.to_portable_string(&string_table),
        "test.bd"
    );
    assert!(matches!(
        &body_token.kind,
        TokenKind::StringSliceLiteral(id) if string_table.resolve(*id) == "# Heading"
    ));
}

#[test]
fn nested_templates_remain_structural_inside_markdown_initializer() {
    let (output, string_table) = prepare_directly("before [:inner] after");
    let declaration = content_constant(&output);
    let template_heads = declaration
        .initializer_tokens
        .iter()
        .filter(|token| matches!(token.kind, TokenKind::TemplateHead))
        .collect::<Vec<_>>();

    assert_eq!(template_heads.len(), 2);
    assert_eq!(
        template_heads[1]
            .location
            .scope
            .to_portable_string(&string_table),
        "test.bd"
    );
    assert!(
        template_heads[1].location.start_pos.char_column > 0,
        "nested template opener should keep its original body position, not the synthetic start"
    );
}

#[test]
fn backslash_remains_body_text_inside_markdown_initializer() {
    let (output, string_table) = prepare_directly(r"before \n after");
    let declaration = content_constant(&output);

    assert!(declaration.initializer_tokens.iter().any(|token| matches!(
        &token.kind,
        TokenKind::StringSliceLiteral(id) if string_table.resolve(*id) == r"before \n after"
    )));
}

#[test]
fn unescaped_outer_close_diagnostic_flows_through_pipeline_preparation() {
    let Err(error) = prepare_via_pipeline("]") else {
        panic!("unescaped implicit Beandown close should fail during preparation");
    };

    assert!(error.warnings.is_empty());
    assert_eq!(
        error.diagnostic.kind,
        DiagnosticKind::Syntax(SyntaxDiagnosticKind::UnescapedImplicitTemplateClose)
    );
    assert!(matches!(
        &error.diagnostic.payload,
        DiagnosticPayload::UnescapedImplicitTemplateClose {
            source_kind: SourceFileKind::Beandown
        }
    ));
}

#[test]
fn double_dash_remains_body_text() {
    let (output, string_table) = prepare_directly("alpha -- still text\nbeta");
    let declaration = content_constant(&output);

    assert!(declaration.initializer_tokens.iter().any(|token| matches!(
        &token.kind,
        TokenKind::StringSliceLiteral(id)
            if string_table.resolve(*id) == "alpha -- still text\nbeta"
    )));
}

#[test]
fn declaration_like_text_remains_markdown_body_text() {
    let (output, string_table) = prepare_directly("import @docs/intro\ncontent #String = value");
    let declaration = content_constant(&output);

    assert!(declaration.initializer_references.is_empty());
    assert!(declaration.initializer_tokens.iter().any(|token| matches!(
        &token.kind,
        TokenKind::StringSliceLiteral(id)
            if string_table.resolve(*id) == "import @docs/intro\ncontent #String = value"
    )));
}

#[test]
fn module_facade_export_syntax_can_target_beandown_content() {
    let mut string_table = StringTable::new();
    let facade_path = PathBuf::from("src/#mod.bst");
    let entry_path = PathBuf::from("src/#page.bst");

    let facade_output = prepare_beanstalk_source(
        "export @./intro { content as intro }\n",
        &facade_path,
        &entry_path,
        &mut string_table,
    );

    assert_eq!(facade_output.file_imports.len(), 1);
    assert_eq!(
        facade_output.file_imports[0].export_mode,
        HeaderExportMode::Public
    );
    assert_eq!(
        facade_output.file_imports[0]
            .header_path
            .to_portable_string(&string_table),
        "src/intro/content"
    );
    assert_eq!(
        facade_output.file_imports[0]
            .alias
            .map(|alias| string_table.resolve(alias)),
        Some("intro")
    );
}

#[test]
fn beandown_body_sees_flat_exported_html_constants() {
    let fixture = BeandownScopeFixture::new(&[("src/intro.bd", "[p]")]);
    let (ast, string_table) =
        fixture.compile_beandown_ast_ok("src/intro.bd", &["@html/#mod.bst", "src/intro.bd"]);

    folded_content_contains(&ast, &string_table, "<p>");
}

#[test]
fn beandown_header_visibility_contains_implicit_html_constants() {
    let fixture = BeandownScopeFixture::new(&[("src/intro.bd", "[p]")]);
    let (headers, mut string_table) = fixture
        .parse_headers_for(&["@html/#mod.bst", "src/intro.bd"])
        .expect("headers should parse");
    let beandown_canonical_path = fixture.project_root_path().join("src/intro.bd");
    let beandown_logical_path = fixture
        .project_path_resolver
        .logical_path_for_canonical_file(&beandown_canonical_path, &mut string_table)
        .expect("Beandown logical path should resolve");
    let beandown_source = InternedPath::from_path_buf(&beandown_logical_path, &mut string_table);
    let visibility = headers
        .import_environment
        .visibility_for(&beandown_source)
        .expect("Beandown visibility should exist");
    let p_name = string_table.intern("p");

    assert!(
        visibility.visible_source_names.contains_key(&p_name),
        "Beandown visibility should include @html p; visible names: {:?}",
        visibility
            .visible_source_names
            .keys()
            .map(|name| string_table.resolve(*name).to_owned())
            .collect::<Vec<_>>()
    );
}

#[test]
fn beandown_body_sees_exported_same_directory_facade_constants() {
    let fixture = BeandownScopeFixture::new(&[
        (
            "src/docs/#mod.bst",
            "export local_label #= \"from facade\"\n",
        ),
        ("src/docs/intro.bd", "[local_label]"),
    ]);
    let (ast, string_table) = fixture.compile_beandown_ast_ok(
        "src/docs/intro.bd",
        &["@html/#mod.bst", "src/docs/#mod.bst", "src/docs/intro.bd"],
    );

    folded_content_contains(&ast, &string_table, "from facade");
}

#[test]
fn beandown_without_same_directory_facade_sees_only_html_constants() {
    let fixture = BeandownScopeFixture::new(&[("src/docs/intro.bd", "[collision]")]);
    let (ast, string_table) = fixture.compile_beandown_ast_ok(
        "src/docs/intro.bd",
        &["@html/#mod.bst", "src/docs/intro.bd"],
    );

    folded_content_contains(&ast, &string_table, "html");
}

#[test]
fn same_directory_facade_constants_override_html_constants() {
    let fixture = BeandownScopeFixture::new(&[
        ("src/docs/#mod.bst", "export collision #= \"local\"\n"),
        ("src/docs/intro.bd", "[collision]"),
    ]);
    let (ast, string_table) = fixture.compile_beandown_ast_ok(
        "src/docs/intro.bd",
        &["@html/#mod.bst", "src/docs/#mod.bst", "src/docs/intro.bd"],
    );
    let content = folded_content_value(&ast, &string_table);

    assert!(content.contains("local"));
    assert!(!content.contains("html"));
}

#[test]
fn exported_html_functions_are_not_visible_to_beandown_body() {
    let fixture = BeandownScopeFixture::new(&[("src/intro.bd", "[render_html]")]);
    let diagnostic =
        fixture.compile_beandown_diagnostic("src/intro.bd", &["@html/#mod.bst", "src/intro.bd"]);

    assert!(
        !matches!(
            diagnostic.kind,
            DiagnosticKind::Syntax(SyntaxDiagnosticKind::UnescapedImplicitTemplateClose)
        ),
        "non-constant filtering should fail during semantic lookup, got {diagnostic:?}"
    );
}

#[test]
fn beandown_runtime_function_call_is_rejected_by_const_template_folding() {
    let fixture = BeandownScopeFixture::new(&[
        (
            "src/docs/#mod.bst",
            r#"export render_local || -> String:
    return "runtime"
;
"#,
        ),
        ("src/docs/intro.bd", "[render_local()]"),
    ]);
    let diagnostic = fixture.compile_beandown_diagnostic(
        "src/docs/intro.bd",
        &["@html/#mod.bst", "src/docs/#mod.bst", "src/docs/intro.bd"],
    );

    assert!(
        !matches!(
            diagnostic.kind,
            DiagnosticKind::Syntax(SyntaxDiagnosticKind::UnescapedImplicitTemplateClose)
        ),
        "runtime function calls should fail through semantic const-template rules, got {diagnostic:?}"
    );
}

#[test]
fn beandown_unknown_template_condition_is_rejected_by_const_template_folding() {
    let fixture = BeandownScopeFixture::new(&[("src/intro.bd", "[if show: visible]")]);
    let diagnostic =
        fixture.compile_beandown_diagnostic("src/intro.bd", &["@html/#mod.bst", "src/intro.bd"]);

    assert!(
        matches!(diagnostic.kind, DiagnosticKind::Rule(_)),
        "unknown Beandown template conditions should use normal const diagnostics, got {diagnostic:?}"
    );
}

#[test]
fn exported_same_directory_functions_and_types_are_not_visible_to_beandown_body() {
    let fixture = BeandownScopeFixture::new(&[
        (
            "src/docs/#mod.bst",
            r#"export LocalType = | value String |
export render_local || -> String:
    return "runtime"
;
"#,
        ),
        ("src/docs/intro.bd", "[render_local][LocalType]"),
    ]);
    let diagnostic = fixture.compile_beandown_diagnostic(
        "src/docs/intro.bd",
        &["@html/#mod.bst", "src/docs/#mod.bst", "src/docs/intro.bd"],
    );

    assert!(
        !matches!(
            diagnostic.kind,
            DiagnosticKind::Syntax(SyntaxDiagnosticKind::UnescapedImplicitTemplateClose)
        ),
        "non-constant facade exports should fail during semantic lookup, got {diagnostic:?}"
    );
}

#[test]
fn beandown_const_record_field_access_folds_in_template_head() {
    let fixture = BeandownScopeFixture::new(&[("src/intro.bd", "[html_defaults.color]")]);
    let (ast, string_table) =
        fixture.compile_beandown_ast_ok("src/intro.bd", &["@html/#mod.bst", "src/intro.bd"]);

    folded_content_contains(&ast, &string_table, "green");
}

#[test]
fn facade_supplied_content_constant_can_be_referenced_normally() {
    let fixture = BeandownScopeFixture::new(&[
        ("src/docs/#mod.bst", "export @./other { content }\n"),
        ("src/docs/other.bd", "shared body"),
        ("src/docs/intro.bd", "[content]"),
    ]);
    let (ast, string_table) = fixture.compile_beandown_ast_ok(
        "src/docs/intro.bd",
        &[
            "@html/#mod.bst",
            "src/docs/#mod.bst",
            "src/docs/other.bd",
            "src/docs/intro.bd",
        ],
    );

    folded_content_contains(&ast, &string_table, "shared body");
}

#[test]
fn generated_self_content_is_not_visible_to_beandown_body() {
    let fixture = BeandownScopeFixture::new(&[("src/docs/intro.bd", "[content]")]);
    let diagnostic = fixture.compile_beandown_diagnostic(
        "src/docs/intro.bd",
        &["@html/#mod.bst", "src/docs/intro.bd"],
    );

    assert!(
        matches!(
            diagnostic.payload,
            DiagnosticPayload::CompileTimeEvaluationError {
                reason: CompileTimeEvaluationErrorReason::ConstantNotVisible,
                ..
            }
        ),
        "expected generated self content to be absent from body visibility, got {diagnostic:?}"
    );
}

#[test]
fn self_originating_content_reexport_is_excluded_from_beandown_body_scope() {
    let fixture = BeandownScopeFixture::new(&[
        ("src/docs/#mod.bst", "export @./intro { content }\n"),
        ("src/docs/intro.bd", "[content]"),
    ]);
    let diagnostic = fixture.compile_beandown_diagnostic(
        "src/docs/intro.bd",
        &["@html/#mod.bst", "src/docs/#mod.bst", "src/docs/intro.bd"],
    );

    assert!(
        matches!(
            diagnostic.payload,
            DiagnosticPayload::CompileTimeEvaluationError {
                reason: CompileTimeEvaluationErrorReason::ConstantNotVisible,
                ..
            }
        ),
        "expected self content to be absent from dependency visibility, got {diagnostic:?}"
    );
}

#[test]
fn beanstalk_grouped_imports_beandown_content_as_folded_string_constant() {
    let fixture = BeandownScopeFixture::new(&[
        ("src/#page.bst", ""),
        (
            "src/main.bst",
            "import @./intro { content as intro_content }\nfrom_intro #String = intro_content\n",
        ),
        ("src/intro.bd", "# Intro"),
    ]);
    let (ast, string_table) = fixture
        .compile_module_ast(&[
            "@html/#mod.bst",
            "src/intro.bd",
            "src/main.bst",
            "src/#page.bst",
        ])
        .expect("module using imported Beandown content should compile through AST");

    assert_eq!(
        folded_constant_value(&ast, &string_table, "from_intro"),
        "<h1>Intro</h1>"
    );
}

#[test]
fn beanstalk_namespace_imports_beandown_content_as_folded_string_constant() {
    let fixture = BeandownScopeFixture::new(&[
        ("src/#page.bst", ""),
        (
            "src/main.bst",
            "import @./intro\nfrom_intro #String = intro.content\n",
        ),
        ("src/intro.bd", "# Intro"),
    ]);
    let (ast, string_table) = fixture
        .compile_module_ast(&[
            "@html/#mod.bst",
            "src/main.bst",
            "src/#page.bst",
            "src/intro.bd",
        ])
        .expect("module using namespace-imported Beandown content should compile through AST");

    assert_eq!(
        folded_constant_value(&ast, &string_table, "from_intro"),
        "<h1>Intro</h1>"
    );
}

#[test]
fn imported_bd_file_produces_no_runtime_or_start_behavior() {
    let fixture = BeandownScopeFixture::new(&[
        ("src/#page.bst", ""),
        (
            "src/main.bst",
            "import @./intro\nfrom_intro #String = intro.content\n",
        ),
        ("src/intro.bd", "# Heading"),
    ]);

    let (headers, string_table) = fixture
        .parse_headers_for(&[
            "@html/#mod.bst",
            "src/intro.bd",
            "src/main.bst",
            "src/#page.bst",
        ])
        .expect("headers should parse");

    assert_eq!(
        headers.entry_runtime_fragment_count, 0,
        "module with empty entry should have no runtime fragments"
    );
    assert!(
        headers.top_level_const_fragments.is_empty(),
        "no top-level const fragments from non-entry files"
    );

    let beandown_headers: Vec<_> = headers
        .headers
        .iter()
        .filter(|h| {
            h.source_file
                .to_portable_string(&string_table)
                .ends_with("intro.bd")
        })
        .collect();

    assert_eq!(
        beandown_headers.len(),
        1,
        ".bd file should contribute exactly one header"
    );
    assert!(
        matches!(beandown_headers[0].kind, HeaderKind::Constant { .. }),
        ".bd header should be a constant, got {:?}",
        beandown_headers[0].kind
    );

    let (ast, ast_string_table) = fixture
        .compile_module_ast(&[
            "@html/#mod.bst",
            "src/intro.bd",
            "src/main.bst",
            "src/#page.bst",
        ])
        .expect("module AST should build");

    let bd_function_nodes: Vec<_> = ast
        .nodes
        .iter()
        .filter(|node| {
            matches!(node.kind, NodeKind::Function(..))
                && node
                    .location
                    .scope
                    .to_portable_string(&ast_string_table)
                    .ends_with("intro.bd")
        })
        .collect();

    assert!(
        bd_function_nodes.is_empty(),
        ".bd file should not produce any AST function nodes"
    );

    fixture.assert_ast_contains_beandown_content(&ast, &ast_string_table, "src/intro.bd");
}

#[test]
fn beandown_dynamic_loop_condition_rejected_by_const_folding() {
    let fixture = BeandownScopeFixture::new(&[("src/intro.bd", "[loop show: visible]")]);
    let diagnostic =
        fixture.compile_beandown_diagnostic("src/intro.bd", &["@html/#mod.bst", "src/intro.bd"]);

    assert!(
        matches!(diagnostic.kind, DiagnosticKind::Rule(_)),
        "dynamic Beandown loop conditions should use normal const diagnostics, got {diagnostic:?}"
    );
}

#[test]
fn beandown_external_prelude_call_rejected_by_const_folding() {
    let fixture = BeandownScopeFixture::new(&[("src/intro.bd", "[io.line([: [\"test\"]])]")]);
    let diagnostic =
        fixture.compile_beandown_diagnostic("src/intro.bd", &["@html/#mod.bst", "src/intro.bd"]);

    assert!(
        !matches!(
            diagnostic.kind,
            DiagnosticKind::Syntax(SyntaxDiagnosticKind::UnescapedImplicitTemplateClose)
        ),
        "external prelude calls should fail through semantic const-template rules, got {diagnostic:?}"
    );
}
