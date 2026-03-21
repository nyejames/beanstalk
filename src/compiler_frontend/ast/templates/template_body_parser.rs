//! Template body parsing.
//!
//! WHAT: Parses the body section of a template — string tokens, nested child
//! templates, slot definitions, and newlines — in source order.
//!
//! WHY: Separates body token consumption from head parsing and composition,
//! keeping each parsing phase focused and testable.

use crate::compiler_frontend::ast::ast::ScopeContext;
use crate::compiler_frontend::ast::expressions::expression::Expression;
use crate::compiler_frontend::ast::templates::template::{
    CommentDirectiveKind, TemplateAtom, TemplateSegmentOrigin, TemplateType,
};
use crate::compiler_frontend::ast::templates::template_composition::{
    effective_inherited_style_for_nested_templates, inherited_style_for_nested_child_templates,
};
use crate::compiler_frontend::ast::templates::template_types::{Template, TemplateInheritance};
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::datatypes::Ownership;
use crate::compiler_frontend::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, TokenKind};
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
                    Ownership::ImmutableOwned,
                ));
            }

            TokenKind::Newline => {
                let newline_id = string_table.intern("\n");
                template.content.add(Expression::string_slice(
                    newline_id,
                    token_stream.current_location(),
                    Ownership::ImmutableOwned,
                ));
            }

            _ => {
                return_syntax_error!(
                    format!(
                        "Invalid Token Used Inside template body when creating template node. Token: {:?}",
                        token_stream.current_token_kind()
                    ),
                    token_stream
                        .current_location()
                        .to_error_location(string_table)
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
        recursive_style: inherited_style_for_nested_child_templates(&template.style),
        direct_child_wrappers: template.style.child_templates.to_owned(),
    };
    let nested_template = Template::new_with_doc_context(
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

            // Preserve formatter boundaries when folding nested compile-time
            // templates into this template's body stream.
            let inherited_style = effective_inherited_style_for_nested_templates(&template.style);

            let interned_child =
                nested_template.fold_into_stringid(&inherited_style, string_table)?;

            template.content.atoms.push(
                crate::compiler_frontend::ast::templates::template::TemplateAtom::Content(
                    crate::compiler_frontend::ast::templates::template::TemplateSegment::from_child_template_output(
                        Expression::string_slice(
                            interned_child,
                            token_stream.current_location(),
                            Ownership::ImmutableOwned,
                        ),
                        TemplateSegmentOrigin::Body,
                        nested_template.clone(),
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
                template.style.clear_inherited,
            );
            return Ok(());
        }
    }

    let expr = Expression::template(nested_template, Ownership::ImmutableOwned);
    template.content.add(expr);
    Ok(())
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
