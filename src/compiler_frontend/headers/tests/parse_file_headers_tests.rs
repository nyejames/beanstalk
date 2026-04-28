//! Header parsing regression tests.
//!
//! WHAT: validates top-level declaration classification, signature extraction, dependency edge
//!       generation, import normalization, and header-level diagnostics.
//! WHY: headers are the first compiler stage after tokenization; incorrect classification or
//!      dependency edges break everything downstream.

use super::*;
use crate::compiler_frontend::ast::statements::functions::{
    FunctionReturn, FunctionSignature, ReturnChannel, ReturnSlot,
};
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::external_packages::ExternalPackageRegistry;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::style_directives::StyleDirectiveRegistry;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::lexer::tokenize;
use crate::compiler_frontend::tokenizer::newline_handling::NewlineMode;
use crate::compiler_frontend::tokenizer::tokens::{TokenKind, TokenizeMode};
use std::path::PathBuf;

fn parse_single_file_headers(source: &str) -> Headers {
    let mut string_table = StringTable::new();
    let style_directives = StyleDirectiveRegistry::built_ins();
    let file_path = PathBuf::from("src/#page.bst");
    let interned_path = InternedPath::from_path_buf(&file_path, &mut string_table);
    let file_tokens = tokenize(
        source,
        &interned_path,
        TokenizeMode::Normal,
        NewlineMode::NormalizeToLf,
        &style_directives,
        &mut string_table,
        None,
    )
    .expect("tokenization should succeed");

    let external_package_registry = ExternalPackageRegistry::new();
    let mut warnings = Vec::new();

    parse_headers(
        vec![file_tokens],
        &external_package_registry,
        &mut warnings,
        &file_path,
        HeaderParseOptions::default(),
        &mut string_table,
    )
    .expect("headers should parse")
}

fn parse_single_file_headers_with_warnings(
    source: &str,
) -> (
    Headers,
    Vec<crate::compiler_frontend::compiler_warnings::CompilerWarning>,
) {
    let mut string_table = StringTable::new();
    let style_directives = StyleDirectiveRegistry::built_ins();
    let file_path = PathBuf::from("src/#page.bst");
    let interned_path = InternedPath::from_path_buf(&file_path, &mut string_table);
    let file_tokens = tokenize(
        source,
        &interned_path,
        TokenizeMode::Normal,
        NewlineMode::NormalizeToLf,
        &style_directives,
        &mut string_table,
        None,
    )
    .expect("tokenization should succeed");

    let external_package_registry = ExternalPackageRegistry::new();
    let mut warnings = Vec::new();

    let headers = parse_headers(
        vec![file_tokens],
        &external_package_registry,
        &mut warnings,
        &file_path,
        HeaderParseOptions::default(),
        &mut string_table,
    )
    .expect("headers should parse");

    (headers, warnings)
}

fn parse_single_file_headers_with_table(source: &str) -> (Headers, StringTable) {
    let mut string_table = StringTable::new();
    let style_directives = StyleDirectiveRegistry::built_ins();
    let file_path = PathBuf::from("src/#page.bst");
    let interned_path = InternedPath::from_path_buf(&file_path, &mut string_table);
    let file_tokens = tokenize(
        source,
        &interned_path,
        TokenizeMode::Normal,
        NewlineMode::NormalizeToLf,
        &style_directives,
        &mut string_table,
        None,
    )
    .expect("tokenization should succeed");

    let external_package_registry = ExternalPackageRegistry::new();
    let mut warnings = Vec::new();

    let headers = parse_headers(
        vec![file_tokens],
        &external_package_registry,
        &mut warnings,
        &file_path,
        HeaderParseOptions::default(),
        &mut string_table,
    )
    .expect("headers should parse");

    (headers, string_table)
}

fn parse_single_file_headers_with_entry(
    source: &str,
    file_path: &str,
    entry_file_path: &str,
) -> Result<Headers, Vec<CompilerError>> {
    let mut string_table = StringTable::new();
    let style_directives = StyleDirectiveRegistry::built_ins();
    let file_path = PathBuf::from(file_path);
    let entry_file_path = PathBuf::from(entry_file_path);
    let interned_path = InternedPath::from_path_buf(&file_path, &mut string_table);
    let file_tokens = tokenize(
        source,
        &interned_path,
        TokenizeMode::Normal,
        NewlineMode::NormalizeToLf,
        &style_directives,
        &mut string_table,
        None,
    )
    .expect("tokenization should succeed");

    let external_package_registry = ExternalPackageRegistry::new();
    let mut warnings = Vec::new();

    parse_headers(
        vec![file_tokens],
        &external_package_registry,
        &mut warnings,
        &entry_file_path,
        HeaderParseOptions::default(),
        &mut string_table,
    )
}

fn first_function_signature(headers: &Headers) -> &FunctionSignature {
    headers
        .headers
        .iter()
        .find_map(|header| match &header.kind {
            HeaderKind::Function { signature } => Some(signature),
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
    let headers = parse_single_file_headers("import @libs/html/basic\n[basic]\n");
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
fn exported_constant_headers_are_parsed() {
    let headers = parse_single_file_headers("#theme = \"dark\"\n");
    assert!(
        headers
            .headers
            .iter()
            .any(|header| matches!(header.kind, HeaderKind::Constant { .. })),
        "expected exported constant header"
    );
}

#[test]
fn malformed_children_wrapper_constant_initializer_reports_eof_delimiter_error() {
    let result = parse_single_file_headers_with_entry(
        "#broken = [$children([:<li>[$slot]</li>):\n<ul>[$slot]</ul>\n]\n",
        "src/#page.bst",
        "src/#page.bst",
    );

    assert!(
        result.is_err(),
        "unterminated '$children(..)' wrapper templates should fail instead of hanging"
    );
    let errors = result.err().expect("expected parse errors");

    assert!(errors.iter().any(|error| {
        error
            .msg
            .contains("Unexpected end of file while parsing declaration initializer")
            && error.msg.contains("Missing ']'")
    }));
}

#[test]
fn malformed_nested_children_wrapper_constant_initializer_reports_eof_delimiter_error() {
    let result = parse_single_file_headers_with_entry(
        "#broken = [$children([:<tr>[$slot]</tr>):\n<table>\n    [$children([:<td>[$slot]</td>):[$slot]]\n</table>\n]\n",
        "src/#page.bst",
        "src/#page.bst",
    );

    assert!(
        result.is_err(),
        "nested unterminated '$children(..)' wrapper templates should fail instead of hanging"
    );
    let errors = result.err().expect("expected parse errors");

    assert!(errors.iter().any(|error| {
        error
            .msg
            .contains("Unexpected end of file while parsing declaration initializer")
            && error.msg.contains("Missing ']'")
    }));
}

#[test]
fn exported_untyped_constant_has_no_strict_dependencies() {
    let headers = parse_single_file_headers("import @styles/docs/navbar\n#theme = navbar\n");
    let constant_header = headers
        .headers
        .iter()
        .find(|header| matches!(header.kind, HeaderKind::Constant { .. }))
        .expect("expected constant header");

    assert!(
        constant_header.dependencies.is_empty(),
        "strict constant dependencies come from declared type syntax only"
    );
}

#[test]
fn exported_typed_constant_headers_are_parsed_and_follow_on_constant_stays_header() {
    let headers =
        parse_single_file_headers("# page String = [: world]\n\n# test = [page: Hello ]\n");

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
        "follow-on '# test = ...' should remain a constant header"
    );
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
        .find(|header| matches!(header.kind, HeaderKind::ConstTemplate))
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
        "import @libs/html/basic\n\
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
fn hash_prefixed_declarations_still_parse_as_headers_without_elevating_body_symbols() {
    let (headers, string_table) = parse_single_file_headers_with_table(
        "#theme = \"dark\"\n\
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
        "the '#theme = ...' declaration should remain a real top-level constant header"
    );
    assert_eq!(
        headers.headers.len(),
        2,
        "expected one exported constant header plus the implicit start header"
    );
    assert!(
        headers
            .headers
            .iter()
            .any(|header| matches!(header.kind, HeaderKind::Constant { .. })),
        "exported declaration after '#' should still classify as a constant header"
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
    let headers = parse_single_file_headers("#f||:\n;\n");
    let signature = first_function_signature(&headers);

    assert!(signature.returns.is_empty());
}

#[test]
fn function_value_return_is_parsed_into_canonical_return_slots() {
    let headers = parse_single_file_headers("#f|| -> Int:\n;\n");
    let signature = first_function_signature(&headers);

    assert_eq!(
        &signature.returns,
        &vec![ReturnSlot::success(FunctionReturn::Value(DataType::Int))]
    );
}

#[test]
fn function_named_return_is_preserved_for_ast_resolution() {
    let headers = parse_single_file_headers("#f|| -> Point:\n;\n");
    let signature = first_function_signature(&headers);

    assert!(matches!(
        signature.returns.as_slice(),
        [ReturnSlot {
            value: FunctionReturn::Value(DataType::NamedType(_)),
            channel: ReturnChannel::Success
        }]
    ));
}

#[test]
fn function_alias_return_is_parsed_into_canonical_return_slots() {
    let headers = parse_single_file_headers("#f|x Int| -> x:\n;\n");
    let signature = first_function_signature(&headers);

    assert_eq!(
        &signature.returns,
        &vec![ReturnSlot::success(FunctionReturn::AliasCandidates {
            parameter_indices: vec![0],
            data_type: DataType::Int,
        })]
    );
}

#[test]
fn function_signature_rejects_void_return_syntax() {
    let source = format!("#f|| {}{}:\n;\n", "-> ", "Void");
    let result = parse_single_file_headers_with_entry(&source, "src/#page.bst", "src/#page.bst");
    assert!(result.is_err(), "void return syntax must be rejected");
    let errors = result.err().expect("expected parse errors");

    assert!(errors.iter().any(|error| {
        error
            .msg
            .contains("Void is not a valid function return declaration")
            || error.msg.contains("omit '->' entirely")
    }));
}

#[test]
fn function_signature_rejects_none_return_syntax() {
    let source = format!("#f|| {}{}:\n;\n", "-> ", "None");
    let result = parse_single_file_headers_with_entry(&source, "src/#page.bst", "src/#page.bst");
    assert!(result.is_err(), "none return syntax must be rejected");
    let errors = result.err().expect("expected parse errors");

    assert!(errors.iter().any(|error| {
        error
            .msg
            .contains("None is not a valid function return type")
    }));
}

#[test]
fn function_signature_preserves_unknown_symbolic_return_for_ast_resolution() {
    let headers = parse_single_file_headers("#f|| -> MissingType:\n;\n");
    let signature = first_function_signature(&headers);

    assert!(matches!(
        signature.returns.as_slice(),
        [ReturnSlot {
            value: FunctionReturn::Value(DataType::NamedType(_)),
            channel: ReturnChannel::Success
        }]
    ));
}

#[test]
fn function_signature_reports_missing_arrow_before_return_type() {
    let result = parse_single_file_headers_with_entry(
        "#f|x Int| Int:\n;\n",
        "src/#page.bst",
        "src/#page.bst",
    );
    assert!(
        result.is_err(),
        "missing arrow before return type must fail"
    );
    let errors = result.err().expect("expected parse errors");

    assert!(errors.iter().any(|error| {
        error
            .msg
            .contains("Expected '->' or ':' after function parameters")
    }));
}

#[test]
fn function_signature_reports_missing_colon_after_return_list() {
    let result =
        parse_single_file_headers_with_entry("#f|| -> Int\n;\n", "src/#page.bst", "src/#page.bst");
    assert!(
        result.is_err(),
        "missing ':' after return declarations must fail"
    );
    let errors = result.err().expect("expected parse errors");

    assert!(errors.iter().any(|error| {
        error
            .msg
            .contains("Function return declarations must end with ':'")
    }));
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

    assert!(errors.iter().any(|error| {
        error.error_type == crate::compiler_frontend::compiler_errors::ErrorType::Rule
            && error
                .msg
                .contains("There is already a top-level declaration using this name")
    }));
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

    let HeaderKind::Choice { variants } = &choice_header.kind else {
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

    assert!(errors.iter().any(|error| {
        error.msg.contains("Duplicate choice variant")
            && error.msg.contains("Variant names must be unique")
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
    assert!(payload_errors.iter().any(|error| {
        error
            .msg
            .contains("Choice payload shorthand is not supported")
    }));

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
    assert!(payload_paren_errors.iter().any(|error| {
        error
            .msg
            .contains("Constructor-style choice declarations are not supported")
    }));

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
    assert!(default_errors.iter().any(|error| {
        error
            .msg
            .contains("Choice variant default values are deferred")
    }));
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

    let HeaderKind::Choice { variants } = &choice_header.kind else {
        panic!("expected choice metadata");
    };

    assert_eq!(variants.len(), 1, "expected one parsed variant");
    assert_eq!(
        string_table.resolve(variants[0].id),
        "Pending",
        "expected Pending variant"
    );
    match &variants[0].payload {
        crate::compiler_frontend::declaration_syntax::choice::ChoiceVariantPayload::Record {
            fields,
        } => {
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
        "#SITE_TITLE = \"Beanstalk\"\nStatus_type :: bad_variant;\n",
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
    assert!(warnings.iter().any(|warning| {
        warning.msg.contains("Status_type") && warning.msg.contains("CamelCase")
    }));
    assert!(warnings.iter().any(|warning| {
        warning.msg.contains("bad_variant") && warning.msg.contains("CamelCase")
    }));
    assert!(
        warnings
            .iter()
            .all(|warning| !warning.msg.contains("SITE_TITLE")),
        "UPPER_CASE top-level constants should not emit naming warnings"
    );
}

#[test]
fn header_parsing_rejects_keyword_shadow_constant_name() {
    let result =
        parse_single_file_headers_with_entry("#FALSE = 1\n", "src/#page.bst", "src/#page.bst");
    assert!(
        result.is_err(),
        "keyword-shadow top-level constants must fail during header parsing"
    );
    let errors = result.err().expect("expected parse errors");

    assert!(errors.iter().any(|error| {
        error.msg.contains("Identifier 'FALSE' is reserved")
            && error.msg.contains("shadows language keyword 'false'")
    }));
}

#[test]
fn trait_declarations_using_must_are_reserved_during_header_parsing() {
    let result = parse_single_file_headers_with_entry(
        "Drawable must:\n    draw |This, surface Surface| -> String;\n;\n",
        "src/#page.bst",
        "src/#page.bst",
    );

    assert!(
        result.is_err(),
        "trait declarations using 'must' should fail during header parsing"
    );
    let errors = result.err().expect("expected parse errors");

    assert!(errors.iter().any(|error| {
        error.error_type == crate::compiler_frontend::compiler_errors::ErrorType::Rule
            && error
                .msg
                .contains("Trait declarations using 'must' are reserved for traits")
            && error.msg.contains("deferred for Alpha")
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
fn typed_constant_creates_strict_dependency_on_declared_type() {
    // WHY: the declared type creates a structural ordering constraint so that the type
    // is sorted before any constant that references it. Initializer-expression references
    // do NOT create strict deps — only the declared type annotation does.
    let (headers, string_table) = parse_single_file_headers_with_table(
        "import @styles/NavBar\n#theme NavBar = default_navbar\n",
    );

    let constant_header = headers
        .headers
        .iter()
        .find(|header| matches!(header.kind, HeaderKind::Constant { .. }))
        .expect("expected constant header");

    assert!(
        !constant_header.dependencies.is_empty(),
        "typed constant must create a strict dependency on its declared type"
    );
    assert!(
        constant_header
            .dependencies
            .iter()
            .any(|dep| dep.name_str(&string_table) == Some("NavBar")),
        "strict dependency must reference the declared type name 'NavBar'"
    );
}

#[test]
fn struct_fields_create_strict_dependencies_on_named_field_types() {
    // WHY: struct fields whose types are user-defined names create strict sort edges so that
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
        "Span must carry a strict dependency on Point via its field type annotations"
    );
}

#[test]
fn constant_header_with_declared_type_captures_type_in_declaration() {
    // Confirms the header-stage contract: declared type annotation is present in the
    // Constant header's declaration, proving initializer resolution is deferred to AST.
    let headers = parse_single_file_headers("#threshold Int = 42\n");

    let constant_header = headers
        .headers
        .iter()
        .find(|header| matches!(header.kind, HeaderKind::Constant { .. }))
        .expect("expected constant header");

    let HeaderKind::Constant { declaration } = &constant_header.kind else {
        panic!("expected Constant header kind");
    };

    assert!(
        !matches!(declaration.type_annotation, DataType::Inferred),
        "declared type annotation on a typed constant must be resolved at the header stage, not left as Inferred"
    );
}
