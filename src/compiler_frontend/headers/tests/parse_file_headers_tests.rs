//! Header parsing regression tests.
//!
//! WHAT: validates top-level declaration classification, signature extraction, dependency edge
//!       generation, import normalization, and header-level diagnostics.
//! WHY: headers are the first compiler stage after tokenization; incorrect classification or
//!      dependency edges break everything downstream.

use super::*;
use crate::compiler_frontend::compiler_messages::{
    CompilerDiagnostic, DeferredFeatureDiagnosticKind, DeferredFeatureReason, DiagnosticBag,
    DiagnosticKind, DiagnosticPayload, InvalidChoiceVariantReason, InvalidDeclarationReason,
    InvalidFunctionSignatureReason, InvalidSignatureMemberReason, InvalidThisUsageReason,
    InvalidTypeAnnotationReason, ReservedNameOwner, RuleDiagnosticKind, SyntaxDiagnosticKind,
};
use crate::compiler_frontend::datatypes::parsed::ParsedTypeRef;
use crate::compiler_frontend::declaration_syntax::choice::ChoiceVariantPayloadSyntax;
use crate::compiler_frontend::declaration_syntax::signature_members::{
    FunctionReturnSyntax, FunctionSignatureSyntax, ReturnChannelSyntax, ReturnSlotSyntax,
};
use crate::compiler_frontend::external_packages::ExternalPackageRegistry;
use crate::compiler_frontend::headers::types::HeaderExportMode;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::style_directives::StyleDirectiveRegistry;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::lexer::tokenize;
use crate::compiler_frontend::tokenizer::tokens::{
    FileTokens, SourceLocation, Token, TokenKind, TokenizerEntryMode,
};
use crate::compiler_frontend::traits::syntax::TraitThisUsage;
use crate::libraries::external_import_providers::resolution_table::ExternalImportResolutionTable;
use std::path::{Path, PathBuf};

#[derive(Debug)]
struct HeaderTestDiagnostics {
    diagnostics: Vec<CompilerDiagnostic>,
}

struct HeaderTestPrepareContext<'a> {
    entry_file_path: &'a Path,
    options: &'a HeaderParseOptions,
    style_directives: &'a StyleDirectiveRegistry,
    external_package_registry: &'a ExternalPackageRegistry,
}

fn prepare_single_file(
    source: &str,
    file_path: &Path,
    entry_file_path: &Path,
    string_table: &mut StringTable,
) -> FileFrontendPrepareOutput {
    let external_package_registry = ExternalPackageRegistry::new();
    let options = HeaderParseOptions::default();
    let style_directives = StyleDirectiveRegistry::built_ins();
    let interned_path = InternedPath::from_path_buf(file_path, string_table);
    let file_tokens = tokenize(
        source,
        &interned_path,
        TokenizerEntryMode::SourceFile,
        &style_directives,
        string_table,
        None,
    )
    .expect("tokenization should succeed");

    prepare_file_from_tokens(
        file_tokens,
        entry_file_path,
        &options,
        &external_package_registry,
        string_table,
        0,
        0,
    )
    .expect("preparation should succeed")
}

fn prepare_test_source_file(
    source: &str,
    file_path: &Path,
    context: &HeaderTestPrepareContext<'_>,
    string_table: &mut StringTable,
    const_template_offset: usize,
    runtime_fragment_offset: usize,
) -> Result<FileFrontendPrepareOutput, FileFrontendPrepareError> {
    let interned_path = InternedPath::from_path_buf(file_path, string_table);
    let file_tokens = match tokenize(
        source,
        &interned_path,
        TokenizerEntryMode::SourceFile,
        context.style_directives,
        string_table,
        None,
    ) {
        Ok(file_tokens) => file_tokens,
        Err(diagnostic) => {
            return Err(FileFrontendPrepareError {
                warnings: Vec::new(),
                diagnostic: Box::new(diagnostic),
            });
        }
    };

    prepare_file_from_tokens(
        file_tokens,
        context.entry_file_path,
        context.options,
        context.external_package_registry,
        string_table,
        const_template_offset,
        runtime_fragment_offset,
    )
}

fn parse_single_file_headers(source: &str) -> Headers {
    let mut string_table = StringTable::new();
    let file_path = PathBuf::from("src/#page.bst");
    let output = prepare_single_file(source, &file_path, &file_path, &mut string_table);

    parse_headers(
        vec![output],
        &ExternalPackageRegistry::new(),
        &ExternalImportResolutionTable::default(),
        None,
        &mut string_table,
    )
    .expect("headers should parse")
}

fn parse_single_file_headers_with_warnings(
    source: &str,
) -> (
    Headers,
    Vec<crate::compiler_frontend::compiler_messages::CompilerDiagnostic>,
) {
    let mut string_table = StringTable::new();
    let file_path = PathBuf::from("src/#page.bst");
    let output = prepare_single_file(source, &file_path, &file_path, &mut string_table);
    let warnings = output.warnings.clone();

    let headers = parse_headers(
        vec![output],
        &ExternalPackageRegistry::new(),
        &ExternalImportResolutionTable::default(),
        None,
        &mut string_table,
    )
    .expect("headers should parse");

    (headers, warnings)
}

fn parse_single_file_headers_with_table(source: &str) -> (Headers, StringTable) {
    let mut string_table = StringTable::new();
    let file_path = PathBuf::from("src/#page.bst");
    let output = prepare_single_file(source, &file_path, &file_path, &mut string_table);

    let headers = parse_headers(
        vec![output],
        &ExternalPackageRegistry::new(),
        &ExternalImportResolutionTable::default(),
        None,
        &mut string_table,
    )
    .expect("headers should parse");

    (headers, string_table)
}

fn parse_single_file_headers_with_entry(
    source: &str,
    file_path: &str,
    entry_file_path: &str,
) -> Result<Headers, HeaderTestDiagnostics> {
    let mut string_table = StringTable::new();
    let file_path = PathBuf::from(file_path);
    let entry_file_path = PathBuf::from(entry_file_path);
    let external_package_registry = ExternalPackageRegistry::new();
    let options = HeaderParseOptions::default();
    let style_directives = StyleDirectiveRegistry::built_ins();
    let interned_path = InternedPath::from_path_buf(&file_path, &mut string_table);
    let file_tokens = tokenize(
        source,
        &interned_path,
        TokenizerEntryMode::SourceFile,
        &style_directives,
        &mut string_table,
        None,
    )
    .expect("tokenization should succeed");

    let prepare_result = prepare_file_from_tokens(
        file_tokens,
        &entry_file_path,
        &options,
        &external_package_registry,
        &mut string_table,
        0,
        0,
    );

    let output = match prepare_result {
        Ok(output) => output,
        Err(error) => {
            return Err(HeaderTestDiagnostics {
                diagnostics: vec![*error.diagnostic],
            });
        }
    };

    parse_headers(
        vec![output],
        &external_package_registry,
        &ExternalImportResolutionTable::default(),
        options.project_path_resolver.as_ref(),
        &mut string_table,
    )
    .map_err(|bag| HeaderTestDiagnostics {
        diagnostics: bag.into_diagnostics(),
    })
}

fn expect_header_error(
    result: Result<Headers, HeaderTestDiagnostics>,
    message: &str,
) -> HeaderTestDiagnostics {
    match result {
        Ok(_) => panic!("{message}"),
        Err(errors) => errors,
    }
}

fn first_function_signature(headers: &Headers) -> &FunctionSignatureSyntax {
    headers
        .headers
        .iter()
        .find_map(|header| match &header.kind {
            HeaderKind::Function { signature, .. } => Some(signature),
            _ => None,
        })
        .expect("expected function header")
}

fn start_function_header(headers: &Headers) -> &Header {
    headers
        .headers
        .iter()
        .find(|header| matches!(header.kind, HeaderKind::StartFunction))
        .expect("expected start function header")
}

fn non_start_header_names(headers: &Headers, string_table: &StringTable) -> Vec<String> {
    headers
        .headers
        .iter()
        .filter(|header| !matches!(header.kind, HeaderKind::StartFunction))
        .filter_map(|header| {
            header
                .tokens
                .src_path
                .name()
                .map(|name| string_table.resolve(name).to_owned())
        })
        .collect()
}

fn symbol_tokens_in_header_body(header: &Header, string_table: &StringTable) -> Vec<String> {
    header
        .tokens
        .tokens
        .iter()
        .filter_map(|token| match token.kind {
            TokenKind::Symbol(symbol) => Some(string_table.resolve(symbol).to_owned()),
            _ => None,
        })
        .collect()
}

#[test]
fn start_function_dependencies_stay_empty_even_with_imported_runtime_template_tokens() {
    let headers = parse_single_file_headers("func basic()\n[basic]\n");
    let start_header = headers
        .headers
        .iter()
        .find(|header| matches!(header.kind, HeaderKind::StartFunction))
        .expect("expected start function header");

    assert!(
        start_header.dependencies.is_empty(),
        "start function headers must not carry dependency-graph edges"
    );
}

#[test]
fn compile_time_constant_headers_are_parsed() {
    let headers = parse_single_file_headers("theme #= \"dark\"\n");
    assert!(
        headers
            .headers
            .iter()
            .any(|header| matches!(header.kind, HeaderKind::Constant { .. })),
        "expected compile-time constant header"
    );
}

#[test]
fn malformed_children_wrapper_constant_initializer_reports_eof_delimiter_error() {
    let result = parse_single_file_headers_with_entry(
        "broken #= [$children([:<li>[$slot]</li>):\n<ul>[$slot]</ul>\n]\n",
        "src/#page.bst",
        "src/#page.bst",
    );

    assert!(
        result.is_err(),
        "unterminated '$children(..)' wrapper templates should fail instead of hanging"
    );
    let errors = result.err().expect("expected parse errors");

    assert!(errors.diagnostics.iter().any(|diagnostic| matches!(
        diagnostic.payload,
        DiagnosticPayload::UnexpectedEndOfFile { .. }
    )));
}

#[test]
fn malformed_nested_children_wrapper_constant_initializer_reports_eof_delimiter_error() {
    let result = parse_single_file_headers_with_entry(
        "broken #= [$children([:<tr>[$slot]</tr>):\n<table>\n    [$children([:<td>[$slot]</td>):[$slot]]\n</table>\n]\n",
        "src/#page.bst",
        "src/#page.bst",
    );

    assert!(
        result.is_err(),
        "nested unterminated '$children(..)' wrapper templates should fail instead of hanging"
    );
    let errors = result.err().expect("expected parse errors");

    assert!(errors.diagnostics.iter().any(|diagnostic| matches!(
        diagnostic.payload,
        DiagnosticPayload::UnexpectedEndOfFile { .. }
    )));
}

#[test]
fn exported_untyped_constant_has_no_header_provided_dependencies() {
    let headers = parse_single_file_headers("theme #= navbar\n");
    let constant_header = headers
        .headers
        .iter()
        .find(|header| matches!(header.kind, HeaderKind::Constant { .. }))
        .expect("expected constant header");

    assert!(
        constant_header.dependencies.is_empty(),
        "header-provided constant dependencies come from declared type syntax only"
    );
}

#[test]
fn exported_typed_constant_headers_are_parsed_and_follow_on_constant_stays_header() {
    let headers = parse_single_file_headers("page #String = [: world]\n\ntest #= [page: Hello ]\n");

    assert!(
        matches!(
            headers.headers.first().map(|header| &header.kind),
            Some(HeaderKind::Constant { .. })
        ),
        "first header should be parsed as a constant"
    );
    assert!(
        matches!(
            headers.headers.get(1).map(|header| &header.kind),
            Some(HeaderKind::Constant { .. })
        ),
        "follow-on 'test #= ...' should remain a constant header"
    );
}

#[test]
fn non_generic_headers_keep_generic_parameter_lists_empty() {
    let headers = parse_single_file_headers(
        "identity |value Int| -> Int:\n\
             return value\n\
         ;\n\
         Box = |\n\
             value Int,\n\
         |\n\
         Status :: Ready,\n\
         ;\n\
         Alias as Int\n",
    );

    for header in &headers.headers {
        match &header.kind {
            HeaderKind::Function {
                generic_parameters, ..
            }
            | HeaderKind::Struct {
                generic_parameters, ..
            }
            | HeaderKind::Choice {
                generic_parameters, ..
            } => {
                assert!(
                    generic_parameters.parameters.is_empty(),
                    "non-generic declarations should keep generic parameter lists empty"
                );
            }
            _ => {}
        }
    }
}

#[test]
fn generic_declaration_headers_parse_parameter_lists() {
    let (headers, string_table) = parse_single_file_headers_with_table(
        "identity type T |value T| -> T:\n\
             return value\n\
         ;\n\
         Box type Item = |\n\
             value Item,\n\
         |\n\
         ResultShape type OkType, ErrType ::\n\
             Ok | value OkType |,\n\
             Err | error ErrType |,\n\
         ;\n",
    );

    let mut generic_parameter_counts = Vec::new();
    for header in &headers.headers {
        match &header.kind {
            HeaderKind::Function {
                generic_parameters, ..
            }
            | HeaderKind::Struct {
                generic_parameters, ..
            }
            | HeaderKind::Choice {
                generic_parameters, ..
            } => generic_parameter_counts.push(generic_parameters.len()),
            _ => {}
        }
    }

    assert_eq!(generic_parameter_counts, vec![1, 1, 2]);
    assert_eq!(
        headers.module_symbols.generic_declarations_by_path.len(),
        3,
        "only declarations with generic parameters should be registered as generic declarations"
    );

    let generic_names = headers
        .module_symbols
        .generic_declarations_by_path
        .values()
        .flat_map(|metadata| {
            metadata
                .parameters
                .parameters
                .iter()
                .map(|parameter| string_table.resolve(parameter.name).to_owned())
        })
        .collect::<Vec<_>>();

    assert!(generic_names.contains(&"T".to_owned()));
    assert!(generic_names.contains(&"Item".to_owned()));
    assert!(generic_names.contains(&"OkType".to_owned()));
    assert!(generic_names.contains(&"ErrType".to_owned()));
}

#[test]
fn top_level_const_template_outside_entry_file_errors() {
    let result = parse_single_file_headers_with_entry(
        "#[html.head: [\"x\"]]\n",
        "src/lib.bst",
        "src/#page.bst",
    );

    assert!(
        result.is_err(),
        "const templates outside the entry file should error"
    );
}

#[test]
fn top_level_const_template_tokens_keep_close_and_eof_for_ast_parser() {
    let headers = parse_single_file_headers("#[3]\n");

    let const_template_header = headers
        .headers
        .iter()
        .find(|header| matches!(header.kind, HeaderKind::ConstTemplate { .. }))
        .expect("expected top-level const template header");

    assert!(
        matches!(
            const_template_header
                .tokens
                .tokens
                .first()
                .map(|token| &token.kind),
            Some(TokenKind::TemplateHead)
        ),
        "const template token stream should start with template opener"
    );

    assert!(
        const_template_header
            .tokens
            .tokens
            .iter()
            .any(|token| matches!(token.kind, TokenKind::TemplateClose)),
        "const template token stream should preserve template close token"
    );

    assert!(
        matches!(
            const_template_header
                .tokens
                .tokens
                .last()
                .map(|token| &token.kind),
            Some(TokenKind::Eof)
        ),
        "const template token stream should end with EOF sentinel"
    );
}

#[test]
fn top_level_const_template_collects_if_condition_dependency_refs() {
    let (headers, string_table) = parse_single_file_headers_with_table(
        "#[if show_banner:
            [if maybe_name is |name|:
                [name]
            ]
        ]\n",
    );

    let const_template_header = headers
        .headers
        .iter()
        .find(|header| matches!(header.kind, HeaderKind::ConstTemplate { .. }))
        .expect("expected top-level const template header");
    let HeaderKind::ConstTemplate {
        condition_references,
        ..
    } = &const_template_header.kind
    else {
        panic!("expected const template header");
    };

    let names = condition_references
        .iter()
        .map(|reference| string_table.resolve(reference.name))
        .collect::<Vec<_>>();

    assert_eq!(names, vec!["show_banner", "maybe_name"]);
}

#[test]
fn start_function_local_references_do_not_create_module_dependencies() {
    let headers = parse_single_file_headers(
        "value = 1\n\
         another = value + 1\n\
         io(another)\n",
    );

    let start_header = headers
        .headers
        .iter()
        .find(|header| matches!(header.kind, HeaderKind::StartFunction))
        .expect("expected start function header");

    assert!(
        start_header.dependencies.is_empty(),
        "local start-function symbols must not be tracked as inter-header/module dependencies"
    );
}

#[test]
fn loop_binding_symbols_remain_in_start_function_body() {
    let (headers, string_table) = parse_single_file_headers_with_table(
        "items = {1, 2, 3}\n\
         \n\
         loop items |item, index|:\n\
             io(item)\n\
         ;\n",
    );

    assert_eq!(
        headers.headers.len(),
        1,
        "loop-only top-level files should emit only the implicit start header"
    );
    assert!(matches!(headers.headers[0].kind, HeaderKind::StartFunction));

    let start_header = start_function_header(&headers);
    let start_symbols = symbol_tokens_in_header_body(start_header, &string_table);
    let header_names = non_start_header_names(&headers, &string_table);

    assert!(
        start_symbols.iter().any(|symbol| symbol == "item"),
        "loop item binding should stay in the implicit start body token stream"
    );
    assert!(
        start_symbols.iter().any(|symbol| symbol == "index"),
        "loop index binding should stay in the implicit start body token stream"
    );
    assert!(
        start_header
            .tokens
            .tokens
            .iter()
            .any(|token| matches!(token.kind, TokenKind::Loop)),
        "start header should preserve the top-level loop statement tokens"
    );
    assert!(
        !header_names
            .iter()
            .any(|name| name == "item" || name == "index"),
        "loop binding names must never be elevated into headers"
    );
}

#[test]
fn top_level_expression_symbols_stay_in_implicit_start_body() {
    let (headers, string_table) = parse_single_file_headers_with_table(
        "func basic()\n\
         items = {1, 2, 3}\n\
         loop items |item, index|:\n\
             io(item)\n\
         ;\n\
         [basic]\n\
         basic()\n\
         items\n",
    );

    assert_eq!(
        headers.headers.len(),
        1,
        "imports and top-level expressions should still collapse into one start header here"
    );
    assert!(matches!(headers.headers[0].kind, HeaderKind::StartFunction));

    let start_header = start_function_header(&headers);
    let start_symbols = symbol_tokens_in_header_body(start_header, &string_table);
    let header_names = non_start_header_names(&headers, &string_table);

    assert!(
        start_symbols.iter().any(|symbol| symbol == "basic"),
        "imported symbol usage in expression/template position should stay in start body"
    );
    assert!(
        start_symbols.iter().any(|symbol| symbol == "item")
            && start_symbols.iter().any(|symbol| symbol == "index"),
        "loop binding symbols inside top-level loops should remain start-body tokens"
    );
    assert!(
        start_header
            .tokens
            .tokens
            .iter()
            .any(|token| matches!(token.kind, TokenKind::TemplateHead)),
        "runtime top-level templates should remain in the start-function token stream"
    );
    assert!(
        !header_names
            .iter()
            .any(|name| name == "basic" || name == "items" || name == "item" || name == "index"),
        "expression-position symbols must not be misclassified as top-level declaration headers"
    );
}

#[test]
fn compile_time_declarations_parse_as_headers_without_elevating_body_symbols() {
    let (headers, string_table) = parse_single_file_headers_with_table(
        "theme #= \"dark\"\n\
         items = {theme}\n\
         loop items |item, index|:\n\
             io(item)\n\
         ;\n\
         [theme]\n\
         theme\n",
    );

    let header_names = non_start_header_names(&headers, &string_table);
    assert_eq!(
        header_names,
        vec![String::from("theme")],
        "the `theme #= ...` declaration should remain a real top-level constant header"
    );
    assert_eq!(
        headers.headers.len(),
        2,
        "expected one compile-time constant header plus the implicit start header"
    );
    assert!(
        headers
            .headers
            .iter()
            .any(|header| matches!(header.kind, HeaderKind::Constant { .. })),
        "compile-time binding syntax should classify as a constant header"
    );

    let start_header = start_function_header(&headers);
    let start_symbols = symbol_tokens_in_header_body(start_header, &string_table);

    assert!(
        start_symbols.iter().any(|symbol| symbol == "theme"),
        "same-name symbol uses later in top-level expressions should stay in start body"
    );
    assert!(
        start_symbols.iter().any(|symbol| symbol == "item")
            && start_symbols.iter().any(|symbol| symbol == "index"),
        "loop-binding symbols in start-body statements must not become headers"
    );
    assert!(
        !header_names
            .iter()
            .any(|name| name == "items" || name == "item" || name == "index"),
        "only legitimate '#'-prefixed declarations should become headers"
    );
}

#[test]
fn function_without_arrow_has_zero_return_slots() {
    let headers = parse_single_file_headers("f||:\n;\n");
    let signature = first_function_signature(&headers);

    assert!(signature.returns.is_empty());
}

#[test]
fn function_value_return_is_preserved_as_return_syntax_shell() {
    let headers = parse_single_file_headers("f|| -> Int:\n;\n");
    let signature = first_function_signature(&headers);

    assert!(matches!(
        signature.returns.as_slice(),
        [ReturnSlotSyntax {
            value: FunctionReturnSyntax::Value {
                type_annotation: ParsedTypeRef::BuiltinInt { .. },
                ..
            },
            channel: ReturnChannelSyntax::Success,
            ..
        }]
    ));
}

#[test]
fn function_named_return_is_preserved_for_ast_resolution() {
    let headers = parse_single_file_headers("f|| -> Point:\n;\n");
    let signature = first_function_signature(&headers);

    assert!(matches!(
        signature.returns.as_slice(),
        [ReturnSlotSyntax {
            value: FunctionReturnSyntax::Value {
                type_annotation: ParsedTypeRef::Named { .. },
                ..
            },
            channel: ReturnChannelSyntax::Success,
            ..
        }]
    ));
}

#[test]
fn function_alias_return_is_preserved_as_parameter_reference_shell() {
    let headers = parse_single_file_headers("f|x Int| -> x:\n;\n");
    let signature = first_function_signature(&headers);

    assert!(matches!(
        signature.returns.as_slice(),
        [ReturnSlotSyntax {
            value: FunctionReturnSyntax::AliasCandidates {
                parameter_indices,
                ..
            },
            channel: ReturnChannelSyntax::Success,
            ..
        }] if parameter_indices == &vec![0]
    ));
}

#[test]
fn function_parameter_default_stays_in_header_syntax_tokens() {
    let (headers, string_table) =
        parse_single_file_headers_with_table("label |prefix String = \"item\"| -> String:\n;\n");
    let signature = first_function_signature(&headers);

    let parameter = signature
        .parameters
        .first()
        .expect("expected one parameter shell");
    assert!(matches!(
        parameter.type_annotation,
        ParsedTypeRef::BuiltinString { .. }
    ));
    assert!(
        parameter.default_tokens.iter().any(|token| matches!(
            token.kind,
            TokenKind::StringSliceLiteral(id) if string_table.resolve(id) == "item"
        )),
        "header should capture default expression tokens without building an AST expression"
    );
}

#[test]
fn struct_field_default_stays_in_header_syntax_tokens() {
    let (headers, string_table) = parse_single_file_headers_with_table(
        "DEFAULT_WIDTH #= 80\nConfig = |\n    width Int = DEFAULT_WIDTH,\n|\n",
    );
    let struct_header = headers
        .headers
        .iter()
        .find(|header| matches!(header.kind, HeaderKind::Struct { .. }))
        .expect("expected struct header");

    let HeaderKind::Struct { fields, .. } = &struct_header.kind else {
        panic!("expected Struct header kind");
    };
    let field = fields.first().expect("expected width field shell");

    assert!(matches!(
        field.type_annotation,
        ParsedTypeRef::BuiltinInt { .. }
    ));
    assert!(
        field.default_tokens.iter().any(|token| matches!(
            token.kind,
            TokenKind::Symbol(id) if string_table.resolve(id) == "DEFAULT_WIDTH"
        )),
        "header should preserve struct default tokens for AST-time constant resolution"
    );
}

#[test]
fn function_signature_rejects_void_return_syntax() {
    let source = format!("f|| {}{}:\n;\n", "-> ", "Void");
    let result = parse_single_file_headers_with_entry(&source, "src/#page.bst", "src/#page.bst");
    assert!(result.is_err(), "void return syntax must be rejected");
    let errors = result.err().expect("expected parse errors");

    assert!(errors.diagnostics.iter().any(|diagnostic| matches!(
        diagnostic.payload,
        DiagnosticPayload::InvalidFunctionSignature {
            reason: InvalidFunctionSignatureReason::VoidNotAllowed
        }
    )));
}

#[test]
fn function_signature_rejects_none_return_syntax() {
    let source = format!("f|| {}{}:\n;\n", "-> ", "None");
    let result = parse_single_file_headers_with_entry(&source, "src/#page.bst", "src/#page.bst");
    assert!(result.is_err(), "none return syntax must be rejected");
    let errors = result.err().expect("expected parse errors");

    assert!(errors.diagnostics.iter().any(|diagnostic| matches!(
        diagnostic.payload,
        DiagnosticPayload::InvalidTypeAnnotation {
            reason: InvalidTypeAnnotationReason::NoneNotAllowed,
            ..
        }
    )));
}

#[test]
fn function_signature_preserves_unknown_symbolic_return_for_ast_resolution() {
    let headers = parse_single_file_headers("f|| -> MissingType:\n;\n");
    let signature = first_function_signature(&headers);

    assert!(matches!(
        signature.returns.as_slice(),
        [ReturnSlotSyntax {
            value: FunctionReturnSyntax::Value {
                type_annotation: ParsedTypeRef::Named { .. },
                ..
            },
            channel: ReturnChannelSyntax::Success,
            ..
        }]
    ));
}

#[test]
fn trait_declaration_headers_parse_requirement_shells() {
    let (headers, string_table) = parse_single_file_headers_with_table(
        "DISPLAYABLE must:\n\
             display |This| -> String\n\
             reset |~This|\n\
             copy_value |This, other This| -> This\n\
         ;\n",
    );

    let trait_header = headers
        .headers
        .iter()
        .find(|header| matches!(header.kind, HeaderKind::Trait { .. }))
        .expect("expected trait header");

    let HeaderKind::Trait { declaration } = &trait_header.kind else {
        panic!("expected trait header kind");
    };

    assert_eq!(string_table.resolve(declaration.name), "DISPLAYABLE");
    assert_eq!(declaration.requirements.len(), 3);
    assert_eq!(
        declaration.requirements[0].this_usage,
        TraitThisUsage::Immutable
    );
    assert_eq!(
        declaration.requirements[1].this_usage,
        TraitThisUsage::Mutable
    );

    let copy_requirement = &declaration.requirements[2];
    assert!(matches!(
        copy_requirement.signature.parameters[1].type_annotation,
        ParsedTypeRef::This { .. }
    ));
    assert!(matches!(
        copy_requirement.signature.returns[0].value,
        FunctionReturnSyntax::Value {
            type_annotation: ParsedTypeRef::This { .. },
            ..
        }
    ));
}

#[test]
fn empty_marker_trait_declaration_is_a_valid_header() {
    let headers = parse_single_file_headers("MARKER must:\n;\n");

    let trait_header = headers
        .headers
        .iter()
        .find(|header| matches!(header.kind, HeaderKind::Trait { .. }))
        .expect("expected trait header");

    let HeaderKind::Trait { declaration } = &trait_header.kind else {
        panic!("expected trait header kind");
    };

    assert!(
        declaration.requirements.is_empty(),
        "marker traits should parse with no requirement shells"
    );
}

#[test]
fn trait_conformance_headers_parse_single_and_continued_trait_lists() {
    let (headers, string_table) =
        parse_single_file_headers_with_table("Card must DISPLAYABLE,\n    SERIALIZABLE\n");

    let conformance_header = headers
        .headers
        .iter()
        .find(|header| matches!(header.kind, HeaderKind::TraitConformance { .. }))
        .expect("expected trait conformance header");

    let HeaderKind::TraitConformance { conformance } = &conformance_header.kind else {
        panic!("expected trait conformance header kind");
    };

    let trait_names = conformance
        .traits
        .iter()
        .map(|trait_ref| string_table.resolve(trait_ref.name).to_owned())
        .collect::<Vec<_>>();

    assert_eq!(string_table.resolve(conformance.target.name), "Card");
    assert_eq!(trait_names, vec!["DISPLAYABLE", "SERIALIZABLE"]);
}

#[test]
fn builtin_type_conformance_headers_parse_as_trait_conformances() {
    let (headers, string_table) = parse_single_file_headers_with_table("Int must DISPLAYABLE\n");

    let conformance_header = headers
        .headers
        .iter()
        .find(|header| matches!(header.kind, HeaderKind::TraitConformance { .. }))
        .expect("expected builtin trait conformance header");

    let HeaderKind::TraitConformance { conformance } = &conformance_header.kind else {
        panic!("expected trait conformance header kind");
    };

    assert_eq!(string_table.resolve(conformance.target.name), "Int");
    assert_eq!(
        string_table.resolve(conformance.traits[0].name),
        "DISPLAYABLE"
    );
}

#[test]
fn trait_requirement_rejects_lowercase_this_receiver() {
    let result = parse_single_file_headers_with_entry(
        "BAD must:\n    wrong |this|\n;\n",
        "src/#page.bst",
        "src/#page.bst",
    );
    let errors = expect_header_error(result, "lowercase this should be rejected");

    assert!(errors.diagnostics.iter().any(|diagnostic| matches!(
        diagnostic.payload,
        DiagnosticPayload::InvalidSignatureMember {
            reason: InvalidSignatureMemberReason::ThisNotAllowed
        }
    )));
}

#[test]
fn trait_requirement_rejects_missing_this_receiver() {
    let result = parse_single_file_headers_with_entry(
        "BAD must:\n    wrong |value Int|\n;\n",
        "src/#page.bst",
        "src/#page.bst",
    );
    let errors = expect_header_error(result, "trait requirements should start with This");

    assert!(errors.diagnostics.iter().any(|diagnostic| matches!(
        diagnostic.payload,
        DiagnosticPayload::InvalidSignatureMember {
            reason: InvalidSignatureMemberReason::TraitReceiverMustBeThis
        }
    )));
}

#[test]
fn trait_requirement_rejects_mutable_this_after_receiver() {
    let result = parse_single_file_headers_with_entry(
        "BAD must:\n    wrong |This, ~This|\n;\n",
        "src/#page.bst",
        "src/#page.bst",
    );
    let errors = expect_header_error(result, "mutable This is receiver-only");

    assert!(errors.diagnostics.iter().any(|diagnostic| matches!(
        diagnostic.payload,
        DiagnosticPayload::InvalidSignatureMember {
            reason: InvalidSignatureMemberReason::TraitMutableThisOnlyFirstParameter
        }
    )));
}

#[test]
fn trait_requirement_rejects_composed_this_type_forms() {
    let result = parse_single_file_headers_with_entry(
        "BAD must:\n    wrong |This, values {This}|\n;\n",
        "src/#page.bst",
        "src/#page.bst",
    );
    let errors = expect_header_error(result, "composed This forms are deferred");

    assert!(errors.diagnostics.iter().any(|diagnostic| matches!(
        diagnostic.payload,
        DiagnosticPayload::InvalidTypeAnnotation {
            reason: InvalidTypeAnnotationReason::TraitThisMustBeDirect,
            ..
        }
    )));
}

#[test]
fn trait_requirement_rejects_method_bodies_and_reversed_mutability() {
    let method_body_result = parse_single_file_headers_with_entry(
        "BAD must:\n    wrong |This|:\n        return \"bad\"\n    ;\n;\n",
        "src/#page.bst",
        "src/#page.bst",
    );
    let method_body_errors = expect_header_error(
        method_body_result,
        "trait requirements cannot have method bodies",
    );

    assert!(
        method_body_errors
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.kind
                == DiagnosticKind::Syntax(SyntaxDiagnosticKind::UnexpectedTokenInDeclaration))
    );

    let reversed_mutability_result = parse_single_file_headers_with_entry(
        "BAD must:\n    wrong |This ~|\n;\n",
        "src/#page.bst",
        "src/#page.bst",
    );
    let reversed_mutability_errors = expect_header_error(
        reversed_mutability_result,
        "trait receiver mutability must be written as ~This",
    );

    assert!(
        reversed_mutability_errors
            .diagnostics
            .iter()
            .any(|diagnostic| matches!(
                diagnostic.payload,
                DiagnosticPayload::ExpectedToken { .. }
            ))
    );
}

#[test]
fn trait_conformance_rejects_missing_trait_name() {
    let result =
        parse_single_file_headers_with_entry("Card must\n", "src/#page.bst", "src/#page.bst");
    let errors = expect_header_error(result, "conformance declarations require a trait name");

    assert!(errors.diagnostics.iter().any(|diagnostic| matches!(
        diagnostic.payload,
        DiagnosticPayload::InvalidDeclaration {
            reason: InvalidDeclarationReason::TraitConformanceMissingTrait,
            ..
        }
    )));
}

#[test]
fn trait_conformance_rejects_semicolon_terminator() {
    let result = parse_single_file_headers_with_entry(
        "Card must DISPLAYABLE;\n",
        "src/#page.bst",
        "src/#page.bst",
    );
    let errors = expect_header_error(
        result,
        "conformance declarations should be newline terminated",
    );

    assert!(errors.diagnostics.iter().any(|diagnostic| matches!(
        diagnostic.payload,
        DiagnosticPayload::InvalidDeclaration {
            reason: InvalidDeclarationReason::TraitConformanceSemicolon,
            ..
        }
    )));
}

#[test]
fn trait_conformance_rejects_trailing_comma() {
    let result = parse_single_file_headers_with_entry(
        "Card must DISPLAYABLE,\n",
        "src/#page.bst",
        "src/#page.bst",
    );
    let errors = expect_header_error(result, "trailing conformance commas should be rejected");

    assert!(errors.diagnostics.iter().any(|diagnostic| matches!(
        diagnostic.payload,
        DiagnosticPayload::UnexpectedTrailingComma
    )));
}

#[test]
fn trait_declaration_and_reference_names_must_be_all_caps() {
    let declaration_result = parse_single_file_headers_with_entry(
        "Displayable must:\n;\n",
        "src/#page.bst",
        "src/#page.bst",
    );
    let declaration_errors = expect_header_error(
        declaration_result,
        "trait declarations should require all-caps names",
    );

    assert!(
        declaration_errors
            .diagnostics
            .iter()
            .any(|diagnostic| matches!(
                diagnostic.payload,
                DiagnosticPayload::InvalidDeclaration {
                    reason: InvalidDeclarationReason::InvalidTraitName,
                    ..
                }
            ))
    );

    let conformance_result = parse_single_file_headers_with_entry(
        "Card must Displayable\n",
        "src/#page.bst",
        "src/#page.bst",
    );
    let conformance_errors = expect_header_error(
        conformance_result,
        "trait references should require all-caps names",
    );

    assert!(
        conformance_errors
            .diagnostics
            .iter()
            .any(|diagnostic| matches!(
                diagnostic.payload,
                DiagnosticPayload::InvalidDeclaration {
                    reason: InvalidDeclarationReason::InvalidTraitName,
                    ..
                }
            ))
    );
}

#[test]
fn trait_this_outside_trait_declaration_is_targeted() {
    let result =
        parse_single_file_headers_with_entry("value This = 1\n", "src/#page.bst", "src/#page.bst");
    let errors = expect_header_error(result, "This outside trait declarations should be rejected");

    assert!(errors.diagnostics.iter().any(|diagnostic| matches!(
        diagnostic.payload,
        DiagnosticPayload::InvalidThisUsage {
            reason: InvalidThisUsageReason::OutsideTraitDeclaration
        }
    )));
}

#[test]
fn function_signature_reports_missing_arrow_before_return_type() {
    let result = parse_single_file_headers_with_entry(
        "f|x Int| Int:\n;\n",
        "src/#page.bst",
        "src/#page.bst",
    );
    assert!(
        result.is_err(),
        "missing arrow before return type must fail"
    );
    let errors = result.err().expect("expected parse errors");

    assert!(errors.diagnostics.iter().any(|diagnostic| matches!(
        diagnostic.payload,
        DiagnosticPayload::InvalidFunctionSignature {
            reason: InvalidFunctionSignatureReason::MissingArrowOrColon { .. }
        }
    )));
}

#[test]
fn function_signature_reports_missing_colon_after_return_list() {
    let result =
        parse_single_file_headers_with_entry("f|| -> Int\n;\n", "src/#page.bst", "src/#page.bst");
    assert!(
        result.is_err(),
        "missing ':' after return declarations must fail"
    );
    let errors = result.err().expect("expected parse errors");

    assert!(errors.diagnostics.iter().any(|diagnostic| matches!(
        diagnostic.payload,
        DiagnosticPayload::InvalidFunctionSignature {
            reason: InvalidFunctionSignatureReason::MissingColonAfterReturns
        }
    )));
}

#[test]
fn duplicate_top_level_function_names_error_during_header_parsing() {
    let result = parse_single_file_headers_with_entry(
        "simple_function |number Int| -> Int:\n\
             return number + 1\n\
         ;\n\
         \n\
         simple_function |value Int| -> Int:\n\
             return value + 2\n\
         ;\n",
        "src/#page.bst",
        "src/#page.bst",
    );

    assert!(
        result.is_err(),
        "duplicate top-level function names should fail during header parsing"
    );
    let errors = result.err().expect("expected parse errors");

    assert!(errors.diagnostics.iter().any(|diagnostic| {
        matches!(
            diagnostic.payload,
            DiagnosticPayload::DuplicateDeclaration { .. }
        )
    }));
}

#[test]
fn duplicate_header_detection_ignores_qualified_match_arms() {
    let mut string_table = StringTable::new();
    let source_file = InternedPath::from_single_str("src/#page.bst", &mut string_table);
    let status = string_table.intern("Status");
    let ready = string_table.intern("Ready");
    let write = string_table.intern("write");
    let location = SourceLocation::default();

    let mut token_stream = FileTokens::new(
        source_file,
        vec![
            Token::new(TokenKind::Symbol(status), location.clone()),
            Token::new(TokenKind::DoubleColon, location.clone()),
            Token::new(TokenKind::Symbol(ready), location.clone()),
            Token::new(TokenKind::FatArrow, location.clone()),
            Token::new(TokenKind::Symbol(write), location.clone()),
            Token::new(TokenKind::Eof, location),
        ],
    );
    token_stream.index = 1;

    assert!(
        !super::super::top_level_classifier::starts_duplicate_top_level_header_declaration(
            &token_stream
        ),
        "qualified match arms in the start body are not choice declarations"
    );
}

#[test]
fn choice_headers_parse_unit_variants_in_declaration_order() {
    let (headers, string_table) =
        parse_single_file_headers_with_table("Status :: Ready,\nBusy,\nIdle,\n;\n");
    let choice_header = headers
        .headers
        .iter()
        .find(|header| matches!(header.kind, HeaderKind::Choice { .. }))
        .expect("expected choice header");

    let HeaderKind::Choice { variants, .. } = &choice_header.kind else {
        panic!("expected choice metadata");
    };

    assert_eq!(variants.len(), 3, "expected three parsed variants");
    assert_eq!(string_table.resolve(variants[0].id), "Ready");
    assert_eq!(string_table.resolve(variants[1].id), "Busy");
    assert_eq!(string_table.resolve(variants[2].id), "Idle");
}

#[test]
fn choice_headers_reject_duplicate_variants() {
    let result = parse_single_file_headers_with_entry(
        "Status :: Ready, Ready;\n",
        "src/#page.bst",
        "src/#page.bst",
    );
    assert!(result.is_err(), "duplicate choice variants must fail");
    let errors = result.err().expect("expected parse errors");

    assert!(errors.diagnostics.iter().any(|diagnostic| {
        matches!(
            diagnostic.payload,
            DiagnosticPayload::DuplicateDeclaration { .. }
        )
    }));
}

#[test]
fn choice_headers_reject_invalid_payload_forms() {
    // Shorthand payload is invalid by design (not deferred).
    let payload_shorthand_result = parse_single_file_headers_with_entry(
        "Status :: Ready String;\n",
        "src/#page.bst",
        "src/#page.bst",
    );
    assert!(
        payload_shorthand_result.is_err(),
        "shorthand payload variants must be rejected"
    );
    let payload_errors = payload_shorthand_result
        .err()
        .expect("expected payload parse errors");
    assert!(payload_errors.diagnostics.iter().any(|diagnostic| matches!(
        diagnostic.payload,
        DiagnosticPayload::InvalidChoiceVariant {
            reason: InvalidChoiceVariantReason::PayloadShorthandNotSupported,
            ..
        }
    )));

    // Constructor-style declarations are invalid by design.
    let payload_paren_result = parse_single_file_headers_with_entry(
        "Status :: Ready(String);\n",
        "src/#page.bst",
        "src/#page.bst",
    );
    assert!(
        payload_paren_result.is_err(),
        "constructor-style payload variants must be rejected"
    );
    let payload_paren_errors = payload_paren_result
        .err()
        .expect("expected constructor-style payload parse errors");
    assert!(
        payload_paren_errors
            .diagnostics
            .iter()
            .any(|diagnostic| matches!(
                diagnostic.payload,
                DiagnosticPayload::InvalidChoiceVariant {
                    reason: InvalidChoiceVariantReason::ConstructorStyleNotSupported,
                    ..
                }
            ))
    );

    // Default values remain deferred.
    let defaults_result = parse_single_file_headers_with_entry(
        "Status :: Ready = true;\n",
        "src/#page.bst",
        "src/#page.bst",
    );
    assert!(
        defaults_result.is_err(),
        "choice variant defaults must fail"
    );
    let default_errors = defaults_result
        .err()
        .expect("expected default parse errors");
    assert!(default_errors.diagnostics.iter().any(|diagnostic| matches!(
        diagnostic.payload,
        DiagnosticPayload::DeferredFeature {
            reason: DeferredFeatureReason::ChoiceVariantDefaultValue
        }
    )));
}

#[test]
fn choice_headers_accept_record_payload_variants() {
    let (headers, string_table) =
        parse_single_file_headers_with_table("Status :: Pending |\n    RetryCount Int,\n|;\n");

    let choice_header = headers
        .headers
        .iter()
        .find(|header| matches!(header.kind, HeaderKind::Choice { .. }))
        .expect("expected choice header");

    let HeaderKind::Choice { variants, .. } = &choice_header.kind else {
        panic!("expected choice metadata");
    };

    assert_eq!(variants.len(), 1, "expected one parsed variant");
    assert_eq!(
        string_table.resolve(variants[0].id),
        "Pending",
        "expected Pending variant"
    );
    match &variants[0].payload {
        ChoiceVariantPayloadSyntax::Record { fields } => {
            assert_eq!(fields.len(), 1, "expected one payload field");
            assert_eq!(
                fields[0].id.name_str(&string_table),
                Some("RetryCount"),
                "expected RetryCount field"
            );
        }
        other => panic!("expected Record payload, got {other:?}"),
    }
}

#[test]
fn header_parsing_emits_naming_warnings_for_non_camel_type_like_symbols() {
    let (headers, warnings) = parse_single_file_headers_with_warnings(
        "SITE_TITLE #= \"Beanstalk\"\nStatus_type :: bad_variant;\n",
    );

    assert!(
        headers
            .headers
            .iter()
            .any(|header| matches!(header.kind, HeaderKind::Choice { .. })),
        "fixture should still parse a choice header"
    );
    assert_eq!(
        warnings.len(),
        2,
        "expected warnings for choice name and variant only; uppercase constant should be allowed"
    );
    assert!(
        warnings
            .iter()
            .all(|warning| matches!(
                warning.kind,
                crate::compiler_frontend::compiler_messages::DiagnosticKind::Rule(
                    crate::compiler_frontend::compiler_messages::RuleDiagnosticKind::IdentifierNamingConvention
                )
            )),
        "expected naming convention warnings for choice name and variant only"
    );
}

#[test]
fn header_parsing_rejects_keyword_shadow_constant_name() {
    let result =
        parse_single_file_headers_with_entry("FALSE #= 1\n", "src/#page.bst", "src/#page.bst");
    assert!(
        result.is_err(),
        "keyword-shadow top-level constants must fail during header parsing"
    );
    let errors = result.err().expect("expected parse errors");

    assert!(errors.diagnostics.iter().any(|diagnostic| matches!(
        diagnostic.payload,
        DiagnosticPayload::ReservedNameCollision {
            reserved_by: ReservedNameOwner::Keyword,
            ..
        }
    )));
}

#[test]
fn trait_declarations_using_must_parse_as_trait_headers() {
    let headers = parse_single_file_headers("DISPLAYABLE must:\n    display |This| -> String\n;\n");

    assert!(
        headers
            .headers
            .iter()
            .any(|header| matches!(header.kind, HeaderKind::Trait { .. })),
        "trait declarations using 'must:' should produce trait headers"
    );
}

#[test]
fn generic_type_aliases_are_rejected_during_header_parsing() {
    let result = parse_single_file_headers_with_entry(
        "Response type T as ResultShape of T, Error\n",
        "src/#page.bst",
        "src/#page.bst",
    );

    assert!(
        result.is_err(),
        "generic type aliases should fail during header parsing"
    );
    let errors = result.err().expect("expected parse errors");

    assert!(errors.diagnostics.iter().any(|diagnostic| {
        diagnostic.kind == DiagnosticKind::Rule(RuleDiagnosticKind::InvalidDeclaration)
            && matches!(
                diagnostic.payload,
                DiagnosticPayload::InvalidDeclaration {
                    reason: InvalidDeclarationReason::ParameterizedGenericTypeAlias,
                    ..
                }
            )
    }));
}

#[test]
fn entry_runtime_fragment_count_is_zero_with_no_templates() {
    let headers = parse_single_file_headers("x = 1\n");
    assert_eq!(
        headers.entry_runtime_fragment_count, 0,
        "no top-level templates should yield runtime fragment count of 0"
    );
}

#[test]
fn entry_runtime_fragment_count_is_zero_for_const_only_templates() {
    // #[...] is a const (exported) template — it does not contribute to the runtime count.
    let headers = parse_single_file_headers("#[3]\n");
    assert_eq!(
        headers.entry_runtime_fragment_count, 0,
        "const templates should not increment the runtime fragment count"
    );
}

#[test]
fn entry_runtime_fragment_count_reflects_runtime_template_count() {
    // [3] is a runtime template (no # prefix); one at top level should yield count 1.
    let headers = parse_single_file_headers("[3]\n");
    assert_eq!(
        headers.entry_runtime_fragment_count, 1,
        "one runtime top-level template should yield runtime fragment count of 1"
    );
}

#[test]
fn entry_runtime_fragment_count_accumulates_across_multiple_runtime_templates() {
    let headers = parse_single_file_headers("[1]\n[2]\n[3]\n");
    assert_eq!(
        headers.entry_runtime_fragment_count, 3,
        "three runtime top-level templates should yield runtime fragment count of 3"
    );
}

#[test]
fn entry_runtime_fragment_count_is_zero_when_parsed_as_non_entry_file() {
    // A library file (non-entry) with only declarations reports runtime fragment count 0.
    // WHY: only `FileRole::Entry` increments runtime_fragment_count.
    let headers = parse_single_file_headers_with_entry(
        "f || -> Int:\n    1\n;\n",
        "src/lib.bst",
        "src/#page.bst",
    )
    .expect("headers should parse");
    assert_eq!(
        headers.entry_runtime_fragment_count, 0,
        "entry_runtime_fragment_count must be 0 when the file is not the entry file"
    );
}

#[test]
fn typed_constant_creates_header_provided_dependency_on_declared_type() {
    // WHY: the declared type creates a structural ordering constraint so that the type
    // is sorted before any constant that references it. Initializer-expression references
    // do NOT create header-provided deps — only the declared type annotation does.
    let (headers, string_table) =
        parse_single_file_headers_with_table("struct NavBar {}\ntheme #NavBar = default_navbar\n");

    let constant_header = headers
        .headers
        .iter()
        .find(|header| matches!(header.kind, HeaderKind::Constant { .. }))
        .expect("expected constant header");

    assert!(
        !constant_header.dependencies.is_empty(),
        "typed constant must create a header-provided dependency on its declared type"
    );
    assert!(
        constant_header
            .dependencies
            .iter()
            .any(|dep| dep.name_str(&string_table) == Some("NavBar")),
        "header-provided dependency must reference the declared type name 'NavBar'"
    );
}

#[test]
fn struct_fields_create_header_provided_dependencies_on_named_field_types() {
    // WHY: struct fields whose types are user-defined names create header-provided sort edges so that
    // the named type is always sorted before the struct that depends on it.
    let (headers, string_table) = parse_single_file_headers_with_table(
        "Point = |x Int, y Int|\nSpan = |start Point, end Point|\n",
    );

    let span_header = headers
        .headers
        .iter()
        .find(|header| {
            matches!(header.kind, HeaderKind::Struct { .. })
                && header.tokens.src_path.name_str(&string_table) == Some("Span")
        })
        .expect("expected Span struct header");

    assert!(
        span_header
            .dependencies
            .iter()
            .any(|dep| dep.name_str(&string_table) == Some("Point")),
        "Span must carry a header-provided dependency on Point via its field type annotations"
    );
}

#[test]
fn function_error_return_creates_header_provided_dependency_on_named_type() {
    // WHY: final `T!` error slots are part of the declaration surface. Their named types must
    // participate in header dependency sorting before AST resolves function signatures.
    let (headers, string_table) = parse_single_file_headers_with_table(
        "AppError = |message String|\nparse || -> Int, AppError!:\n    return 1\n;\n",
    );

    let parse_header = headers
        .headers
        .iter()
        .find(|header| {
            matches!(header.kind, HeaderKind::Function { .. })
                && header.tokens.src_path.name_str(&string_table) == Some("parse")
        })
        .expect("expected parse function header");

    assert!(
        parse_header
            .dependencies
            .iter()
            .any(|dep| dep.name_str(&string_table) == Some("AppError")),
        "function error return slot must carry a header-provided dependency on AppError"
    );
}

#[test]
fn constant_header_with_declared_type_captures_type_in_declaration() {
    // Confirms the header-stage contract: declared type annotation is present in the
    // Constant header's declaration, proving initializer resolution is deferred to AST.
    let headers = parse_single_file_headers("threshold #Int = 42\n");

    let constant_header = headers
        .headers
        .iter()
        .find(|header| matches!(header.kind, HeaderKind::Constant { .. }))
        .expect("expected constant header");

    let HeaderKind::Constant { declaration, .. } = &constant_header.kind else {
        panic!("expected Constant header kind");
    };

    assert!(
        !matches!(declaration.type_annotation, ParsedTypeRef::Inferred),
        "declared type annotation on a typed constant must be resolved at the header stage, not left as Inferred"
    );
}

/// Verifies that `parse_headers` correctly aggregates per-file outputs from multiple source files.
///
/// WHAT: entry file contributes runtime templates, const templates, and a start function;
///       a non-entry library file contributes declarations; a facade file contributes exports.
/// WHY: this is the primary observable boundary introduced by the per-file refactor.
fn parse_multi_file_headers(sources: &[(String, String)], entry_path: &str) -> Headers {
    let mut string_table = StringTable::new();
    let entry_file_path = PathBuf::from(entry_path);
    let external_package_registry = ExternalPackageRegistry::new();
    let options = HeaderParseOptions::default();
    let style_directives = StyleDirectiveRegistry::built_ins();
    let prepare_context = HeaderTestPrepareContext {
        entry_file_path: &entry_file_path,
        options: &options,
        style_directives: &style_directives,
        external_package_registry: &external_package_registry,
    };

    let mut prepared_outputs = Vec::new();
    let mut const_template_offset = 0usize;
    let mut runtime_fragment_offset = 0usize;

    for (source, path_str) in sources {
        let file_path = PathBuf::from(path_str);
        let output = prepare_test_source_file(
            source,
            &file_path,
            &prepare_context,
            &mut string_table,
            const_template_offset,
            runtime_fragment_offset,
        )
        .expect("preparation should succeed");

        const_template_offset += output.const_template_count;
        runtime_fragment_offset += output.runtime_fragment_count;
        prepared_outputs.push(output);
    }

    parse_headers(
        prepared_outputs,
        &external_package_registry,
        &ExternalImportResolutionTable::default(),
        options.project_path_resolver.as_ref(),
        &mut string_table,
    )
    .expect("headers should parse")
}

#[test]
fn multi_file_parsing_aggregates_headers_const_fragments_and_runtime_count() {
    let sources = vec![
        (
            "[runtime1]\n#[const1]\n[runtime2]\n".to_owned(),
            "src/#page.bst".to_owned(),
        ),
        (
            "helper_func || -> Int:\n    return 1\n;\n".to_owned(),
            "src/helper.bst".to_owned(),
        ),
    ];

    let headers = parse_multi_file_headers(&sources, "src/#page.bst");

    // Entry file: 2 runtime templates + 1 const template + 1 start function = 2 headers
    // (const template + start function; runtime templates are inside start function)
    // Non-entry file: 1 function header
    assert!(
        headers.headers.len() >= 2,
        "expected headers from both files to be aggregated"
    );

    // Verify const fragment from entry file is preserved.
    assert_eq!(
        headers.top_level_const_fragments.len(),
        1,
        "expected one const fragment from entry file"
    );
    assert_eq!(
        headers.top_level_const_fragments[0].runtime_insertion_index, 1,
        "const fragment should be inserted after 1 runtime fragment (the one before it)"
    );

    // Verify runtime fragment count is correct for entry file.
    assert_eq!(
        headers.entry_runtime_fragment_count, 2,
        "expected 2 runtime fragments from entry file"
    );
}

/// Parse multiple files and return the full result together with collected warnings and the
/// string table so tests can inspect both success and failure paths.
fn parse_multi_file_headers_with_result(
    sources: &[(String, String)],
    entry_path: &str,
) -> (
    Result<Headers, DiagnosticBag>,
    Vec<CompilerDiagnostic>,
    StringTable,
) {
    let mut string_table = StringTable::new();
    let entry_file_path = PathBuf::from(entry_path);
    let external_package_registry = ExternalPackageRegistry::new();
    let options = HeaderParseOptions::default();
    let style_directives = StyleDirectiveRegistry::built_ins();
    let prepare_context = HeaderTestPrepareContext {
        entry_file_path: &entry_file_path,
        options: &options,
        style_directives: &style_directives,
        external_package_registry: &external_package_registry,
    };

    let mut prepared_outputs = Vec::new();
    let mut warnings = Vec::new();
    let mut diagnostic_bag = DiagnosticBag::new();
    let mut const_template_offset = 0usize;
    let mut runtime_fragment_offset = 0usize;

    for (source, path_str) in sources {
        let file_path = PathBuf::from(path_str);
        match prepare_test_source_file(
            source,
            &file_path,
            &prepare_context,
            &mut string_table,
            const_template_offset,
            runtime_fragment_offset,
        ) {
            Ok(output) => {
                const_template_offset += output.const_template_count;
                runtime_fragment_offset += output.runtime_fragment_count;
                warnings.extend(output.warnings.clone());
                prepared_outputs.push(output);
            }
            Err(error) => {
                warnings.extend(error.warnings);
                diagnostic_bag.push(*error.diagnostic);
            }
        }
    }

    if diagnostic_bag.has_errors() {
        return (Err(diagnostic_bag), warnings, string_table);
    }

    let result = parse_headers(
        prepared_outputs,
        &external_package_registry,
        &ExternalImportResolutionTable::default(),
        options.project_path_resolver.as_ref(),
        &mut string_table,
    );

    (result, warnings, string_table)
}

#[test]
fn multi_file_parsing_aggregates_warnings_from_all_files() {
    let sources = vec![
        (
            "Status_type :: bad_variant;\n".to_owned(),
            "src/#page.bst".to_owned(),
        ),
        (
            "Helper_type :: other_variant;\n".to_owned(),
            "src/helper.bst".to_owned(),
        ),
    ];

    let (result, warnings, _string_table) =
        parse_multi_file_headers_with_result(&sources, "src/#page.bst");

    assert!(result.is_ok(), "expected successful header parsing");
    assert_eq!(
        warnings.len(),
        4,
        "expected four naming-convention warnings (two from each file)"
    );
    assert!(
        warnings.iter().all(|warning| matches!(
            warning.kind,
            DiagnosticKind::Rule(
                crate::compiler_frontend::compiler_messages::RuleDiagnosticKind::IdentifierNamingConvention
            )
        )),
        "all warnings should be naming convention warnings"
    );
}

#[test]
fn multi_file_parsing_preserves_warnings_before_later_parse_error() {
    // The helper file emits naming warnings, then fails on a later duplicate declaration.
    // Those file-local warnings must still be merged even though the file contributes no output.
    let sources = vec![
        ("io(\"hello\")\n".to_owned(), "src/#page.bst".to_owned()),
        (
            "Status_type :: bad_variant;\ndup ||:\n;\ndup ||:\n;\n".to_owned(),
            "src/helper.bst".to_owned(),
        ),
    ];

    let (result, warnings, _string_table) =
        parse_multi_file_headers_with_result(&sources, "src/#page.bst");

    assert!(
        result.is_err(),
        "expected header parsing to fail due to duplicate declaration"
    );

    assert_eq!(
        warnings.len(),
        2,
        "expected two naming-convention warnings from the failing helper file to be preserved"
    );
    assert!(
        warnings.iter().all(|warning| matches!(
            warning.kind,
            DiagnosticKind::Rule(
                crate::compiler_frontend::compiler_messages::RuleDiagnosticKind::IdentifierNamingConvention
            )
        )),
        "all warnings should be naming convention warnings"
    );
}

#[test]
fn per_file_fork_merge_produces_correct_headers_and_warnings_for_multiple_files() {
    let sources = [
        (
            "FooA #= \"a\"\nBarA #= \"b\"\n".to_owned(),
            "src/#page.bst".to_owned(),
        ),
        (
            "FooB #= \"c\"\nBarB #= \"d\"\n".to_owned(),
            "src/helper.bst".to_owned(),
        ),
    ];

    let (result, warnings, string_table) =
        parse_multi_file_headers_with_result(&sources, "src/#page.bst");

    let headers = result.expect("headers should parse");

    // 4 constant headers + 1 start header = 5 headers
    assert_eq!(headers.headers.len(), 5, "expected 4 constants + 1 start");

    let constant_names: Vec<String> = headers
        .headers
        .iter()
        .filter_map(|header| match &header.kind {
            HeaderKind::Constant { .. } => header
                .tokens
                .src_path
                .name()
                .map(|n| string_table.resolve(n).to_owned()),
            _ => None,
        })
        .collect();

    assert!(constant_names.contains(&"FooA".to_owned()));
    assert!(constant_names.contains(&"BarA".to_owned()));
    assert!(constant_names.contains(&"FooB".to_owned()));
    assert!(constant_names.contains(&"BarB".to_owned()));

    // PascalCase top-level constant names should produce naming warnings.
    assert_eq!(
        warnings.len(),
        4,
        "expected four naming convention warnings for PascalCase constants"
    );
    assert!(
        warnings.iter().all(|warning| matches!(
            warning.kind,
            DiagnosticKind::Rule(
                crate::compiler_frontend::compiler_messages::RuleDiagnosticKind::IdentifierNamingConvention
            )
        )),
        "all warnings should be naming convention warnings"
    );
}

#[test]
fn per_file_fork_merge_remaps_non_identity_strings_across_multiple_files() {
    // Both files intern generated deferred-feature strings into their local suffixes.
    // Because the fork source is shared and frozen before the loop, the second merge must remap
    // that local ID past the first file's generated string in the module table.
    let sources = [
        (
            "Foo #= \"a\"\n#[facade_fragment]\n".to_owned(),
            "src/#mod.bst".to_owned(),
        ),
        (
            "Bar #= \"b\"\n#[const_fragment]\n".to_owned(),
            "src/helper.bst".to_owned(),
        ),
    ];

    let (result, warnings, string_table) =
        parse_multi_file_headers_with_result(&sources, "src/#page.bst");

    assert!(
        result.is_err(),
        "expected header parsing to fail due to deferred header features"
    );

    // PascalCase constants produce naming warnings before the errors.
    assert_eq!(
        warnings.len(),
        2,
        "expected two naming convention warnings before errors"
    );

    let errors = result.err().expect("expected errors").into_diagnostics();
    assert_eq!(errors.len(), 2, "expected two deferred feature errors");

    let mut feature_names = Vec::new();
    for error in &errors {
        let DiagnosticPayload::DeferredFeature { reason } = &error.payload else {
            panic!("expected DeferredFeature payload, got {:?}", error.payload);
        };
        match reason {
            DeferredFeatureReason::NamedFeature { feature } => {
                feature_names.push(string_table.resolve(*feature).to_owned());
            }
            other => panic!("expected NamedFeature reason, got {:?}", other),
        }
    }

    assert!(
        feature_names.contains(&"top-level const templates in module facades".to_owned()),
        "first file's generated feature name must resolve correctly"
    );
    assert!(
        feature_names.contains(&"top-level const templates in non-entry files".to_owned()),
        "second file's generated feature name must resolve correctly after non-identity remap"
    );
}

#[test]
fn module_facade_rejects_top_level_const_template() {
    let source = "#[hello]\n";
    let file_path = PathBuf::from("src/#mod.bst");
    let entry_file_path = PathBuf::from("src/#page.bst");
    let result = parse_single_file_headers_with_entry(
        source,
        &file_path.to_string_lossy(),
        &entry_file_path.to_string_lossy(),
    );

    let diagnostics = match result {
        Ok(_) => panic!("expected header parsing to fail for const template in module facade"),
        Err(err) => err.diagnostics,
    };

    let diag = diagnostics
        .first()
        .expect("expected at least one diagnostic");
    assert!(
        matches!(
            diag.kind,
            DiagnosticKind::DeferredFeature(DeferredFeatureDiagnosticKind::DeferredFeature)
        ),
        "expected deferred feature diagnostic, got {:?}",
        diag.kind
    );
}

#[test]
fn import_only_file_contributes_file_imports_and_module_file_paths() {
    use crate::compiler_frontend::headers::symbol_collection::build_module_symbols;

    let mut string_table = StringTable::new();
    let file_path = PathBuf::from("src/helper.bst");
    let entry_file_path = PathBuf::from("src/#page.bst");
    let helper_output = prepare_single_file(
        "import @core/math\n",
        &file_path,
        &entry_file_path,
        &mut string_table,
    );

    let page_output = prepare_single_file(
        "value #= 1\n",
        &PathBuf::from("src/#page.bst"),
        &entry_file_path,
        &mut string_table,
    );

    let module_symbols = build_module_symbols(&[helper_output, page_output], &mut string_table)
        .expect("module symbols should build");

    let helper_path =
        InternedPath::from_path_buf(&PathBuf::from("src/helper.bst"), &mut string_table);
    let page_path = InternedPath::from_path_buf(&PathBuf::from("src/#page.bst"), &mut string_table);

    assert!(
        module_symbols.module_file_paths.contains(&helper_path),
        "import-only files must contribute to module_file_paths"
    );
    assert!(
        module_symbols.module_file_paths.contains(&page_path),
        "entry files must contribute to module_file_paths"
    );

    let helper_imports = module_symbols
        .file_imports_by_source
        .get(&helper_path)
        .expect("import-only file imports must be registered");

    assert_eq!(helper_imports.len(), 1);
    assert_eq!(
        helper_imports[0]
            .header_path
            .to_portable_string(&string_table),
        "core/math"
    );
    assert_eq!(
        helper_imports[0].export_mode,
        crate::compiler_frontend::headers::types::HeaderExportMode::Private
    );
}

#[test]
fn per_file_prepare_output_preserves_file_role_and_imports_on_output() {
    let mut string_table = StringTable::new();
    let file_path = PathBuf::from("src/helper.bst");
    let entry_file_path = PathBuf::from("src/#page.bst");
    let output = prepare_single_file(
        "import @core/math\n",
        &file_path,
        &entry_file_path,
        &mut string_table,
    );

    assert_eq!(output.file_role, FileRole::Normal);
    assert_eq!(output.file_imports.len(), 1);
    assert_eq!(
        output.file_imports[0]
            .header_path
            .to_portable_string(&string_table),
        "core/math"
    );
}

#[test]
fn module_facade_prepare_output_has_module_facade_role() {
    let mut string_table = StringTable::new();
    let file_path = PathBuf::from("src/#mod.bst");
    let entry_file_path = PathBuf::from("src/#page.bst");
    let output = prepare_single_file(
        "Button = | label String |\n",
        &file_path,
        &entry_file_path,
        &mut string_table,
    );

    assert_eq!(output.file_role, FileRole::ModuleFacade);
    assert!(output.file_imports.is_empty());
}

// ------------------------------
//  Export keyword parsing tests
// ------------------------------

#[test]
fn export_outside_module_facade_is_rejected() {
    let result = parse_single_file_headers_with_entry(
        "export Button = | label String |\n",
        "src/helper.bst",
        "src/#page.bst",
    );
    let errors = expect_header_error(result, "export outside #mod.bst should be rejected");

    assert!(errors.diagnostics.iter().any(|diagnostic| diagnostic.kind
        == DiagnosticKind::Rule(RuleDiagnosticKind::ExportOutsideModuleFacade)));
}

#[test]
fn export_alone_is_rejected() {
    let result = parse_single_file_headers_with_entry("export\n", "src/#mod.bst", "src/#page.bst");
    let errors = expect_header_error(result, "export alone should be rejected");

    assert!(errors.diagnostics.iter().any(|diagnostic| diagnostic.kind
        == DiagnosticKind::Rule(RuleDiagnosticKind::MissingExportTarget)));
}

#[test]
fn export_import_path_parsed_as_public_import() {
    let mut string_table = StringTable::new();
    let output = prepare_single_file(
        "export import @./button { Button }\n",
        &PathBuf::from("src/#mod.bst"),
        &PathBuf::from("src/#page.bst"),
        &mut string_table,
    );

    assert_eq!(output.file_imports.len(), 1);
    assert_eq!(output.file_imports[0].export_mode, HeaderExportMode::Public);
    assert_eq!(
        output.file_imports[0]
            .header_path
            .to_portable_string(&string_table),
        "src/button/Button"
    );
}

#[test]
fn export_path_sugar_parsed_as_public_import() {
    let mut string_table = StringTable::new();
    let output = prepare_single_file(
        "export @./card { Card, render as render_card }\n",
        &PathBuf::from("src/#mod.bst"),
        &PathBuf::from("src/#page.bst"),
        &mut string_table,
    );

    assert_eq!(output.file_imports.len(), 2);
    assert!(
        output
            .file_imports
            .iter()
            .all(|import| import.export_mode == HeaderExportMode::Public)
    );
}

#[test]
fn export_bare_path_rejected_as_deferred_namespace_export() {
    let result =
        parse_single_file_headers_with_entry("export @./layout\n", "src/#mod.bst", "src/#page.bst");
    let errors = expect_header_error(
        result,
        "bare namespace export should be rejected as deferred",
    );

    assert!(errors.diagnostics.iter().any(|diagnostic| diagnostic.kind
        == DiagnosticKind::Rule(RuleDiagnosticKind::DeferredNamespaceExport)));
}

#[test]
fn export_before_authored_declaration_marks_header_public() {
    let source = "export Button = | label String |\nexport render |button Button| -> String:\n    return button.label\n;\n";
    let headers = parse_single_file_headers_with_entry(source, "src/#mod.bst", "src/#page.bst")
        .expect("headers should parse");

    let public_headers: Vec<_> = headers
        .headers
        .iter()
        .filter(|header| header.export_mode == HeaderExportMode::Public)
        .collect();

    assert_eq!(
        public_headers.len(),
        2,
        "expected two public headers: struct and function"
    );
}

#[test]
fn unmarked_authored_declarations_in_mod_facade_remain_private() {
    let source = "Button = | label String |\nrender |button Button| -> String:\n    return button.label\n;\n";
    let headers = parse_single_file_headers_with_entry(source, "src/#mod.bst", "src/#page.bst")
        .expect("headers should parse");

    let non_start_headers: Vec<_> = headers
        .headers
        .iter()
        .filter(|header| !matches!(header.kind, HeaderKind::StartFunction))
        .collect();

    assert!(
        non_start_headers
            .iter()
            .all(|header| header.export_mode == HeaderExportMode::Private),
        "unmarked declarations in #mod.bst should remain private"
    );
}

#[test]
fn duplicate_declaration_detection_works_with_exported_declarations() {
    let result = parse_single_file_headers_with_entry(
        "export Button = | label String |\nButton = | title String |\n",
        "src/#mod.bst",
        "src/#page.bst",
    );
    let errors = expect_header_error(
        result,
        "duplicate declaration with export should still be rejected",
    );

    assert!(errors.diagnostics.iter().any(|diagnostic| matches!(
        diagnostic.payload,
        DiagnosticPayload::DuplicateDeclaration { .. }
    )));
}

#[test]
fn export_before_constant_marks_header_public() {
    let source = "export theme #= \"dark\"\nexport threshold #Int = 42\n";
    let headers = parse_single_file_headers_with_entry(source, "src/#mod.bst", "src/#page.bst")
        .expect("headers should parse");

    let public_constants: Vec<_> = headers
        .headers
        .iter()
        .filter(|header| {
            matches!(header.kind, HeaderKind::Constant { .. })
                && header.export_mode == HeaderExportMode::Public
        })
        .collect();

    assert_eq!(
        public_constants.len(),
        2,
        "expected two public constant headers"
    );
}

#[test]
fn export_before_type_alias_marks_header_public() {
    let source = "export UserId as Int\n";
    let headers = parse_single_file_headers_with_entry(source, "src/#mod.bst", "src/#page.bst")
        .expect("headers should parse");

    let alias_header = headers
        .headers
        .iter()
        .find(|header| matches!(header.kind, HeaderKind::TypeAlias { .. }))
        .expect("expected type alias header");

    assert_eq!(alias_header.export_mode, HeaderExportMode::Public);
}

#[test]
fn export_before_choice_marks_header_public() {
    let source = "export Status :: Ready, Failed | message String |;\n";
    let headers = parse_single_file_headers_with_entry(source, "src/#mod.bst", "src/#page.bst")
        .expect("headers should parse");

    let choice_header = headers
        .headers
        .iter()
        .find(|header| matches!(header.kind, HeaderKind::Choice { .. }))
        .expect("expected choice header");

    assert_eq!(choice_header.export_mode, HeaderExportMode::Public);
}

#[test]
fn export_before_trait_declaration_marks_header_public() {
    let source = "export DISPLAY_TEXT must:\n    display |This| -> String\n;\n";
    let headers = parse_single_file_headers_with_entry(source, "src/#mod.bst", "src/#page.bst")
        .expect("headers should parse");

    let trait_header = headers
        .headers
        .iter()
        .find(|header| matches!(header.kind, HeaderKind::Trait { .. }))
        .expect("expected trait header");

    assert_eq!(trait_header.export_mode, HeaderExportMode::Public);
}

#[test]
fn export_before_trait_conformance_is_rejected() {
    let result = parse_single_file_headers_with_entry(
        "export Label must DISPLAY_TEXT\n",
        "src/#mod.bst",
        "src/#page.bst",
    );
    let errors = expect_header_error(result, "trait conformance should not be exported");

    assert!(errors.diagnostics.iter().any(|diagnostic| diagnostic.kind
        == DiagnosticKind::Rule(RuleDiagnosticKind::InvalidExportTarget)));
}

#[test]
fn export_before_unsupported_runtime_statement_is_rejected() {
    let result = parse_single_file_headers_with_entry(
        "export io(\"hello\")\n",
        "src/#mod.bst",
        "src/#page.bst",
    );
    let errors = expect_header_error(result, "export before runtime statement should be rejected");

    assert!(errors.diagnostics.iter().any(|diagnostic| diagnostic.kind
        == DiagnosticKind::Rule(RuleDiagnosticKind::InvalidExportTarget)));
}

#[test]
fn export_before_runtime_template_is_rejected() {
    let result = parse_single_file_headers_with_entry(
        "export [: hello ]\n",
        "src/#mod.bst",
        "src/#page.bst",
    );
    let errors = expect_header_error(result, "export before runtime template should be rejected");

    assert!(errors.diagnostics.iter().any(|diagnostic| diagnostic.kind
        == DiagnosticKind::Rule(RuleDiagnosticKind::RuntimeTemplateInModuleFacade)));
}

#[test]
fn export_import_and_private_import_normalize_to_one_public_record() {
    let mut string_table = StringTable::new();
    let output = prepare_single_file(
        "import @./button { Button }\nexport import @./button { Button }\n",
        &PathBuf::from("src/#mod.bst"),
        &PathBuf::from("src/#page.bst"),
        &mut string_table,
    );

    assert_eq!(output.file_imports.len(), 1);
    assert_eq!(output.file_imports[0].export_mode, HeaderExportMode::Public);
}
