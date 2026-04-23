//! Template node construction orchestrator.
//!
//! WHAT: Provides `Template::new()` — the main entry point for creating a
//! template AST node from a token stream. Delegates to focused submodules
//! for head parsing, body parsing, composition, formatting, and folding.
//!
//! WHY: This file used to contain ALL template logic (~1700 lines). It has
//! been refactored into an orchestrator that coordinates the pipeline stages
//! defined in sibling modules while keeping the overall flow readable.
//!
//! ## Runtime metadata ownership
//!
//! `Template::new()` is the authoritative owner of final runtime template metadata.
//! It builds the render plan and sets `content_needs_formatting = false` before
//! returning. AST finalization trusts this and only resyncs metadata when a
//! template's content actually changes during normalization (e.g. a nested
//! compile-time template is folded into a string slice).

use crate::compiler_frontend::ast::ScopeContext;
use crate::compiler_frontend::ast::templates::template::{
    CommentDirectiveKind, TemplateAtom, TemplateSegmentOrigin, TemplateType,
};
use crate::compiler_frontend::ast::templates::template_body_parser::parse_template_body;
use crate::compiler_frontend::ast::templates::template_composition::compose_template_head_chain;
use crate::compiler_frontend::ast::templates::template_formatting::{
    BodyFormattingResult, apply_body_formatter,
};
use crate::compiler_frontend::ast::templates::template_head_parser::{
    apply_doc_comment_defaults, parse_template_head,
};
use crate::compiler_frontend::ast::templates::template_render_plan::TemplateRenderPlan;
use crate::compiler_frontend::ast::templates::template_slots::ensure_no_slot_insertions_remain;
use crate::compiler_frontend::ast::templates::template_types::Template;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::FileTokens;
use crate::return_syntax_error;

pub(crate) use crate::compiler_frontend::ast::templates::template_types::TemplateInheritance;

// Re-export composition functions used by slots.rs and other consumers.
pub(crate) use crate::compiler_frontend::ast::templates::template_composition::apply_inherited_child_templates_to_content;

impl Template {
    /// Creates a new template node by parsing the token stream.
    ///
    /// This is the main public entry point. It delegates to:
    /// 1. `parse_template_head` — head directives, expressions, style config
    /// 2. `parse_template_body` — body string tokens, nested templates, slots
    /// 3. Composition — child wrapper application, head-chain resolution
    /// 4. Formatting — style-directed body formatting
    /// 5. Validation — directive-owned warnings and slot insertion checks
    pub fn new(
        token_stream: &mut FileTokens,
        context: &ScopeContext,
        templates_inherited: Vec<Template>,
        string_table: &mut StringTable,
    ) -> Result<Template, CompilerError> {
        let inheritance = TemplateInheritance::from_parent_wrappers(templates_inherited);
        Self::new_with_doc_context(token_stream, context, inheritance, string_table, false)
    }

    /// Internal constructor that supports doc comment context propagation.
    /// Called recursively for nested templates in the body parser.
    pub(crate) fn new_with_doc_context(
        token_stream: &mut FileTokens,
        context: &ScopeContext,
        inheritance: TemplateInheritance,
        string_table: &mut StringTable,
        doc_context: bool,
    ) -> Result<Template, CompilerError> {
        let direct_child_wrappers = inheritance.direct_child_wrappers.to_owned();
        // These are variables or special keywords passed into the template head.
        // Nested templates do not inherit formatter/style state by default.
        let mut template = Self::create_default(vec![]);
        // Capture the opening token location early so style/directive errors can
        // still point at the template even if parsing later advances deeply.
        template.location = token_stream.current_location();

        // Templates that call any functions or have children that call functions
        // Can't be folded at compile time (EVENTUALLY CAN FOLD THE CONST FUNCTIONS TOO).
        // This is because the template might be changing at runtime.
        // If the entire template can be folded, it just becomes a string after the AST stage.
        let mut foldable = true;

        // Stage 1: Parse the template head (directives, expressions, style config)
        parse_template_head(
            token_stream,
            context,
            &mut template,
            &mut foldable,
            string_table,
        )?;

        if doc_context {
            apply_doc_comment_defaults(&mut template);
        }

        // Stage 2: Parse the template body (strings, nested templates, slots)
        parse_template_body(
            token_stream,
            context,
            &mut template,
            &direct_child_wrappers,
            &mut foldable,
            string_table,
        )?;
        let requires_post_format_recomposition =
            template_requires_post_format_recomposition(&template);

        // Stage 3: build the composed pre-format snapshot.
        build_unformatted_template_content(
            &mut template,
            &mut foldable,
            string_table,
            requires_post_format_recomposition,
        )?;

        // Stage 4: format body-origin text and produce a structured render plan.
        let BodyFormattingResult {
            plan: render_plan,
            content_changed,
            ..
        } = format_template_body(&template, context, string_table)?;

        // Stage 5: rebuild formatted content and re-run composition.
        finalize_template_after_formatting(
            &mut template,
            render_plan,
            content_changed,
            &mut foldable,
            string_table,
            requires_post_format_recomposition,
        )?;

        template.content_needs_formatting = false;

        // Stage 6: Post-parse validation
        if matches!(
            template.kind,
            TemplateType::Comment(CommentDirectiveKind::Doc)
        ) && !template.content.is_const_evaluable_value()
        {
            return_syntax_error!(
                "'$doc' comments can only contain compile-time values.",
                template.location,
                {
                    PrimarySuggestion => "Use constants and foldable template/string values inside '$doc' comments",
                }
            );
        }

        // `$insert(...)` helpers are allowed to survive while a template still has
        // unresolved `$slot` markers, because that template may later compose into
        // an immediate parent and contribute upward. Once a template has no slots
        // left, any remaining `$insert(...)` is out of scope and must error.
        if !matches!(template.kind, TemplateType::SlotInsert(_)) && !template.has_unresolved_slots()
        {
            ensure_no_slot_insertions_remain(&template.content, &template.location, string_table)?;
        }

        if foldable
            && !matches!(
                template.kind,
                TemplateType::SlotInsert(_)
                    | TemplateType::SlotDefinition(_)
                    | TemplateType::Comment(_)
            )
        {
            template.kind = TemplateType::String;
        }

        Ok(template)
    }
}

/// Stage 3 helper: compute the composed pre-format content snapshot.
///
/// WHAT:
/// - Applies inherited `$children(..)` wrappers and head-chain composition to the parsed
///   content and stores the result in `template.unformatted_content`.
///
/// WHY:
/// - `template.unformatted_content` is the authoritative pre-formatting structure used
///   when later stages need the original composed shape (for example, debugging or
///   future reformat/recomposition workflows).
fn build_unformatted_template_content(
    template: &mut Template,
    foldable: &mut bool,
    string_table: &StringTable,
    requires_post_format_recomposition: bool,
) -> Result<(), CompilerError> {
    if !requires_post_format_recomposition {
        template.unformatted_content = template.content.to_owned();
        return Ok(());
    }

    template.unformatted_content = apply_inherited_child_templates_to_content(
        template.content.clone(),
        &template.style.child_templates,
        string_table,
    )?;
    template.unformatted_content =
        compose_template_head_chain(&template.unformatted_content, foldable, string_table)?;
    Ok(())
}

/// Stage 4 helper: run template formatting and return the produced render plan.
///
/// WHAT:
/// - Runs formatter/whitespace passes over body-origin text only.
/// - Emits formatter warnings via the shared AST context.
///
/// WHY:
/// - `template.content` still contains parsed source segments at this point, and the
///   frontend formatting pipeline is responsible for building the structured render plan
///   that drives post-format reconstruction.
fn format_template_body(
    template: &Template,
    context: &ScopeContext,
    string_table: &mut StringTable,
) -> Result<BodyFormattingResult, CompilerError> {
    let formatting_result =
        match apply_body_formatter(&template.content, &template.style, string_table) {
            Ok(result) => result,
            Err(messages) => {
                for warning in messages.warnings {
                    context.emit_warning(warning);
                }

                return Err(messages.errors.into_iter().next().unwrap_or_else(|| {
                    CompilerError::compiler_error(
                        "Template formatter failed without returning a compiler error.",
                    )
                }));
            }
        };

    for warning in &formatting_result.warnings {
        context.emit_warning(warning.clone());
    }

    Ok(formatting_result)
}

/// Stage 5 helper: rebuild formatted content and finalize the template outputs.
///
/// WHAT:
/// - Rebuilds `template.content` from the formatter render plan.
/// - Re-applies wrapper/head composition to account for slot/child-template composition
///   that can reintroduce content after formatting.
/// - Stores final `template.render_plan` from the finalized content stream.
///
/// WHY:
/// - The pipeline intentionally uses compose -> format -> rebuild -> compose so formatters
///   only rewrite direct body-origin text while structural template composition remains
///   authoritative for the final render order.
fn finalize_template_after_formatting(
    template: &mut Template,
    render_plan: TemplateRenderPlan,
    content_changed: bool,
    foldable: &mut bool,
    string_table: &StringTable,
    requires_post_format_recomposition: bool,
) -> Result<(), CompilerError> {
    // If formatting made no changes and no post-format recomposition is needed,
    // skip the expensive content → plan → content round-trip.
    if content_changed || requires_post_format_recomposition {
        template.content = render_plan.rebuild_content();
        if requires_post_format_recomposition {
            template.content = apply_inherited_child_templates_to_content(
                template.content.clone(),
                &template.style.child_templates,
                string_table,
            )?;
            template.content =
                compose_template_head_chain(&template.content, foldable, string_table)?;
        }
        template.render_plan = Some(TemplateRenderPlan::from_content(&template.content));
    } else {
        // Formatting was a no-op; keep the original content and use the plan directly.
        template.render_plan = Some(render_plan);
    }

    // `template.render_plan` must always match the finalized content stream before HIR sees
    // the template. AST owns both piece ordering and runtime-template planning.
    template.content_needs_formatting = false;
    template.refresh_kind_from_content();
    Ok(())
}

fn template_requires_post_format_recomposition(template: &Template) -> bool {
    if !template.style.child_templates.is_empty() {
        return true;
    }

    template.content.atoms.iter().any(|atom| {
        matches!(
            atom,
            TemplateAtom::Content(segment)
                if segment.origin == TemplateSegmentOrigin::Head
        )
    })
}

#[cfg(test)]
#[path = "tests/create_template_node/mod.rs"]
mod create_template_node_tests;
