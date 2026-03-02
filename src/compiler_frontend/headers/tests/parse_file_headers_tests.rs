use super::*;
use crate::backends::function_registry::HostRegistry;
use crate::compiler_frontend::ast::statements::functions::{FunctionReturn, FunctionSignature};
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::tokenizer::tokenizer::tokenize;
use crate::compiler_frontend::tokenizer::tokens::TokenizeMode;
use std::path::PathBuf;

fn parse_single_file_headers(source: &str) -> Headers {
    let mut string_table = StringTable::new();
    let file_path = PathBuf::from("src/#page.bst");
    let interned_path = InternedPath::from_path_buf(&file_path, &mut string_table);
    let file_tokens = tokenize(
        source,
        &interned_path,
        TokenizeMode::Normal,
        &mut string_table,
    )
    .expect("tokenization should succeed");

    let host_registry = HostRegistry::new(&mut string_table);
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

fn parse_single_file_headers_with_entry(
    source: &str,
    file_path: &str,
    entry_file_path: &str,
) -> Result<Headers, Vec<crate::compiler_frontend::compiler_errors::CompilerError>> {
    let mut string_table = StringTable::new();
    let file_path = PathBuf::from(file_path);
    let entry_file_path = PathBuf::from(entry_file_path);
    let interned_path = InternedPath::from_path_buf(&file_path, &mut string_table);
    let file_tokens = tokenize(
        source,
        &interned_path,
        TokenizeMode::Normal,
        &mut string_table,
    )
    .expect("tokenization should succeed");

    let host_registry = HostRegistry::new(&mut string_table);
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
    let headers = parse_single_file_headers("import @(libs/html/basic)\n[basic]\n");
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
    let headers = parse_single_file_headers("import @(styles/docs/navbar)\n#theme = navbar\n");
    let constant_header = headers
        .headers
        .iter()
        .find(|header| matches!(header.kind, HeaderKind::Constant { .. }))
        .expect("expected constant header");

    assert_eq!(constant_header.dependencies.len(), 1);
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
        &vec![FunctionReturn::Value(DataType::Int)]
    );
}

#[test]
fn function_alias_return_is_parsed_into_canonical_return_slots() {
    let headers = parse_single_file_headers("#f|x Int| -> x:\n;\n");
    let signature = first_function_signature(&headers);

    assert_eq!(
        &signature.returns,
        &vec![FunctionReturn::AliasCandidates {
            parameter_indices: vec![0],
            data_type: DataType::Int,
        }]
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
fn function_signature_rejects_unknown_symbolic_return_syntax() {
    let result = parse_single_file_headers_with_entry(
        "#f|| -> MissingType:\n;\n",
        "src/#page.bst",
        "src/#page.bst",
    );
    assert!(
        result.is_err(),
        "unknown symbolic return declarations must be rejected"
    );
    let errors = result.err().expect("expected parse errors");

    assert!(errors.iter().any(|error| {
        error
            .msg
            .contains("Unknown return declaration 'MissingType'")
    }));
}
