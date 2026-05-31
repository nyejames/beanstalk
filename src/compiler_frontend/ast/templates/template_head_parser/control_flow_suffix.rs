//! Template head suffix parsing for `if` and `loop`.
//!
//! WHAT: turns final template-head control-flow suffixes into a structured body
//! parser mode.
//! WHY: the head parser must recognize control flow before body parsing, but
//! branch/body splitting belongs to the body parser in the next phase.

use crate::compiler_frontend::ast::statements::if_headers::{ParsedIfHeader, parse_if_header};
use crate::compiler_frontend::ast::statements::loop_headers::{
    ParsedLoopHeader, parse_loop_header_tokens,
};
use crate::compiler_frontend::ast::templates::template_control_flow::{
    TemplateBodyParseMode, TemplateBranchSelector, TemplateControlFlowValidationMode,
    TemplateIfBodyParseInput, TemplateLoopBodyParseInput, TemplateLoopHeader,
    inline_source_consts_for_const_required_expression,
    inline_source_consts_for_const_required_if_condition,
};
use crate::compiler_frontend::ast::type_interner::AstTypeInterner;
use crate::compiler_frontend::ast::{ContextKind, ScopeContext};
use crate::compiler_frontend::compiler_messages::{
    CompilerDiagnostic, InvalidTemplateStructureReason,
};
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::token_scan::NestingDepth;
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, Token, TokenKind};

/// Parse a template `if` suffix after the `if` token has been seen.
#[allow(clippy::result_large_err)]
pub(crate) fn parse_if_suffix(
    token_stream: &mut FileTokens,
    context: &ScopeContext,
    type_interner: &mut AstTypeInterner<'_>,
    validation_mode: TemplateControlFlowValidationMode,
    string_table: &mut StringTable,
) -> Result<TemplateBodyParseMode, CompilerDiagnostic> {
    let location = token_stream.current_location();
    token_stream.advance(); // consume `if`

    if next_meaningful_token_is_body_boundary(token_stream) {
        return Err(CompilerDiagnostic::invalid_template_structure(
            InvalidTemplateStructureReason::MissingTemplateIfCondition,
            location,
        ));
    }

    let parsed_header = parse_if_header(token_stream, context, type_interner, string_table)?;

    ensure_suffix_ends_at_body_start(token_stream)?;
    token_stream.advance(); // consume `:`

    let (mut condition, then_context) = match parsed_header {
        ParsedIfHeader::BoolCondition { condition } => {
            let then_context = context.new_child_control_flow(ContextKind::Branch, string_table);
            (TemplateBranchSelector::Bool(condition), then_context)
        }

        ParsedIfHeader::OptionPresentCapture {
            scrutinee,
            pattern,
            then_context,
        } => {
            let then_context =
                then_context.new_child_control_flow(ContextKind::Branch, string_table);
            (
                TemplateBranchSelector::OptionPresentCapture {
                    scrutinee,
                    pattern: Box::new(pattern),
                },
                then_context,
            )
        }

        ParsedIfHeader::MatchStyle { scrutinee } => {
            return Err(CompilerDiagnostic::invalid_template_structure(
                InvalidTemplateStructureReason::TemplateMatchStyleControlFlowUnsupported,
                scrutinee.location,
            ));
        }
    };

    if validation_mode == TemplateControlFlowValidationMode::ConstRequired {
        condition =
            inline_source_consts_for_const_required_if_condition(condition, context, string_table);
    }

    let else_context = context.new_child_control_flow(ContextKind::Branch, string_table);

    Ok(TemplateBodyParseMode::If(Box::new(
        TemplateIfBodyParseInput {
            selector: condition,
            then_context,
            else_context,
            location,
        },
    )))
}

/// Parse a template `loop` suffix after the `loop` token has been seen.
#[allow(clippy::result_large_err)]
pub(crate) fn parse_loop_suffix(
    token_stream: &mut FileTokens,
    context: &ScopeContext,
    type_interner: &mut AstTypeInterner<'_>,
    validation_mode: TemplateControlFlowValidationMode,
    string_table: &mut StringTable,
) -> Result<TemplateBodyParseMode, CompilerDiagnostic> {
    let location = token_stream.current_location();
    token_stream.advance(); // consume `loop`

    if next_meaningful_token_is_body_boundary(token_stream) {
        return Err(CompilerDiagnostic::invalid_template_structure(
            InvalidTemplateStructureReason::MissingTemplateLoopHeader,
            location,
        ));
    }

    let body_start_index = find_template_body_start(token_stream)?;
    let suffix_tokens = &token_stream.tokens[token_stream.index..body_start_index];

    if has_top_level_suffix_separator(suffix_tokens) {
        return Err(CompilerDiagnostic::invalid_template_structure(
            InvalidTemplateStructureReason::ControlFlowSuffixNotFinal,
            location,
        ));
    }

    let mut warnings = Vec::new();
    let (parsed_header, body_context) = parse_loop_header_tokens(
        suffix_tokens,
        context.new_child_control_flow(ContextKind::Loop, string_table),
        type_interner,
        &mut warnings,
        string_table,
    )?;

    for warning in warnings {
        context.emit_warning(warning);
    }

    let header = match parsed_header {
        ParsedLoopHeader::Conditional { mut condition } => {
            if validation_mode == TemplateControlFlowValidationMode::ConstRequired {
                condition = inline_source_consts_for_const_required_expression(
                    condition,
                    context,
                    string_table,
                );
            }

            TemplateLoopHeader::Conditional {
                condition: Box::new(condition),
            }
        }

        ParsedLoopHeader::Range { bindings, range } => TemplateLoopHeader::Range {
            bindings: Box::new(bindings),
            range: Box::new(range),
        },
        ParsedLoopHeader::Collection { bindings, iterable } => TemplateLoopHeader::Collection {
            bindings: Box::new(bindings),
            iterable: Box::new(iterable),
        },
    };

    token_stream.index = body_start_index + 1;

    Ok(TemplateBodyParseMode::Loop(Box::new(
        TemplateLoopBodyParseInput {
            header,
            body_context,
            location,
        },
    )))
}

fn next_meaningful_token_is_body_boundary(token_stream: &FileTokens) -> bool {
    let mut index = token_stream.index;

    while index < token_stream.length {
        match token_stream.tokens[index].kind {
            TokenKind::Newline => index += 1,
            TokenKind::StartTemplateBody | TokenKind::TemplateClose | TokenKind::Eof => {
                return true;
            }
            _ => return false,
        }
    }

    true
}

#[allow(clippy::result_large_err)]
fn ensure_suffix_ends_at_body_start(token_stream: &FileTokens) -> Result<(), CompilerDiagnostic> {
    match token_stream.current_token_kind() {
        TokenKind::StartTemplateBody => Ok(()),
        TokenKind::Comma => Err(CompilerDiagnostic::invalid_template_structure(
            InvalidTemplateStructureReason::ControlFlowSuffixNotFinal,
            token_stream.current_location(),
        )),
        _ => Err(CompilerDiagnostic::invalid_template_structure(
            InvalidTemplateStructureReason::UnexpectedTokenAfterControlFlowSuffix,
            token_stream.current_location(),
        )),
    }
}

#[allow(clippy::result_large_err)]
fn find_template_body_start(token_stream: &FileTokens) -> Result<usize, CompilerDiagnostic> {
    let mut nesting_depth = NestingDepth::default();
    let mut index = token_stream.index;

    while index < token_stream.length {
        let token = &token_stream.tokens[index];
        if nesting_depth.is_top_level() && matches!(token.kind, TokenKind::StartTemplateBody) {
            return Ok(index);
        }

        if nesting_depth.is_top_level()
            && matches!(token.kind, TokenKind::TemplateClose | TokenKind::Eof)
        {
            return Err(CompilerDiagnostic::invalid_template_structure(
                InvalidTemplateStructureReason::UnexpectedTokenAfterControlFlowSuffix,
                token.location.clone(),
            ));
        }

        nesting_depth.step(&token.kind);
        index += 1;
    }

    Err(CompilerDiagnostic::invalid_template_structure(
        InvalidTemplateStructureReason::UnexpectedTokenAfterControlFlowSuffix,
        token_stream.current_location(),
    ))
}

fn has_top_level_suffix_separator(tokens: &[Token]) -> bool {
    let mut nesting_depth = NestingDepth::default();
    let mut pipe_depth = 0usize;

    for token in tokens {
        if nesting_depth.is_top_level() {
            if matches!(token.kind, TokenKind::TypeParameterBracket) {
                pipe_depth = if pipe_depth == 0 { 1 } else { 0 };
            } else if pipe_depth == 0 && matches!(token.kind, TokenKind::Comma) {
                return true;
            }
        }

        nesting_depth.step(&token.kind);
    }

    false
}
