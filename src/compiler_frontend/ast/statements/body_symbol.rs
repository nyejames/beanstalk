//! Symbol-led function-body statement parsing.
//!
//! WHAT: parses statement forms that start with a symbol inside function/start-function bodies.
//! WHY: symbol-led statements are the densest statement branch (mutation, calls, declarations,
//! access chains, and start-import callability), so isolating them keeps dispatch readable.

use crate::compiler_frontend::ast::ast::ScopeContext;
use crate::compiler_frontend::ast::ast_nodes::{AstNode, NodeKind};
use crate::compiler_frontend::ast::expressions::expression::ExpressionKind;
use crate::compiler_frontend::ast::expressions::function_calls::parse_function_call;
use crate::compiler_frontend::ast::expressions::mutation::{
    handle_mutation, handle_mutation_target,
};
use crate::compiler_frontend::ast::field_access::parse_field_access;
use crate::compiler_frontend::ast::receiver_methods::free_function_receiver_method_call_error;
use crate::compiler_frontend::ast::statements::body_expr_stmt::parse_symbol_expression_statement_candidate;
use crate::compiler_frontend::ast::statements::declarations::new_declaration;
use crate::compiler_frontend::ast::statements::multi_bind::parse_multi_bind_statement;
use crate::compiler_frontend::builtins::error_type::is_reserved_builtin_symbol;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::compiler_warnings::CompilerWarning;
use crate::compiler_frontend::string_interning::{StringId, StringTable};
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, TokenKind};
use crate::{return_rule_error, return_syntax_error};

fn push_accessed_symbol_statement(
    accessed: AstNode,
    ast: &mut Vec<AstNode>,
    context: &ScopeContext,
    token_stream: &FileTokens,
    symbol_id: StringId,
    string_table: &StringTable,
) -> Result<(), CompilerError> {
    match accessed.kind {
        NodeKind::MethodCall { .. } => {
            ast.push(AstNode {
                kind: NodeKind::Rvalue(accessed.get_expr()?),
                location: accessed.location,
                scope: context.scope.clone(),
            });
            Ok(())
        }
        NodeKind::FieldAccess { .. } => {
            return_syntax_error!(
                format!(
                    "Unexpected token '{:?}' after field access '{}'. Field reads are not valid standalone statements.",
                    token_stream.current_token_kind(),
                    string_table.resolve(symbol_id)
                ),
                token_stream.current_location(),
                {
                    CompilationStage => "AST Construction",
                    PrimarySuggestion => "Assign the field to a variable, mutate it, or call a method instead of leaving it as a standalone statement",
                }
            );
        }
        _ => {
            return_syntax_error!(
                "Standalone expression is not a valid statement in this position.",
                token_stream.current_location(),
                {
                    CompilationStage => "AST Construction",
                    PrimarySuggestion => "Use an assignment, call, control-flow statement, or declaration here",
                }
            );
        }
    }
}

pub(crate) fn parse_symbol_statement(
    token_stream: &mut FileTokens,
    ast: &mut Vec<AstNode>,
    context: &mut ScopeContext,
    warnings: &mut Vec<CompilerWarning>,
    string_table: &mut StringTable,
) -> Result<(), CompilerError> {
    let TokenKind::Symbol(id) = token_stream.current_token_kind().to_owned() else {
        return_syntax_error!(
            "Expected a symbol-led statement.",
            token_stream.current_location(),
            {
                CompilationStage => "AST Construction",
            }
        );
    };

    if is_reserved_builtin_symbol(string_table.resolve(id)) {
        return_rule_error!(
            format!(
                "'{}' is reserved as a builtin language type.",
                string_table.resolve(id)
            ),
            token_stream.current_location(),
            {
                CompilationStage => "AST Construction",
                PrimarySuggestion => "Use a different symbol name for user variables and declarations",
            }
        );
    }

    let full_path = context.scope.append(id);

    if let Some(multi_bind) = parse_multi_bind_statement(token_stream, context, string_table)? {
        ast.push(multi_bind);
        return Ok(());
    }

    if let Some(arg) = context.get_reference(&id) {
        match token_stream.peek_next_token() {
            Some(next_token) if next_token.is_assignment_operator() => {
                token_stream.advance();
                ast.push(handle_mutation(token_stream, arg, context, string_table)?);
                return Ok(());
            }

            Some(TokenKind::Dot) => {
                token_stream.advance();
                let accessed = parse_field_access(token_stream, arg, context, string_table)?;

                if token_stream.current_token_kind().is_assignment_operator() {
                    ast.push(handle_mutation_target(
                        token_stream,
                        arg,
                        accessed,
                        context,
                        string_table,
                    )?);
                    return Ok(());
                }

                push_accessed_symbol_statement(
                    accessed,
                    ast,
                    context,
                    token_stream,
                    id,
                    string_table,
                )?;
                return Ok(());
            }

            Some(TokenKind::DatatypeInt)
            | Some(TokenKind::DatatypeFloat)
            | Some(TokenKind::DatatypeBool)
            | Some(TokenKind::DatatypeString)
            | Some(TokenKind::DatatypeChar)
            | Some(TokenKind::Mutable) => {
                return_rule_error!(
                    format!("Variable '{}' is already declared. Shadowing is not supported in Beanstalk. Use '=' to mutate its value or choose a different variable name", string_table.resolve(id)),
                    token_stream.current_location(), {
                        CompilationStage => "AST Construction",
                        PrimarySuggestion => "Use '=' to mutate the existing variable or choose a different name",
                    }
                );
            }

            _ => {
                let expr = parse_symbol_expression_statement_candidate(
                    token_stream,
                    context,
                    id,
                    string_table,
                )?;

                ast.push(AstNode {
                    kind: NodeKind::Rvalue(expr),
                    location: token_stream.current_location(),
                    scope: context.scope.clone(),
                });
                return Ok(());
            }
        }
    }

    if let Some(host_func_call) = context.host_registry.get_function(string_table.resolve(id)) {
        token_stream.advance();
        let signature = host_func_call.params_to_signature(string_table);

        ast.push(parse_function_call(
            token_stream,
            &full_path,
            context,
            &signature,
            false,
            Some(warnings),
            string_table,
        )?);
        return Ok(());
    }

    if token_stream.peek_next_token() == Some(&TokenKind::OpenParenthesis) {
        if let Some(method_entry) = context.lookup_visible_receiver_method_by_name(id) {
            return Err(free_function_receiver_method_call_error(
                id,
                method_entry,
                token_stream.current_location(),
                "AST Construction",
                string_table,
            ));
        }

        return_rule_error!(
            format!(
                "Call target '{}' is not declared in this scope and is not a registered host function.",
                string_table.resolve(id)
            ),
            token_stream.current_location(), {
                CompilationStage => "AST Construction",
                PrimarySuggestion => "Declare/import this function before calling it, or check the function name spelling",
                AlternativeSuggestion => "If this should be a host function, register it in the host registry for this backend",
            }
        );
    }

    let arg = new_declaration(token_stream, id, context, warnings, string_table)?;

    match arg.value.kind {
        ExpressionKind::StructDefinition(ref params) => {
            ast.push(AstNode {
                kind: NodeKind::StructDefinition(arg.id.to_owned(), params.to_owned()),
                location: token_stream.current_location(),
                scope: context.scope.clone(),
            });
        }

        ExpressionKind::Function(ref signature, ref body) => {
            ast.push(AstNode {
                kind: NodeKind::Function(arg.id.to_owned(), signature.to_owned(), body.to_owned()),
                location: token_stream.current_location(),
                scope: context.scope.clone(),
            });
        }

        _ => {
            ast.push(AstNode {
                kind: NodeKind::VariableDeclaration(arg.to_owned()),
                location: token_stream.current_location(),
                scope: context.scope.clone(),
            });
        }
    }

    context.add_var(arg);
    Ok(())
}
