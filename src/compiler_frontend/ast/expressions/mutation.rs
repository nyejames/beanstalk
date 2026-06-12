//! Mutation expression parsing for assignment and compound assignment.
//!
//! WHAT: parses `=`, `+=`, `-=`, `*=`, `/=`, and `//=` after a place expression
//!       has been resolved, validating mutability and type compatibility.
//! WHY: mutation is a distinct expression kind in the AST; centralising the
//!      parsing, validation, and compound-operator expansion here keeps the
//!      main expression dispatch logic free of assignment-specific rules.

use crate::ast_log;
use crate::compiler_frontend::ast::ScopeContext;
use crate::compiler_frontend::ast::ast_nodes::{AstNode, Declaration, NodeKind};
use crate::compiler_frontend::ast::expressions::error::ExpressionParseError;
use crate::compiler_frontend::ast::expressions::eval_expression::evaluate_expression;
use crate::compiler_frontend::ast::expressions::expression::{Expression, Operator};
use crate::compiler_frontend::ast::expressions::parse_expression::{
    create_expression, create_expression_with_cast_target,
};
use crate::compiler_frontend::ast::field_access::parse_field_access;
use crate::compiler_frontend::ast::place_access::{ast_node_is_mutable_place, ast_node_is_place};
use crate::compiler_frontend::ast::statements::value_production::receiver::try_parse_value_block_at_receiver;
use crate::compiler_frontend::ast::statements::value_production::types::ValueReceiverKind;
use crate::compiler_frontend::ast::type_interner::AstTypeInterner;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::compiler_messages::{
    CompilerDiagnostic, InvalidAssignmentTargetReason, InvalidResultHandlingReason,
    TypeMismatchContext,
};
use crate::compiler_frontend::datatypes::environment::TypeEnvironment;
use crate::compiler_frontend::datatypes::ids::TypeId;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, TokenKind};
use crate::compiler_frontend::type_coercion::compatibility::is_declaration_compatible;
use crate::compiler_frontend::type_coercion::contextual::coerce_expression_to_declared_type;
use crate::compiler_frontend::type_coercion::parse_context::{
    ExpectedType, cast_target_context_for_type_id, parse_expectation_for_type_id,
};

/// Extract the canonical `TypeId` of an assignment target.
///
/// WHAT: resolves the semantic type of the place expression being assigned to.
/// WHY: type checking for both simple and compound assignments needs the target
///      type to validate the RHS and to build coercion nodes.
fn assignment_target_value_type(target: &AstNode) -> Result<TypeId, CompilerError> {
    target.expression_type_id()
}

/// Check that an expression's type is compatible with the assignment target.
///
/// WHAT: compares the RHS expression type against the target type using the
///       declaration-compatibility relation.
/// WHY: assignments are rejected when the value type cannot be coerced to the
///      target type; this helper emits the appropriate diagnostic.
///
/// Note: `_target`, `_assignment_operator`, and `_string_table` are reserved
/// in the signature for future diagnostic enrichment and to keep call sites
/// consistent with similar validation helpers in the expression module.
fn validate_assignment_value_type(
    expected_type_id: TypeId,
    actual_value: &Expression,
    _target: &AstNode,
    _assignment_operator: &str,
    _string_table: &StringTable,
    type_environment: &TypeEnvironment,
) -> Result<(), ExpressionParseError> {
    if is_declaration_compatible(expected_type_id, actual_value.type_id, type_environment) {
        return Ok(());
    }

    Err(CompilerDiagnostic::type_mismatch(
        expected_type_id,
        actual_value.type_id,
        TypeMismatchContext::Assignment,
        actual_value.location.clone(),
    )
    .into())
}

/// Map a compound-assignment token to its arithmetic operator and diagnostic label.
///
/// WHAT: converts `+=`, `-=`, `*=`, `/=`, `//=` into the corresponding
///       `Operator` variant and a human-readable label used in diagnostics.
/// WHY: compound assignments are desugared into `target = target op rhs`;
///      this mapping is needed both for the desugaring and for error messages.
fn compound_assignment_operator(token_kind: &TokenKind) -> Option<(Operator, &'static str)> {
    match token_kind {
        TokenKind::AddAssign => Some((Operator::Add, "Compound assignment '+='")),
        TokenKind::SubtractAssign => Some((Operator::Subtract, "Compound assignment '-='")),
        TokenKind::MultiplyAssign => Some((Operator::Multiply, "Compound assignment '*='")),
        TokenKind::DivideAssign => Some((Operator::Divide, "Compound assignment '/='")),
        TokenKind::IntDivideAssign => Some((Operator::IntDivide, "Compound assignment '//='")),
        _ => None,
    }
}

/// Input bundle for `evaluate_compound_assignment_value` to avoid a long parameter list.
///
/// WHAT: carries the place target, its resolved type, and the operator that
///       will be used to build the desugared RHS expression.
/// WHY: compound assignment evaluation needs the same state as simple
///      assignment plus the operator; bundling keeps the signature readable.
struct CompoundAssignmentInput<'a> {
    variable_declaration: &'a Declaration,
    target: &'a AstNode,
    target_type_id: TypeId,
    operator: Operator,
    label: &'static str,
}

/// Build the RHS value for a compound assignment by evaluating `target op rhs`.
///
/// WHAT: parses the RHS expression, then evaluates `target op rhs` through the
///       normal expression evaluator so that type checking and constant folding
///       apply to the desugared arithmetic.
/// WHY: compound assignments must behave exactly as the equivalent binary
///      expression for type rules and for constant propagation.
fn evaluate_compound_assignment_value(
    token_stream: &mut FileTokens,
    context: &ScopeContext,
    input: CompoundAssignmentInput<'_>,
    type_interner: &mut AstTypeInterner<'_>,
    string_table: &mut StringTable,
) -> Result<Expression, ExpressionParseError> {
    let CompoundAssignmentInput {
        variable_declaration,
        target,
        target_type_id,
        operator,
        label,
    } = input;
    let location = target.location.clone();
    let mut expr_type = ExpectedType::Infer;
    let rhs_context = variable_declaration
        .id
        .name()
        .map(|target_name| context.with_pending_catch_assignment_targets(&[target_name]))
        .unwrap_or_else(|| context.clone());
    let rhs = create_expression(
        token_stream,
        &rhs_context,
        type_interner,
        &mut expr_type,
        &variable_declaration.value.value_mode,
        false,
        string_table,
    )?;

    let rhs_node = AstNode {
        kind: NodeKind::Rvalue(rhs),
        location: location.clone(),
        scope: context.scope.clone(),
    };
    let operator_node = AstNode {
        kind: NodeKind::Operator(operator),
        location,
        scope: context.scope.clone(),
    };
    let mut inferred = ExpectedType::Infer;
    let value = evaluate_expression(
        context,
        vec![target.clone(), rhs_node, operator_node],
        type_interner,
        &mut inferred,
        &variable_declaration.value.value_mode,
        string_table,
    )?;

    validate_assignment_value_type(
        target_type_id,
        &value,
        target,
        label,
        string_table,
        type_interner.environment(),
    )?;

    Ok(value)
}

/// Parse and validate a mutation given an already-resolved place target.
///
/// WHAT: checks that `target` is a mutable place, then parses the assignment
///       operator and RHS, validates type compatibility, and returns an
///       `Assignment` AST node.
/// WHY: this is the core mutation logic shared by `handle_mutation` (which
///      parses field access first) and `handle_mutation_target` (which receives
///      an already-built target node from the caller).
fn build_mutation_from_target(
    token_stream: &mut FileTokens,
    variable_declaration: &Declaration,
    target: AstNode,
    context: &ScopeContext,
    type_interner: &mut AstTypeInterner<'_>,
    string_table: &mut StringTable,
) -> Result<AstNode, ExpressionParseError> {
    let location = token_stream.current_location();
    let target_type_id = assignment_target_value_type(&target)?;

    ast_log!(
        "Handling mutation for ",
        #variable_declaration.value.value_mode, " ",
        Blue variable_declaration.id.to_string(string_table)
    );

    if !ast_node_is_place(&target) {
        return Err(CompilerDiagnostic::invalid_assignment_target(
            InvalidAssignmentTargetReason::NotMutablePlace,
            None,
            Some(target_type_id),
            location,
        )
        .into());
    }

    if !ast_node_is_mutable_place(&target) {
        return Err(CompilerDiagnostic::invalid_assignment_target(
            InvalidAssignmentTargetReason::ImmutableVariable,
            variable_declaration.id.name(),
            Some(target_type_id),
            location,
        )
        .into());
    }

    let value = match token_stream.current_token_kind() {
        TokenKind::Assign => {
            // Simple mutation: variable = new_value. Parse-time context is
            // preserved only for context-sensitive literals. Compound
            // assignments below use Inferred because that context does not
            // apply to arithmetic operators.
            token_stream.advance();

            let mut expr_type =
                parse_expectation_for_type_id(target_type_id, type_interner.environment());
            let mut cast_target_context = cast_target_context_for_type_id(
                target_type_id,
                type_interner.environment(),
                string_table,
            );
            let rhs_context = variable_declaration
                .id
                .name()
                .map(|target_name| context.with_pending_catch_assignment_targets(&[target_name]))
                .unwrap_or_else(|| context.clone());

            let rhs = if let Some(value_block_result) = try_parse_value_block_at_receiver(
                token_stream,
                &rhs_context,
                type_interner,
                &[target_type_id],
                ValueReceiverKind::Assignment,
                string_table,
            ) {
                value_block_result?
            } else {
                create_expression_with_cast_target(
                    token_stream,
                    &rhs_context,
                    type_interner,
                    &mut expr_type,
                    &mut cast_target_context,
                    &variable_declaration.value.value_mode,
                    false,
                    string_table,
                )?
            };

            // Direct option fallback is rejected at each closed receiver so the
            // later statement parser does not report the trailing `else` as an
            // unrelated branch error.
            token_stream.skip_newlines();
            if token_stream.current_token_kind() == &TokenKind::Else
                && type_interner
                    .environment()
                    .option_inner_type(rhs.type_id)
                    .is_some()
            {
                return Err(CompilerDiagnostic::invalid_result_handling(
                    InvalidResultHandlingReason::DirectOptionFallbackSyntax,
                    token_stream.current_location(),
                )
                .into());
            }

            validate_assignment_value_type(
                target_type_id,
                &rhs,
                &target,
                "Assignment",
                string_table,
                type_interner.environment(),
            )?;

            coerce_expression_to_declared_type(rhs, target_type_id, type_interner.environment())
        }

        compound_token => {
            let Some((operator, label)) = compound_assignment_operator(compound_token) else {
                return Err(CompilerDiagnostic::invalid_assignment_target(
                    InvalidAssignmentTargetReason::ExpectedAssignmentOperator,
                    variable_declaration.id.name(),
                    Some(target_type_id),
                    location,
                )
                .into());
            };

            token_stream.advance();

            evaluate_compound_assignment_value(
                token_stream,
                context,
                CompoundAssignmentInput {
                    variable_declaration,
                    target: &target,
                    target_type_id,
                    operator,
                    label,
                },
                type_interner,
                string_table,
            )?
        }
    };

    Ok(AstNode {
        kind: NodeKind::Assignment {
            target: Box::new(target),
            value,
        },
        location: location.clone(),
        scope: context.scope.clone(),
    })
}

/// Parse a mutation when the place target has already been parsed.
///
/// WHAT: thin wrapper around `build_mutation_from_target` for callers that
///       have already resolved the left-hand side (e.g. after field-access parsing).
/// WHY: keeps the public surface small; callers that already own the target
///      node do not need to re-parse it.
pub(crate) fn handle_mutation_target(
    token_stream: &mut FileTokens,
    variable_declaration: &Declaration,
    target: AstNode,
    context: &ScopeContext,
    type_interner: &mut AstTypeInterner<'_>,
    string_table: &mut StringTable,
) -> Result<AstNode, ExpressionParseError> {
    build_mutation_from_target(
        token_stream,
        variable_declaration,
        target,
        context,
        type_interner,
        string_table,
    )
}

/// Handle mutation of an existing mutable variable.
///
/// WHAT: parses field-access chains on the variable, then builds the mutation
///       node through `build_mutation_from_target`.
/// WHY: this is the entry point used by the statement parser when it sees a
///      variable reference followed by an assignment operator.
pub fn handle_mutation(
    token_stream: &mut FileTokens,
    variable_declaration: &Declaration,
    context: &ScopeContext,
    type_interner: &mut AstTypeInterner<'_>,
    string_table: &mut StringTable,
) -> Result<AstNode, ExpressionParseError> {
    let target = parse_field_access(
        token_stream,
        variable_declaration,
        context,
        type_interner,
        string_table,
    )?;

    build_mutation_from_target(
        token_stream,
        variable_declaration,
        target,
        context,
        type_interner,
        string_table,
    )
}

#[cfg(test)]
#[path = "tests/mutation_tests.rs"]
mod mutation_tests;
