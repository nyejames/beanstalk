//! Symbol-led function-body statement parsing.
//!
//! WHAT: parses statement forms that start with a symbol inside function/start-function bodies.
//! WHY: symbol-led statements are the densest statement branch (mutation, calls, declarations,
//! access chains, and start-import callability), so isolating them keeps dispatch readable.

use crate::compiler_frontend::ast::ScopeContext;
use crate::compiler_frontend::ast::ast_nodes::{AstNode, NodeKind};
use crate::compiler_frontend::ast::expressions::expression::ExpressionKind;
use crate::compiler_frontend::ast::expressions::function_calls::{
    ExternalFunctionCallParseInput, parse_external_function_call,
};
use crate::compiler_frontend::ast::expressions::mutation::{
    handle_mutation, handle_mutation_target,
};
use crate::compiler_frontend::ast::field_access::parse_field_access;
use crate::compiler_frontend::ast::receiver_methods::free_function_receiver_method_call_error;
use crate::compiler_frontend::ast::statements::body_expr_stmt::parse_symbol_expression_statement_candidate;
use crate::compiler_frontend::ast::statements::declarations::new_declaration;
use crate::compiler_frontend::ast::statements::multi_bind::parse_multi_bind_statement;
use crate::compiler_frontend::ast::type_interner::AstTypeInterner;
use crate::compiler_frontend::builtins::error_type::is_reserved_builtin_symbol;
use crate::compiler_frontend::compiler_messages::{
    CompilerDiagnostic, InvalidAssignmentTargetReason, InvalidDeclarationReason,
    InvalidStandaloneStatementReason, InvalidThisUsageReason, ReservedNameOwner,
};
use crate::compiler_frontend::symbols::string_interning::{StringId, StringTable};
use crate::compiler_frontend::syntax_errors::statement_position::check_mistaken_keyword_symbol;
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, TokenKind};

// --------------------------
//  Accessed-symbol statement helper
// --------------------------

#[allow(clippy::result_large_err)]
fn push_accessed_symbol_statement(
    accessed_node: AstNode,
    ast: &mut Vec<AstNode>,
    context: &ScopeContext,
    token_stream: &FileTokens,
    _symbol_id: StringId,
    _string_table: &StringTable,
) -> Result<(), CompilerDiagnostic> {
    match accessed_node.kind {
        // Method calls and collection builtins are valid as standalone statements
        // because they may have side effects.
        NodeKind::MethodCall { .. }
        | NodeKind::CollectionBuiltinCall { .. }
        | NodeKind::MapBuiltinCall { .. } => {
            ast.push(AstNode {
                kind: NodeKind::Rvalue(accessed_node.get_expr()?),
                location: accessed_node.location,
                scope: context.scope.clone(),
            });
            Ok(())
        }

        // A bare field read (e.g., `obj.field`) does nothing, so it is rejected.
        NodeKind::FieldAccess { .. } => Err(CompilerDiagnostic::invalid_standalone_statement(
            InvalidStandaloneStatementReason::FieldRead,
            token_stream.current_location(),
        )),

        // Any other accessed expression is also not a valid standalone statement.
        _ => Err(CompilerDiagnostic::invalid_standalone_statement(
            InvalidStandaloneStatementReason::Expression,
            token_stream.current_location(),
        )),
    }
}

// --------------------------
//  `this` statement parsing
// --------------------------

#[allow(clippy::result_large_err)]
pub(crate) fn parse_this_statement(
    token_stream: &mut FileTokens,
    ast: &mut Vec<AstNode>,
    context: &mut ScopeContext,
    type_interner: &mut AstTypeInterner<'_>,
    string_table: &mut StringTable,
) -> Result<(), CompilerDiagnostic> {
    let this_id = string_table.intern("this");

    // `this` cannot be assigned when we are recovering inside a catch block.
    if context.is_assignment_target_unavailable(this_id) {
        return Err(CompilerDiagnostic::invalid_assignment_target(
            InvalidAssignmentTargetReason::UnavailableInCatchRecovery,
            Some(this_id),
            None,
            token_stream.current_location(),
        ));
    }

    let Some(this_reference) = context.get_reference(&this_id) else {
        return Err(CompilerDiagnostic::invalid_this_usage(
            InvalidThisUsageReason::NotInReceiverMethod,
            token_stream.current_location(),
        ));
    };

    match token_stream.peek_next_token() {
        // Direct reassignment of `this` is never allowed.
        Some(next_token) if next_token.is_assignment_operator() => {
            Err(CompilerDiagnostic::invalid_this_usage(
                InvalidThisUsageReason::Reassignment,
                token_stream.current_location(),
            ))
        }

        // Field access on `this`: may be a mutation (`this.x = ...`) or a
        // method/collection call (`this.x()`).
        Some(TokenKind::Dot) => {
            token_stream.advance();
            let accessed_node = parse_field_access(
                token_stream,
                this_reference,
                context,
                type_interner,
                string_table,
            )
            .map_err(CompilerDiagnostic::from)?;

            if token_stream.current_token_kind().is_assignment_operator() {
                ast.push(handle_mutation_target(
                    token_stream,
                    this_reference,
                    accessed_node,
                    context,
                    type_interner,
                    string_table,
                )?);
                return Ok(());
            }

            push_accessed_symbol_statement(
                accessed_node,
                ast,
                context,
                token_stream,
                this_id,
                string_table,
            )?;
            Ok(())
        }

        // Bare `this` or `this` used as the start of a general expression.
        _ => {
            let expression = parse_symbol_expression_statement_candidate(
                token_stream,
                context,
                this_id,
                type_interner,
                string_table,
            )?;

            ast.push(AstNode {
                kind: NodeKind::Rvalue(expression),
                location: token_stream.current_location(),
                scope: context.scope.clone(),
            });
            Ok(())
        }
    }
}

// --------------------------
//  Symbol-led statement parsing
// --------------------------

#[allow(clippy::result_large_err)]
pub(crate) fn parse_symbol_statement(
    token_stream: &mut FileTokens,
    ast: &mut Vec<AstNode>,
    context: &mut ScopeContext,
    type_interner: &mut AstTypeInterner<'_>,
    warnings: &mut Vec<CompilerDiagnostic>,
    string_table: &mut StringTable,
) -> Result<(), CompilerDiagnostic> {
    let TokenKind::Symbol(symbol_id) = token_stream.current_token_kind().to_owned() else {
        return Err(CompilerDiagnostic::expected_symbol_statement(
            token_stream.current_location(),
        ));
    };

    // Reject symbols that look like keywords in statement position.
    if let Some(error) = check_mistaken_keyword_symbol(symbol_id, token_stream, string_table) {
        return Err(error);
    }

    // Built-in type names cannot be used as value-level symbols.
    if is_reserved_builtin_symbol(string_table.resolve(symbol_id)) {
        return Err(CompilerDiagnostic::reserved_name_collision(
            symbol_id,
            ReservedNameOwner::BuiltinType,
            token_stream.current_location(),
        ));
    }

    // Assignment targets are forbidden while recovering inside a catch block.
    if context.is_assignment_target_unavailable(symbol_id) {
        return Err(CompilerDiagnostic::invalid_assignment_target(
            InvalidAssignmentTargetReason::UnavailableInCatchRecovery,
            Some(symbol_id),
            None,
            token_stream.current_location(),
        ));
    }

    // Multi-bind syntax (`a, b = ...`) takes priority over single-symbol dispatch.
    if let Some(multi_bind_node) =
        parse_multi_bind_statement(token_stream, context, type_interner, string_table)?
    {
        ast.push(multi_bind_node);
        return Ok(());
    }

    // If the symbol already names a visible local, treat it as a use
    // (assignment, field access, or expression) rather than a new declaration.
    if let Some(existing_reference) = context.get_reference(&symbol_id) {
        match token_stream.peek_next_token() {
            // Direct reassignment of an existing local variable.
            Some(next_token) if next_token.is_assignment_operator() => {
                token_stream.advance();
                ast.push(handle_mutation(
                    token_stream,
                    existing_reference,
                    context,
                    type_interner,
                    string_table,
                )?);
                return Ok(());
            }

            // Field access on an existing local: may be a mutation or a call.
            Some(TokenKind::Dot) => {
                token_stream.advance();
                let accessed_node = parse_field_access(
                    token_stream,
                    existing_reference,
                    context,
                    type_interner,
                    string_table,
                )
                .map_err(CompilerDiagnostic::from)?;

                if token_stream.current_token_kind().is_assignment_operator() {
                    ast.push(handle_mutation_target(
                        token_stream,
                        existing_reference,
                        accessed_node,
                        context,
                        type_interner,
                        string_table,
                    )?);
                    return Ok(());
                }

                push_accessed_symbol_statement(
                    accessed_node,
                    ast,
                    context,
                    token_stream,
                    symbol_id,
                    string_table,
                )?;
                return Ok(());
            }

            // A type keyword after an existing symbol means the user is trying to
            // redeclare it with an explicit type, which is a shadowing error.
            Some(TokenKind::DatatypeInt)
            | Some(TokenKind::DatatypeFloat)
            | Some(TokenKind::DatatypeBool)
            | Some(TokenKind::DatatypeString)
            | Some(TokenKind::DatatypeChar)
            | Some(TokenKind::Mutable) => {
                return Err(CompilerDiagnostic::shadowed_name(
                    symbol_id,
                    existing_reference.value.location.clone(),
                    token_stream.current_location(),
                ));
            }

            // Otherwise, parse the symbol as the start of a general expression statement.
            _ => {
                let expression = parse_symbol_expression_statement_candidate(
                    token_stream,
                    context,
                    symbol_id,
                    type_interner,
                    string_table,
                )?;

                ast.push(AstNode {
                    kind: NodeKind::Rvalue(expression),
                    location: token_stream.current_location(),
                    scope: context.scope.clone(),
                });
                return Ok(());
            }
        }
    }

    // External (host) function calls have no local declaration; resolve by name.
    if let Some((external_function_id, external_function_def)) =
        context.lookup_visible_external_function(symbol_id)
    {
        if token_stream.peek_next_token() == Some(&TokenKind::TypeParameterBracket) {
            let previous_location = context
                .lookup_visible_external_function_location(symbol_id)
                .unwrap_or_default();
            return Err(CompilerDiagnostic::duplicate_declaration(
                symbol_id,
                previous_location,
                token_stream.current_location(),
            ));
        }

        token_stream.advance();
        let external_call_node = parse_external_function_call(ExternalFunctionCallParseInput {
            token_stream,
            external_function_id,
            external_function: external_function_def,
            context,
            value_required: false,
            allow_boundary_catch: true,
            warnings: Some(warnings),
            type_interner,
            string_table,
        })
        .map_err(CompilerDiagnostic::from)?;
        ast.push(external_call_node);
        return Ok(());
    }

    // An open parenthesis after an unknown symbol means a call attempt.
    // Provide targeted diagnostics for receiver methods and external types.
    if token_stream.peek_next_token() == Some(&TokenKind::OpenParenthesis) {
        if let Some(receiver_method_entry) =
            context.lookup_visible_receiver_method_by_name(symbol_id)
        {
            return Err(free_function_receiver_method_call_error(
                symbol_id,
                receiver_method_entry,
                token_stream.current_location(),
                string_table,
            ));
        }

        if context.lookup_visible_external_type(symbol_id).is_some() {
            return Err(CompilerDiagnostic::invalid_declaration(
                InvalidDeclarationReason::ExternalTypeLiteralConstruction,
                Some(symbol_id),
                token_stream.current_location(),
            ));
        }

        return Err(CompilerDiagnostic::unknown_value_name(
            symbol_id,
            token_stream.current_location(),
        ));
    }

    // No existing reference and no call target: this must be a new declaration.
    let declaration = new_declaration(
        token_stream,
        symbol_id,
        context,
        type_interner,
        warnings,
        string_table,
    )?;

    // Lift struct definitions and functions to the AST statement level;
    // everything else becomes a local variable declaration.
    match declaration.value.kind {
        ExpressionKind::StructDefinition(ref params) => {
            ast.push(AstNode {
                kind: NodeKind::StructDefinition(declaration.id.to_owned(), params.to_owned()),
                location: token_stream.current_location(),
                scope: context.scope.clone(),
            });
        }

        ExpressionKind::Function(ref signature, ref body) => {
            ast.push(AstNode {
                kind: NodeKind::Function(
                    declaration.id.to_owned(),
                    signature.to_owned(),
                    body.to_owned(),
                ),
                location: token_stream.current_location(),
                scope: context.scope.clone(),
            });
        }

        _ => {
            ast.push(AstNode {
                kind: NodeKind::VariableDeclaration(declaration.to_owned()),
                location: token_stream.current_location(),
                scope: context.scope.clone(),
            });
        }
    }

    context.add_var(declaration);
    Ok(())
}
