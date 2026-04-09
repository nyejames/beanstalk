//! AST stage modules for module-wide typed syntax construction.
//!
//! WHAT: groups expression/statement parsing, header-to-AST lowering, and template AST handling.

use crate::compiler_frontend::ast::ast::ScopeContext;
use crate::compiler_frontend::ast::ast_nodes::AstNode;
use crate::compiler_frontend::ast::statements::body_dispatch::parse_function_body_statements;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::compiler_warnings::CompilerWarning;
use crate::compiler_frontend::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::FileTokens;

pub(crate) mod module_ast;
pub(crate) mod signatures;
pub(crate) use module_ast as ast;
pub(crate) mod ast_nodes;
pub(crate) mod import_bindings;
pub(crate) mod receiver_methods;
pub(crate) mod type_resolution;
pub(crate) mod expressions {
    pub(crate) mod call_argument;
    pub(crate) mod call_validation;
    pub(crate) mod eval_expression;
    pub(crate) mod expression;
    pub(crate) mod function_calls;
    pub(crate) mod mutation;
    pub(crate) mod parse_expression;
    pub(crate) mod struct_instance;
}
pub(crate) mod statements {
    pub(crate) mod body_dispatch;
    pub(crate) mod body_expr_stmt;
    pub(crate) mod body_return;
    pub(crate) mod body_symbol;
    pub(crate) mod branching;
    pub(crate) mod choices;
    pub(crate) mod collections;
    pub(crate) mod condition_validation;
    pub(crate) mod declaration_syntax;
    pub(crate) mod declarations;
    pub(crate) mod functions;
    pub(crate) mod loops;
    pub(crate) mod multi_bind;
    pub(crate) mod result_handling;
    pub(crate) mod structs;
}
pub(crate) mod field_access;
pub(crate) mod place_access;
pub(crate) mod templates;

// WHAT: public(crate) entrypoint for function/start-function body parsing.
// WHY: callers should import one obvious `ast`-root function while detailed statement parsing
// lives in focused helper modules.
pub(crate) fn function_body_to_ast(
    token_stream: &mut FileTokens,
    context: ScopeContext,
    warnings: &mut Vec<CompilerWarning>,
    string_table: &mut StringTable,
) -> Result<Vec<AstNode>, CompilerError> {
    parse_function_body_statements(token_stream, context, warnings, string_table)
}

#[cfg(test)]
#[path = "tests/parser_error_recovery_tests.rs"]
mod parser_error_recovery_tests;
#[cfg(test)]
pub(crate) mod test_support;
