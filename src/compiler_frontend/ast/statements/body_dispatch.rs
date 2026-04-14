//! Function-body statement dispatch loop.
//!
//! WHAT: routes one function/start-function token stream through statement-position parsing.
//! WHY: centralized dispatch keeps control flow readable while specialized helpers own detailed
//! syntax handling (symbol statements, returns, expression statements).

use crate::compiler_frontend::ast::ast::{ContextKind, ScopeContext};
use crate::compiler_frontend::ast::ast_nodes::{AstNode, NodeKind};
use crate::compiler_frontend::ast::expressions::expression::Expression;
use crate::compiler_frontend::ast::statements::body_expr_stmt::parse_expression_statement_candidate;
use crate::compiler_frontend::ast::statements::body_return::parse_return_statement;
use crate::compiler_frontend::ast::statements::body_symbol::parse_symbol_statement;
use crate::compiler_frontend::ast::statements::branching::create_branch;
use crate::compiler_frontend::ast::statements::loops::create_loop;
use crate::compiler_frontend::ast::templates::template_types::Template;
use crate::compiler_frontend::compiler_errors::{CompilerError, ErrorMetaDataKey};
use crate::compiler_frontend::compiler_warnings::CompilerWarning;
use crate::compiler_frontend::datatypes::Ownership;
use crate::compiler_frontend::reserved_trait_syntax::{
    reserved_trait_keyword_error, reserved_trait_keyword_or_dispatch_mismatch,
};
use crate::compiler_frontend::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, TokenKind};
use crate::projects::settings;
use crate::{ast_log, return_rule_error, return_syntax_error};

fn unexpected_function_body_token_error(
    token: &TokenKind,
    token_stream: &FileTokens,
) -> CompilerError {
    match token {
        TokenKind::Comma => {
            let mut error = CompilerError::new_syntax_error(
                "Unexpected ',' in function body. Commas only separate items in lists, arguments, or return declarations.",
                token_stream.current_location(),
            );
            error.new_metadata_entry(
                ErrorMetaDataKey::CompilationStage,
                String::from("AST Construction"),
            );
            error.new_metadata_entry(
                ErrorMetaDataKey::PrimarySuggestion,
                String::from("Remove the comma or place it inside a list/argument context"),
            );
            error
        }

        TokenKind::CloseParenthesis => {
            let mut error = CompilerError::new_syntax_error(
                "Unexpected ')' in function body. This usually means an earlier '(' was not parsed in a valid expression or call.",
                token_stream.current_location(),
            );
            error.new_metadata_entry(
                ErrorMetaDataKey::CompilationStage,
                String::from("AST Construction"),
            );
            error.new_metadata_entry(
                ErrorMetaDataKey::PrimarySuggestion,
                String::from(
                    "Remove the stray ')' or complete the expression/call before this point",
                ),
            );
            error
        }

        TokenKind::CloseCurly => {
            let mut error = CompilerError::new_syntax_error(
                "Unexpected '}' in function body. Curly braces are only valid for collection syntax.",
                token_stream.current_location(),
            );
            error.new_metadata_entry(
                ErrorMetaDataKey::CompilationStage,
                String::from("AST Construction"),
            );
            error.new_metadata_entry(
                ErrorMetaDataKey::PrimarySuggestion,
                String::from(
                    "Remove the stray '}' or use collection syntax in a valid expression context",
                ),
            );
            error
        }

        TokenKind::TypeParameterBracket => {
            let mut error = CompilerError::new_syntax_error(
                "Unexpected '|' in function body. '|' is valid in function signatures, struct field/type declarations, and loop binding headers.",
                token_stream.current_location(),
            );
            error.new_metadata_entry(
                ErrorMetaDataKey::CompilationStage,
                String::from("AST Construction"),
            );
            error.new_metadata_entry(
                ErrorMetaDataKey::PrimarySuggestion,
                String::from(
                    "Remove the stray '|' or place it in a valid signature or loop header binding list",
                ),
            );
            error
        }

        TokenKind::Must | TokenKind::TraitThis => {
            let keyword = match reserved_trait_keyword_or_dispatch_mismatch(
                token,
                token_stream.current_location(),
                "AST Construction",
                "function-body statement parsing",
            ) {
                Ok(keyword) => keyword,
                Err(error) => return error,
            };

            reserved_trait_keyword_error(
                keyword,
                token_stream.current_location(),
                "AST Construction",
                "Use a normal statement or expression until traits are implemented",
            )
        }

        TokenKind::Arrow => {
            let mut error = CompilerError::new_syntax_error(
                "Unexpected '->' in function body. Arrow syntax is only valid in function signatures.",
                token_stream.current_location(),
            );
            error.new_metadata_entry(
                ErrorMetaDataKey::CompilationStage,
                String::from("AST Construction"),
            );
            error.new_metadata_entry(
                ErrorMetaDataKey::PrimarySuggestion,
                String::from("Use '->' only in a function signature like '|args| -> Type:'"),
            );
            error
        }

        TokenKind::Wildcard => {
            let mut error = CompilerError::new_syntax_error(
                "Unexpected wildcard '_' in function body. Wildcards are not standalone statements.",
                token_stream.current_location(),
            );
            error.new_metadata_entry(
                ErrorMetaDataKey::CompilationStage,
                String::from("AST Construction"),
            );
            error.new_metadata_entry(
                ErrorMetaDataKey::PrimarySuggestion,
                String::from("Use '_' only in supported pattern positions, or use 'else:' for default match arms"),
            );
            error
        }

        TokenKind::Case => {
            let mut error = CompilerError::new_syntax_error(
                "Unexpected 'case' in function body. 'case' arms are only valid inside an 'if <value> is:' match block.",
                token_stream.current_location(),
            );
            error.new_metadata_entry(
                ErrorMetaDataKey::CompilationStage,
                String::from("AST Construction"),
            );
            error.new_metadata_entry(
                ErrorMetaDataKey::PrimarySuggestion,
                String::from(
                    "Wrap these arms in an 'if <value> is:' block or remove the stray 'case'",
                ),
            );
            error
        }

        other => {
            let mut error = CompilerError::new_syntax_error(
                format!("Unexpected token '{other:?}' in a function body."),
                token_stream.current_location(),
            );
            error.new_metadata_entry(
                ErrorMetaDataKey::CompilationStage,
                String::from("AST Construction"),
            );
            error.new_metadata_entry(
                ErrorMetaDataKey::PrimarySuggestion,
                String::from("Use a valid statement such as a declaration, assignment, call, control-flow block, or template"),
            );
            error
        }
    }
}

pub(crate) fn parse_function_body_statements(
    token_stream: &mut FileTokens,
    mut context: ScopeContext,
    warnings: &mut Vec<CompilerWarning>,
    string_table: &mut StringTable,
) -> Result<Vec<AstNode>, CompilerError> {
    let mut ast: Vec<AstNode> =
        Vec::with_capacity(token_stream.length / settings::TOKEN_TO_NODE_RATIO);

    while token_stream.index < token_stream.length {
        let current_token = token_stream.current_token_kind().to_owned();

        ast_log!("Parsing Token: ", #current_token);

        match current_token {
            TokenKind::ModuleStart => {
                token_stream.advance();
            }

            TokenKind::Symbol(_) => parse_symbol_statement(
                token_stream,
                &mut ast,
                &mut context,
                warnings,
                string_table,
            )?,

            TokenKind::Loop => {
                token_stream.advance();

                ast.push(create_loop(
                    token_stream,
                    context.new_child_control_flow(ContextKind::Loop, string_table),
                    warnings,
                    string_table,
                )?);
            }

            TokenKind::If => {
                token_stream.advance();

                ast.extend(create_branch(
                    token_stream,
                    &mut context.new_child_control_flow(ContextKind::Branch, string_table),
                    warnings,
                    string_table,
                )?);
            }

            TokenKind::Else => {
                if context.kind == ContextKind::Branch {
                    break;
                } else {
                    return_rule_error!(
                        "Unexpected use of 'else' keyword. It can only be used inside an if statement or match statement",
                        token_stream.current_location(), {
                            CompilationStage => "AST Construction",
                            PrimarySuggestion => "Remove the 'else' or place it inside an if/match statement",
                        }
                    )
                }
            }

            TokenKind::Newline => {
                token_stream.advance();
            }

            TokenKind::Return => {
                parse_return_statement(token_stream, &mut ast, &context, string_table)?;
            }

            TokenKind::Break => {
                if !context.is_inside_loop() {
                    return_rule_error!(
                        "Break statements can only be used inside loops",
                        token_stream.current_location(),
                        {
                            CompilationStage => "AST Construction",
                            PrimarySuggestion => "Move this break statement inside a loop body",
                        }
                    );
                }

                ast.push(AstNode {
                    kind: NodeKind::Break,
                    location: token_stream.current_location(),
                    scope: context.scope.clone(),
                });
                token_stream.advance();
            }

            TokenKind::Continue => {
                if !context.is_inside_loop() {
                    return_rule_error!(
                        "Continue statements can only be used inside loops",
                        token_stream.current_location(),
                        {
                            CompilationStage => "AST Construction",
                            PrimarySuggestion => "Move this continue statement inside a loop body",
                        }
                    );
                }

                ast.push(AstNode {
                    kind: NodeKind::Continue,
                    location: token_stream.current_location(),
                    scope: context.scope.clone(),
                });
                token_stream.advance();
            }

            TokenKind::End => match context.kind {
                ContextKind::Expression => {
                    return_syntax_error!(
                            "Unexpected scope close. Expressions are not terminated like this.
                            Surround the expression with brackets if you need it to be multi-line. This might just be a compiler_frontend bug.",
                            token_stream.current_location()
                        );
                }
                ContextKind::Template => {
                    return_syntax_error!(
                            "Unexpected use of ';' inside a template. Templates are not closed with ';'.
                            If you are seeing this error, this might be a compiler_frontend bug instead.",
                            token_stream.current_location()
                        )
                }
                _ => {
                    token_stream.advance();
                    break;
                }
            },

            // Top-level runtime template in the entry start() body.
            // Each template becomes a PushStartRuntimeFragment so the HIR builder can
            // push the evaluated string directly to the runtime fragment list.
            // This replaces the old synthetic VariableDeclaration(#template) protocol.
            TokenKind::TemplateHead => {
                if context.kind != ContextKind::Module {
                    return_rule_error!(
                        "Templates can only be used like this at the top level. Not inside the body of a function",
                        token_stream.current_location()
                    )
                }

                let template = Template::new(token_stream, &context, vec![], string_table)?;
                let expr = Expression::template(template, Ownership::MutableOwned);
                let location = token_stream.current_location();

                ast.push(AstNode {
                    kind: NodeKind::PushStartRuntimeFragment(expr),
                    location,
                    scope: context.scope.clone(),
                })
            }

            TokenKind::Eof => {
                break;
            }

            TokenKind::OpenParenthesis
            | TokenKind::FloatLiteral(_)
            | TokenKind::IntLiteral(_)
            | TokenKind::StringSliceLiteral(_)
            | TokenKind::BoolLiteral(_)
            | TokenKind::CharLiteral(_)
            | TokenKind::Copy
            | TokenKind::Mutable => {
                let expr =
                    parse_expression_statement_candidate(token_stream, &context, string_table)?;

                ast.push(AstNode {
                    kind: NodeKind::Rvalue(expr),
                    location: token_stream.current_location(),
                    scope: context.scope.clone(),
                });
            }

            _ => {
                return Err(unexpected_function_body_token_error(
                    token_stream.current_token_kind(),
                    token_stream,
                ));
            }
        }
    }

    Ok(ast)
}
