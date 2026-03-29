//! Shared AST parser test helpers.
//!
//! WHAT: builds small single-file AST fixtures by running the real tokenizer, header parser,
//! dependency sorter, and AST builder.
//! WHY: statement/parser tests should validate the true frontend pipeline instead of isolated
//! helper calls that can drift from production behavior.

use crate::compiler_frontend::FrontendBuildProfile;
use crate::compiler_frontend::ast::ast::Ast;
use crate::compiler_frontend::ast::ast_nodes::{AstNode, NodeKind};
use crate::compiler_frontend::ast::statements::functions::FunctionSignature;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::headers::parse_file_headers::parse_headers_with_path_resolver;
use crate::compiler_frontend::host_functions::HostRegistry;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::module_dependencies::resolve_module_dependencies;
use crate::compiler_frontend::paths::path_format::PathStringFormatConfig;
use crate::compiler_frontend::string_interning::StringTable;
use crate::compiler_frontend::style_directives::StyleDirectiveRegistry;
use crate::compiler_frontend::test_support::test_project_path_resolver;
use crate::compiler_frontend::tokenizer::lexer::tokenize;
use crate::compiler_frontend::tokenizer::newline_handling::NewlineMode;
use crate::compiler_frontend::tokenizer::tokens::TokenizeMode;
use crate::projects::settings::IMPLICIT_START_FUNC_NAME;

fn parse_single_file_ast_result(source: &str) -> Result<(Ast, StringTable), CompilerError> {
    let mut string_table = StringTable::new();
    let style_directives = StyleDirectiveRegistry::built_ins();
    let host_registry = HostRegistry::new();
    let file_path = InternedPath::from_single_str("#page.bst", &mut string_table);
    let file_tokens = tokenize(
        source,
        &file_path,
        TokenizeMode::Normal,
        NewlineMode::NormalizeToLf,
        &style_directives,
        &mut string_table,
        None,
    )?;

    let mut warnings = Vec::new();
    let headers = parse_headers_with_path_resolver(
        vec![file_tokens],
        &host_registry,
        &mut warnings,
        &std::path::PathBuf::from("#page.bst"),
        None,
        Some(test_project_path_resolver()),
        PathStringFormatConfig::default(),
        &mut string_table,
    )
    .map_err(|mut errors| errors.remove(0))?;

    let sorted_headers = resolve_module_dependencies(headers.headers, &mut string_table)
        .map_err(|mut errors| errors.remove(0))?;

    let entry_path = InternedPath::from_single_str("#page.bst", &mut string_table);
    let ast = Ast::new(
        sorted_headers,
        headers.top_level_template_items,
        &host_registry,
        &style_directives,
        &mut string_table,
        entry_path,
        FrontendBuildProfile::Dev,
        Some(test_project_path_resolver()),
        PathStringFormatConfig::default(),
    )
    .map_err(|mut messages| messages.errors.remove(0))?;

    Ok((ast, string_table))
}

pub(crate) fn parse_single_file_ast(source: &str) -> (Ast, StringTable) {
    parse_single_file_ast_result(source).expect("source should parse into AST")
}

pub(crate) fn parse_single_file_ast_error(source: &str) -> CompilerError {
    match parse_single_file_ast_result(source) {
        Ok(_) => panic!("source should fail during frontend parsing"),
        Err(error) => error,
    }
}

pub(crate) fn function_node_by_name<'a>(
    ast: &'a Ast,
    string_table: &StringTable,
    name: &str,
) -> &'a AstNode {
    ast.nodes
        .iter()
        .find(|node| match &node.kind {
            NodeKind::Function(path, ..) => path.name_str(string_table) == Some(name),
            _ => false,
        })
        .unwrap_or_else(|| panic!("expected function '{name}' in AST"))
}

pub(crate) fn function_signature_by_name<'a>(
    ast: &'a Ast,
    string_table: &StringTable,
    name: &str,
) -> &'a FunctionSignature {
    let node = function_node_by_name(ast, string_table, name);
    match &node.kind {
        NodeKind::Function(_, signature, _) => signature,
        _ => unreachable!("function lookup should only return function nodes"),
    }
}

pub(crate) fn function_body_by_name<'a>(
    ast: &'a Ast,
    string_table: &StringTable,
    name: &str,
) -> &'a [AstNode] {
    let node = function_node_by_name(ast, string_table, name);
    match &node.kind {
        NodeKind::Function(_, _, body) => body,
        _ => unreachable!("function lookup should only return function nodes"),
    }
}

pub(crate) fn start_function_body<'a>(ast: &'a Ast, string_table: &StringTable) -> &'a [AstNode] {
    function_body_by_name(ast, string_table, IMPLICIT_START_FUNC_NAME)
}
