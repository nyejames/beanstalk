//! Multi-bind parsing and target resolution helpers.
//!
//! WHAT: parses `a, b = value` style statements and resolves each target into the AST shape HIR
//! will lower later.
//! WHY: multi-bind syntax mixes statement parsing, declaration rules, and target validation, so it
//! deserves a dedicated module instead of living inside the general function-body dispatcher.

use crate::compiler_frontend::ast::ast::ScopeContext;
use crate::compiler_frontend::ast::ast_nodes::{
    AstNode, Declaration, MultiBindTarget, MultiBindTargetKind, NodeKind,
};
use crate::compiler_frontend::ast::expressions::expression::Expression;
use crate::compiler_frontend::ast::expressions::parse_expression::create_expression;
use crate::compiler_frontend::ast::statements::declaration_syntax::{
    BindingTargetSyntax, parse_binding_target_syntax,
};
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::datatypes::{DataType, Ownership};
use crate::compiler_frontend::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, TokenKind};
use crate::compiler_frontend::traits::ContainsReferences;
use crate::{return_rule_error, return_syntax_error, return_type_error};
use std::collections::HashSet;

struct ResolvedMultiBindTargets {
    targets: Vec<MultiBindTarget>,
    new_declarations: Vec<Declaration>,
}

pub(crate) fn parse_multi_bind_statement(
    token_stream: &mut FileTokens,
    context: &mut ScopeContext,
    string_table: &mut StringTable,
) -> Result<Option<AstNode>, CompilerError> {
    if !statement_has_top_level_comma(token_stream) {
        return Ok(None);
    }

    let Some(parsed_targets) = parse_target_list(token_stream, string_table)? else {
        return Ok(None);
    };

    validate_unique_target_names(&parsed_targets, string_table)?;
    let rhs_expression = parse_multi_bind_rhs_expression(token_stream, context, string_table)?;
    let rhs_slots = extract_rhs_slot_types(&rhs_expression)?;

    if rhs_slots.len() != parsed_targets.len() {
        return_type_error!(
            format!(
                "Multi-bind arity mismatch: {} target(s) but {} value slot(s).",
                parsed_targets.len(),
                rhs_slots.len()
            ),
            rhs_expression.location.clone(),
            {
                CompilationStage => "AST Construction",
                PrimarySuggestion => "Match the number of targets to the number of returned value slots exactly",
            }
        );
    }

    let resolved_targets =
        resolve_multi_bind_targets(&parsed_targets, &rhs_slots, context, string_table)?;
    register_new_declarations(context, resolved_targets.new_declarations);

    Ok(Some(AstNode {
        kind: NodeKind::MultiBind {
            targets: resolved_targets.targets,
            value: rhs_expression,
        },
        location: token_stream.current_location(),
        scope: context.scope.clone(),
    }))
}

fn statement_has_top_level_comma(token_stream: &FileTokens) -> bool {
    let mut paren_depth = 0usize;
    let mut curly_depth = 0usize;
    let mut template_depth = 0usize;
    let mut index = token_stream.index;

    while index < token_stream.length {
        let token = &token_stream.tokens[index].kind;
        let at_top_level = paren_depth == 0 && curly_depth == 0 && template_depth == 0;

        if at_top_level && matches!(token, TokenKind::Newline | TokenKind::End | TokenKind::Eof) {
            break;
        }

        if at_top_level && matches!(token, TokenKind::Comma) {
            return true;
        }

        match token {
            TokenKind::OpenParenthesis => paren_depth += 1,
            TokenKind::CloseParenthesis => paren_depth = paren_depth.saturating_sub(1),
            TokenKind::OpenCurly => curly_depth += 1,
            TokenKind::CloseCurly => curly_depth = curly_depth.saturating_sub(1),
            TokenKind::TemplateHead => template_depth += 1,
            TokenKind::TemplateClose => template_depth = template_depth.saturating_sub(1),
            _ => {}
        }

        index += 1;
    }

    false
}

fn parse_target_list(
    token_stream: &mut FileTokens,
    string_table: &mut StringTable,
) -> Result<Option<Vec<BindingTargetSyntax>>, CompilerError> {
    let start_index = token_stream.index;
    let mut parsed_targets = Vec::new();
    let mut seen_comma = false;

    loop {
        let TokenKind::Symbol(name) = token_stream.current_token_kind().to_owned() else {
            return_syntax_error!(
                "Malformed multi-bind target list. Expected a symbol target name.",
                token_stream.current_location(),
                {
                    CompilationStage => "AST Construction",
                    PrimarySuggestion => "Use a flat list of symbol targets like 'a, b = value'",
                }
            );
        };

        token_stream.advance();
        let target_syntax = parse_binding_target_syntax(token_stream, name, string_table)?;
        validate_target_mutability(&target_syntax, string_table)?;
        parsed_targets.push(target_syntax);

        match token_stream.current_token_kind() {
            TokenKind::Comma => {
                seen_comma = true;
                token_stream.advance();

                if matches!(
                    token_stream.current_token_kind(),
                    TokenKind::Comma | TokenKind::Assign | TokenKind::Newline | TokenKind::End
                ) {
                    return_syntax_error!(
                        "Malformed multi-bind target list near ','.",
                        token_stream.current_location(),
                        {
                            CompilationStage => "AST Construction",
                            PrimarySuggestion => "Separate targets with a single comma and include one target after each comma",
                        }
                    );
                }
            }
            TokenKind::Assign => break,
            TokenKind::Newline | TokenKind::End | TokenKind::Eof => {
                return_syntax_error!(
                    "Multi-bind target list is missing a shared '=' assignment operator.",
                    token_stream.current_location(),
                    {
                        CompilationStage => "AST Construction",
                        PrimarySuggestion => "Add one '=' after the full target list, for example 'a, b = value'",
                    }
                );
            }
            _ => {
                return_syntax_error!(
                    format!(
                        "Invalid token '{:?}' after multi-bind target.",
                        token_stream.current_token_kind()
                    ),
                    token_stream.current_location(),
                    {
                        CompilationStage => "AST Construction",
                        PrimarySuggestion => "Use ',' between targets and a single '=' before the right-hand expression",
                    }
                );
            }
        }
    }

    if !seen_comma || parsed_targets.len() < 2 {
        token_stream.index = start_index;
        return Ok(None);
    }

    token_stream.advance();
    Ok(Some(parsed_targets))
}

fn validate_target_mutability(
    target_syntax: &BindingTargetSyntax,
    string_table: &StringTable,
) -> Result<(), CompilerError> {
    if target_syntax.mutable_marker && !target_syntax.has_explicit_type() {
        return_rule_error!(
            format!(
                "Mutable multi-bind target '{}' requires an explicit type annotation",
                string_table.resolve(target_syntax.name)
            ),
            target_syntax.location.clone(),
            {
                CompilationStage => "AST Construction",
                PrimarySuggestion => "Write mutable multi-bind targets as '~Type', for example 'value ~Float'",
            }
        );
    }

    Ok(())
}

fn validate_unique_target_names(
    parsed_targets: &[BindingTargetSyntax],
    string_table: &StringTable,
) -> Result<(), CompilerError> {
    let mut seen_names = HashSet::new();
    for target in parsed_targets {
        if !seen_names.insert(target.name) {
            return_rule_error!(
                format!(
                    "Duplicate multi-bind target '{}' in the same target list",
                    string_table.resolve(target.name)
                ),
                target.location.clone(),
                {
                    CompilationStage => "AST Construction",
                    PrimarySuggestion => "Use each target name at most once per multi-bind statement",
                }
            );
        }
    }

    Ok(())
}

fn parse_multi_bind_rhs_expression(
    token_stream: &mut FileTokens,
    context: &ScopeContext,
    string_table: &mut StringTable,
) -> Result<Expression, CompilerError> {
    if matches!(
        token_stream.current_token_kind(),
        TokenKind::Newline | TokenKind::End | TokenKind::Eof
    ) {
        return_syntax_error!(
            "Multi-bind statement is missing a right-hand expression after '='.",
            token_stream.current_location(),
            {
                CompilationStage => "AST Construction",
                PrimarySuggestion => "Provide one expression that returns a multi-value pack",
            }
        );
    }

    let mut inferred_rhs_type = DataType::Inferred;
    let rhs_expression = create_expression(
        token_stream,
        context,
        &mut inferred_rhs_type,
        &Ownership::ImmutableOwned,
        false,
        string_table,
    )?;

    if token_stream.current_token_kind() == &TokenKind::Comma {
        return_syntax_error!(
            "Multi-bind statements accept exactly one right-hand expression.",
            token_stream.current_location(),
            {
                CompilationStage => "AST Construction",
                PrimarySuggestion => "Return multiple values from a single expression instead of writing multiple right-hand expressions",
            }
        );
    }

    Ok(rhs_expression)
}

fn extract_rhs_slot_types(rhs_expression: &Expression) -> Result<Vec<DataType>, CompilerError> {
    match &rhs_expression.data_type {
        DataType::Returns(slots) => Ok(slots.to_owned()),
        _ => {
            return_type_error!(
                "Multi-bind right-hand expression must evaluate to a multi-value return pack.",
                rhs_expression.location.clone(),
                {
                    CompilationStage => "AST Construction",
                    PrimarySuggestion => "Use an expression that returns multiple values, such as a multi-return function call",
                }
            );
        }
    }
}

// WHAT: resolves each parsed binding target into either a fresh declaration target or an existing
// mutable assignment target.
// WHY: multi-bind is intentionally allowed to mix new names with existing mutable locals, and HIR
// needs that distinction preserved instead of rediscovering it later.
fn resolve_multi_bind_targets(
    parsed_targets: &[BindingTargetSyntax],
    rhs_slots: &[DataType],
    context: &ScopeContext,
    string_table: &StringTable,
) -> Result<ResolvedMultiBindTargets, CompilerError> {
    let mut resolved_targets = Vec::with_capacity(parsed_targets.len());
    let mut new_declarations = Vec::new();

    for (slot_type, target_syntax) in rhs_slots.iter().zip(parsed_targets.iter()) {
        let explicit_type = resolve_target_explicit_type(target_syntax, context, string_table)?;
        let Some(existing_declaration) = context.get_reference(&target_syntax.name) else {
            let target_data_type = resolve_new_target_data_type(
                target_syntax,
                explicit_type,
                slot_type,
                string_table,
            )?;
            let target_ownership = binding_target_ownership(target_syntax);
            let target_id = context.scope.append(target_syntax.name);

            new_declarations.push(Declaration {
                id: target_id.to_owned(),
                value: Expression::no_value(
                    target_syntax.location.clone(),
                    target_data_type.to_owned(),
                    target_ownership.to_owned(),
                ),
            });

            resolved_targets.push(MultiBindTarget {
                id: target_id,
                data_type: target_data_type,
                ownership: target_ownership,
                kind: MultiBindTargetKind::Declaration,
                location: target_syntax.location.clone(),
            });
            continue;
        };

        resolved_targets.push(resolve_existing_target(
            target_syntax,
            slot_type,
            explicit_type.as_ref(),
            existing_declaration,
            string_table,
        )?);
    }

    Ok(ResolvedMultiBindTargets {
        targets: resolved_targets,
        new_declarations,
    })
}

fn resolve_existing_target(
    target_syntax: &BindingTargetSyntax,
    slot_type: &DataType,
    explicit_type: Option<&DataType>,
    existing_declaration: &Declaration,
    string_table: &StringTable,
) -> Result<MultiBindTarget, CompilerError> {
    if target_syntax.mutable_marker {
        return_rule_error!(
            format!(
                "Existing multi-bind target '{}' cannot use a mutable marker.",
                string_table.resolve(target_syntax.name)
            ),
            target_syntax.location.clone(),
            {
                CompilationStage => "AST Construction",
                PrimarySuggestion => "Remove '~' from existing targets and keep mutability on the original declaration",
            }
        );
    }

    if !existing_declaration.value.ownership.is_mutable() {
        return_rule_error!(
            format!(
                "Existing multi-bind target '{}' is immutable and cannot be reassigned.",
                string_table.resolve(target_syntax.name)
            ),
            target_syntax.location.clone(),
            {
                CompilationStage => "AST Construction",
                PrimarySuggestion => "Declare the variable as mutable with '~=' before reassigning it",
            }
        );
    }

    if let Some(explicit_type) = explicit_type
        && explicit_type != &existing_declaration.value.data_type
    {
        return_type_error!(
            format!(
                "Explicit type for existing target '{}' does not match the declared variable type.",
                string_table.resolve(target_syntax.name)
            ),
            target_syntax.location.clone(),
            {
                CompilationStage => "AST Construction",
                PrimarySuggestion => "Use the exact declared type for this existing target or remove the explicit type annotation",
            }
        );
    }

    let expected_slot_type = existing_declaration.value.data_type.to_owned();
    if &expected_slot_type != slot_type {
        return_type_error!(
            format!(
                "Type mismatch for target '{}'. Expected '{}', got '{}'.",
                string_table.resolve(target_syntax.name),
                expected_slot_type.display_with_table(string_table),
                slot_type.display_with_table(string_table)
            ),
            target_syntax.location.clone(),
            {
                CompilationStage => "AST Construction",
                PrimarySuggestion => "Ensure each multi-bind slot type matches the existing target type exactly",
            }
        );
    }

    Ok(MultiBindTarget {
        id: existing_declaration.id.to_owned(),
        data_type: expected_slot_type,
        ownership: existing_declaration.value.ownership.to_owned(),
        kind: MultiBindTargetKind::Assignment,
        location: target_syntax.location.clone(),
    })
}

fn resolve_new_target_data_type(
    target_syntax: &BindingTargetSyntax,
    explicit_type: Option<DataType>,
    slot_type: &DataType,
    string_table: &StringTable,
) -> Result<DataType, CompilerError> {
    let inferred_slot_type = slot_type.to_owned();
    let Some(explicit_type) = explicit_type else {
        return Ok(inferred_slot_type);
    };

    if explicit_type != inferred_slot_type {
        return_type_error!(
            format!(
                "Type mismatch for target '{}'. Expected '{}', got '{}'.",
                string_table.resolve(target_syntax.name),
                explicit_type.display_with_table(string_table),
                inferred_slot_type.display_with_table(string_table)
            ),
            target_syntax.location.clone(),
            {
                CompilationStage => "AST Construction",
                PrimarySuggestion => "Update the target type annotation or the right-hand expression slot type so they match exactly",
            }
        );
    }

    Ok(explicit_type)
}

fn binding_target_ownership(target_syntax: &BindingTargetSyntax) -> Ownership {
    if target_syntax.mutable_marker {
        Ownership::MutableOwned
    } else {
        Ownership::ImmutableOwned
    }
}

fn register_new_declarations(context: &mut ScopeContext, new_declarations: Vec<Declaration>) {
    for declaration in new_declarations {
        context.add_var(declaration);
    }
}

fn resolve_target_explicit_type(
    target: &BindingTargetSyntax,
    context: &ScopeContext,
    string_table: &StringTable,
) -> Result<Option<DataType>, CompilerError> {
    if let Some(type_name) = target.explicit_named_type {
        let Some(named_declaration) = context.get_reference(&type_name) else {
            return_rule_error!(
                format!(
                    "Unknown type '{}'. Type names must be declared before use.",
                    string_table.resolve(type_name)
                ),
                target.location.clone(),
                {
                    CompilationStage => "AST Construction",
                    PrimarySuggestion => "Declare this type before using it in a multi-bind target",
                }
            );
        };

        let resolved_named_type = named_declaration.value.data_type.to_owned();
        return Ok(Some(if target.explicit_named_optional {
            DataType::Option(Box::new(resolved_named_type))
        } else {
            resolved_named_type
        }));
    }

    if matches!(target.explicit_type, DataType::Inferred) {
        return Ok(None);
    }

    Ok(Some(target.explicit_type.to_owned()))
}
