use super::*;
use crate::compiler_frontend::ast::statements::functions::{
    FunctionReturn, FunctionSignature, ReturnChannel, ReturnSlot,
};
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::host_functions::HostRegistry;
use crate::compiler_frontend::style_directives::StyleDirectiveRegistry;
use crate::compiler_frontend::tokenizer::lexer::tokenize;
use crate::compiler_frontend::tokenizer::newline_handling::NewlineMode;
use crate::compiler_frontend::tokenizer::tokens::TokenizeMode;
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

    let host_registry = HostRegistry::new();
    let mut warnings = Vec::new();

    parse_headers(
        vec![file_tokens],
        &host_registry,
        &mut warnings,
        &file_path,
        &mut string_table,
    )
    .expect("headers should parse")
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

    let host_registry = HostRegistry::new();
    let mut warnings = Vec::new();

    let headers = parse_headers(
        vec![file_tokens],
        &host_registry,
        &mut warnings,
        &file_path,
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

    let host_registry = HostRegistry::new();
    let mut warnings = Vec::new();

    parse_headers(
        vec![file_tokens],
        &host_registry,
        &mut warnings,
        &entry_file_path,
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

#[test]
fn import_paths_are_captured_for_start_function_dependencies() {
    let headers = parse_single_file_headers("import @libs/html/basic\n[basic]\n");
    let start_header = headers
        .headers
        .iter()
        .find(|header| matches!(header.kind, HeaderKind::StartFunction))
        .expect("expected start function header");

    assert!(
        !start_header.dependencies.is_empty(),
        "start function should depend on imported symbol path"
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
fn exported_constant_dependency_tracks_imported_symbol() {
    let headers = parse_single_file_headers("import @styles/docs/navbar\n#theme = navbar\n");
    let constant_header = headers
        .headers
        .iter()
        .find(|header| matches!(header.kind, HeaderKind::Constant { .. }))
        .expect("expected constant header");

    assert_eq!(constant_header.dependencies.len(), 1);
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
fn constant_symbol_dependencies_track_import_struct_and_constant_and_ignore_member_access() {
    let headers = parse_single_file_headers(
        "import @styles/docs/theme\n\
         #base = \"seed\"\n\
         Card = |\n\
             title String,\n\
         |\n\
         #result Card = Card(theme.value + base)\n",
    );

    let struct_path = headers
        .headers
        .iter()
        .find(|header| matches!(header.kind, HeaderKind::Struct { .. }))
        .expect("expected struct header")
        .tokens
        .src_path
        .to_owned();

    let base_constant_path = headers
        .headers
        .iter()
        .find(|header| {
            matches!(
                &header.kind,
                HeaderKind::Constant { metadata } if metadata.file_constant_order == 0
            )
        })
        .expect("expected first constant header")
        .tokens
        .src_path
        .to_owned();

    let result_header = headers
        .headers
        .iter()
        .find(|header| {
            matches!(
                &header.kind,
                HeaderKind::Constant { metadata } if metadata.file_constant_order == 1
            )
        })
        .expect("expected result constant header");

    let import_dependency = result_header
        .dependencies
        .iter()
        .next()
        .expect("result constant should track imported symbol dependency")
        .to_owned();

    let HeaderKind::Constant { metadata } = &result_header.kind else {
        panic!("expected constant metadata");
    };

    assert_eq!(
        metadata.symbol_dependencies.len(),
        3,
        "expected dependencies for imported symbol, local struct, and local constant only"
    );
    assert!(
        metadata.symbol_dependencies.contains(&import_dependency),
        "constant symbol dependencies should include imported symbols"
    );
    assert!(
        metadata.symbol_dependencies.contains(&struct_path),
        "constant symbol dependencies should include same-file struct references"
    );
    assert!(
        metadata.symbol_dependencies.contains(&base_constant_path),
        "constant symbol dependencies should include same-file constant references"
    );
}

#[test]
fn struct_default_dependencies_track_imports_and_local_constants() {
    let headers = parse_single_file_headers(
        "import @styles/docs/theme\n\
         #base = \"red\"\n\
         Card = |\n\
             title String = theme + base,\n\
         |\n",
    );
    let struct_header = headers
        .headers
        .iter()
        .find(|header| matches!(header.kind, HeaderKind::Struct { .. }))
        .expect("expected struct header");

    let HeaderKind::Struct { metadata } = &struct_header.kind else {
        panic!("expected struct metadata");
    };

    assert_eq!(
        metadata.default_value_dependencies.len(),
        2,
        "struct default dependencies should capture imported and local constant symbols",
    );
}

#[test]
fn struct_default_dependencies_ignore_field_access_member_symbol() {
    let headers = parse_single_file_headers(
        "import @styles/docs/theme\n\
         Card = |\n\
             title String = theme.value,\n\
         |\n",
    );
    let struct_header = headers
        .headers
        .iter()
        .find(|header| matches!(header.kind, HeaderKind::Struct { .. }))
        .expect("expected struct header");

    let HeaderKind::Struct { metadata } = &struct_header.kind else {
        panic!("expected struct metadata");
    };

    assert_eq!(
        metadata.default_value_dependencies.len(),
        1,
        "member symbol after '.' should not be treated as an extra dependency edge",
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

    let HeaderKind::Choice { metadata } = &choice_header.kind else {
        panic!("expected choice metadata");
    };

    assert_eq!(metadata.variants.len(), 3, "expected three parsed variants");
    assert_eq!(string_table.resolve(metadata.variants[0].name), "Ready");
    assert_eq!(string_table.resolve(metadata.variants[1].name), "Busy");
    assert_eq!(string_table.resolve(metadata.variants[2].name), "Idle");
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
fn choice_headers_reject_payload_variant_forms_for_alpha() {
    let payload_type_result = parse_single_file_headers_with_entry(
        "Status :: Ready String;\n",
        "src/#page.bst",
        "src/#page.bst",
    );
    assert!(
        payload_type_result.is_err(),
        "payload choice variants must fail in alpha"
    );
    let payload_errors = payload_type_result
        .err()
        .expect("expected payload parse errors");
    assert!(payload_errors.iter().any(|error| {
        error
            .msg
            .contains("Choice payload variants are deferred for Alpha")
    }));

    let payload_paren_result = parse_single_file_headers_with_entry(
        "Status :: Ready(String);\n",
        "src/#page.bst",
        "src/#page.bst",
    );
    assert!(
        payload_paren_result.is_err(),
        "constructor-style payload variants must fail in alpha"
    );
    let payload_paren_errors = payload_paren_result
        .err()
        .expect("expected constructor-style payload parse errors");
    assert!(payload_paren_errors.iter().any(|error| {
        error.msg.contains(
            "Choice payload variants using constructor-style declarations ('Variant(...)') are deferred for Alpha",
        )
    }));

    let defaults_result = parse_single_file_headers_with_entry(
        "Status :: Ready = true;\n",
        "src/#page.bst",
        "src/#page.bst",
    );
    assert!(
        defaults_result.is_err(),
        "choice variant defaults must fail in alpha"
    );
    let default_errors = defaults_result
        .err()
        .expect("expected default parse errors");
    assert!(default_errors.iter().any(|error| {
        error
            .msg
            .contains("Choice variant default values are deferred for Alpha")
    }));

    let tagged_result = parse_single_file_headers_with_entry(
        "Status :: Pending |\n    RetryCount Int,\n|;\n",
        "src/#page.bst",
        "src/#page.bst",
    );
    assert!(
        tagged_result.is_err(),
        "tagged choice variants must fail in alpha"
    );
    let tagged_errors = tagged_result.err().expect("expected tagged parse errors");
    assert!(tagged_errors.iter().any(|error| {
        error
            .msg
            .contains("Tagged choice variant bodies using '| ... |' are deferred for Alpha")
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
            && error.msg.contains("not implemented yet in Alpha")
    }));
}
