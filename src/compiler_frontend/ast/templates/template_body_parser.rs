//! Template body parsing.
//!
//! WHAT: Parses the body section of a template — string tokens, nested child
//! templates, slot definitions, and newlines — in source order.
//!
//! WHY: Separates body token consumption from head parsing and composition,
//! keeping each parsing phase focused and testable.

use crate::compiler_frontend::ast::ScopeContext;
use crate::compiler_frontend::ast::expressions::expression::Expression;
use crate::compiler_frontend::ast::templates::template::{
    CommentDirectiveKind, TemplateAtom, TemplateSegmentOrigin, TemplateType,
};
use crate::compiler_frontend::ast::templates::template_types::{Template, TemplateInheritance};
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::token_scan::consume_balanced_template_region;
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, TokenKind};
use crate::compiler_frontend::value_mode::ValueMode;
use crate::{ast_log, return_syntax_error};

/// Parses the body section of a template, consuming tokens until the closing
/// delimiter or EOF. Nested child templates are recursively parsed.
///
/// `foldable` is set to `false` if a runtime (non-const) child template is
/// encountered.
pub(crate) fn parse_template_body(
    token_stream: &mut FileTokens,
    context: &ScopeContext,
    template: &mut Template,
    direct_child_wrappers: &[Template],
    foldable: &mut bool,
    string_table: &mut StringTable,
) -> Result<(), CompilerError> {
    // The tokenizer only allows for strings, templates or slots inside the template body
    while token_stream.index < token_stream.tokens.len() {
        match &token_stream.current_token_kind() {
            TokenKind::Eof => {
                break;
            }

            TokenKind::TemplateClose => {
                ast_log!("Breaking out of template body. Found a template close.");
                // Need to skip the closer
                token_stream.advance();
                break;
            }

            TokenKind::TemplateHead => {
                // When child templates are suppressed (e.g. `$doc`), brackets are
                // treated as balanced literal text rather than parsed as nested templates.
                if template.style.suppress_child_templates {
                    consume_balanced_brackets_as_literal_text(token_stream, template, string_table);
                    continue;
                }

                parse_nested_template(
                    token_stream,
                    context,
                    template,
                    direct_child_wrappers,
                    foldable,
                    string_table,
                )?;
                continue;
            }

            TokenKind::RawStringLiteral(content) | TokenKind::StringSliceLiteral(content) => {
                template.content.add(Expression::string_slice(
                    *content,
                    token_stream.current_location(),
                    ValueMode::ImmutableOwned,
                ));
            }

            TokenKind::Newline => {
                let newline_id = string_table.intern("\n");
                template.content.add(Expression::string_slice(
                    newline_id,
                    token_stream.current_location(),
                    ValueMode::ImmutableOwned,
                ));
            }

            _ => {
                return_syntax_error!(
                    format!(
                        "Invalid Token Used Inside template body when creating template node. Token: {:?}",
                        token_stream.current_token_kind()
                    ),
                    token_stream.current_location()
                )
            }
        }

        token_stream.advance();
    }

    Ok(())
}

/// Handles a nested `[...]` template token encountered inside a parent body.
/// Recursively parses the child, then either folds it into the parent content
/// or pushes it as a child template expression.
fn parse_nested_template(
    token_stream: &mut FileTokens,
    context: &ScopeContext,
    template: &mut Template,
    direct_child_wrappers: &[Template],
    foldable: &mut bool,
    string_table: &mut StringTable,
) -> Result<(), CompilerError> {
    let nested_inheritance = TemplateInheritance {
        direct_child_wrappers: template.style.child_templates.to_owned(),
    };
    let nested_template = Template::new_nested_template(
        token_stream,
        context,
        nested_inheritance,
        string_table,
        matches!(
            template.kind,
            TemplateType::Comment(CommentDirectiveKind::Doc)
        ),
    )?;

    // Doc comment children are collected separately from template content.
    if matches!(
        template.kind,
        TemplateType::Comment(CommentDirectiveKind::Doc)
    ) {
        template.doc_children.push(nested_template);
        return Ok(());
    }

    match &nested_template.kind {
        TemplateType::String
            if !nested_template.has_unresolved_slots()
                && !has_direct_child_template_outputs(&nested_template) =>
        {
            ast_log!(
                "Found a compile time foldable template inside a template. Folding into a string slice..."
            );

            let mut fold_context = context.new_template_fold_context(
                string_table,
                "nested compile-time template folding in body parser",
            )?;
            let interned_child = nested_template.fold_into_stringid(&mut fold_context)?;

            template.content.atoms.push(
                crate::compiler_frontend::ast::templates::template::TemplateAtom::Content(
                    crate::compiler_frontend::ast::templates::template::TemplateSegment::from_child_template_output(
                        Expression::string_slice(
                            interned_child,
                            token_stream.current_location(),
                            ValueMode::ImmutableOwned,
                        ),
                        TemplateSegmentOrigin::Body,
                        nested_template.clone_for_composition(),
                    ),
                ),
            );

            return Ok(());
        }

        TemplateType::StringFunction => {
            *foldable = false;
        }

        TemplateType::Comment(_) => {
            return Ok(());
        }

        TemplateType::String | TemplateType::SlotInsert(_) => {}
        TemplateType::SlotDefinition(slot_key) => {
            template.content.push_slot_with_child_wrappers(
                slot_key.to_owned(),
                direct_child_wrappers.to_owned(),
                template.style.child_templates.to_owned(),
                template.style.skip_parent_child_wrappers,
            );
            return Ok(());
        }
    }

    let expr = Expression::template(nested_template, ValueMode::ImmutableOwned);
    template.content.add(expr);
    Ok(())
}

/// Consumes a `[...]` bracketed region as literal text when child templates are
/// suppressed (e.g. in `$doc` bodies). Tracks bracket nesting depth so balanced
/// brackets are included in the literal output.
fn consume_balanced_brackets_as_literal_text(
    token_stream: &mut FileTokens,
    template: &mut Template,
    string_table: &mut StringTable,
) {
    // Emit the opening bracket as literal text.
    let open_bracket_id = string_table.intern("[");
    template.content.add(Expression::string_slice(
        open_bracket_id,
        token_stream.current_location(),
        ValueMode::ImmutableOwned,
    ));
    token_stream.advance();

    let _ = consume_balanced_template_region(
        token_stream,
        |token, token_kind| match token_kind {
            TokenKind::TemplateHead => {
                let bracket_id = string_table.intern("[");
                template.content.add(Expression::string_slice(
                    bracket_id,
                    token.location.clone(),
                    ValueMode::ImmutableOwned,
                ));
            }
            TokenKind::TemplateClose => {
                let bracket_id = string_table.intern("]");
                template.content.add(Expression::string_slice(
                    bracket_id,
                    token.location.clone(),
                    ValueMode::ImmutableOwned,
                ));
            }
            TokenKind::RawStringLiteral(content) | TokenKind::StringSliceLiteral(content) => {
                template.content.add(Expression::string_slice(
                    *content,
                    token.location.clone(),
                    ValueMode::ImmutableOwned,
                ));
            }
            TokenKind::Newline => {
                let newline_id = string_table.intern("\n");
                template.content.add(Expression::string_slice(
                    newline_id,
                    token.location.clone(),
                    ValueMode::ImmutableOwned,
                ));
            }
            TokenKind::Symbol(id) | TokenKind::StyleDirective(id) => {
                let prefix = if matches!(token_kind, TokenKind::StyleDirective(_)) {
                    "$"
                } else {
                    ""
                };
                let name = string_table.resolve(*id).to_owned();
                let literal = format!("{prefix}{name}");
                let literal_id = string_table.intern(&literal);
                template.content.add(Expression::string_slice(
                    literal_id,
                    token.location.clone(),
                    ValueMode::ImmutableOwned,
                ));
            }
            TokenKind::StartTemplateBody | TokenKind::Colon => {
                let colon_id = string_table.intern(":");
                template.content.add(Expression::string_slice(
                    colon_id,
                    token.location.clone(),
                    ValueMode::ImmutableOwned,
                ));
            }
            TokenKind::Comma => {
                let comma_id = string_table.intern(",");
                template.content.add(Expression::string_slice(
                    comma_id,
                    token.location.clone(),
                    ValueMode::ImmutableOwned,
                ));
            }
            TokenKind::OpenParenthesis => {
                let paren_id = string_table.intern("(");
                template.content.add(Expression::string_slice(
                    paren_id,
                    token.location.clone(),
                    ValueMode::ImmutableOwned,
                ));
            }
            TokenKind::CloseParenthesis => {
                let paren_id = string_table.intern(")");
                template.content.add(Expression::string_slice(
                    paren_id,
                    token.location.clone(),
                    ValueMode::ImmutableOwned,
                ));
            }
            _ => {}
        },
        |_location| (),
    );
}

/// Returns true if the template contains any direct child template output atoms.
/// Folding such templates would merge those individual child outputs into one
/// string slice, losing the structure needed for `$children(..)` wrapper
/// application in slot composition.
fn has_direct_child_template_outputs(template: &Template) -> bool {
    template.content.atoms.iter().any(|atom| match atom {
        TemplateAtom::Content(segment) => segment.is_child_template_output,
        TemplateAtom::Slot(_) => false,
    })
}
