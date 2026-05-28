//! Template node construction orchestrator.
//!
//! WHAT: Provides `Template::new()` — the main entry point for creating a
//! template AST node from a token stream. Delegates to focused submodules
//! for head parsing, body parsing, composition, formatting, and folding.
//!
//! WHY: Template construction crosses several tightly ordered responsibilities. Keeping the
//! orchestration here and the implementation details in sibling modules makes the stage boundary
//! explicit without rebuilding template state in later frontend phases.
//!
//! ## Runtime metadata ownership
//!
//! `Template::new()` is the authoritative owner of final runtime template metadata.
//! It builds the render plan and sets `content_needs_formatting = false` before
//! returning. AST finalization trusts this and only resyncs metadata when a
//! template's content actually changes during normalization (e.g. a nested
//! compile-time template is folded into a string slice).

use crate::compiler_frontend::ast::ScopeContext;
use crate::compiler_frontend::ast::templates::error::TemplateError;
use crate::compiler_frontend::ast::templates::template::{
    CommentDirectiveKind, TemplateAtom, TemplateParsingMode, TemplateSegmentOrigin, TemplateType,
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
use crate::compiler_frontend::ast::type_interner::AstTypeInterner;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::compiler_messages::{
    CompilerDiagnostic, DiagnosticSeverity, InvalidTemplateStructureReason,
};
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::FileTokens;
#[cfg(test)]
use crate::compiler_frontend::type_coercion::compatibility::TypeCompatibilityCache;

use crate::compiler_frontend::ast::templates::template_composition::apply_inherited_child_templates_to_content;
pub(crate) use crate::compiler_frontend::ast::templates::template_types::TemplateInheritance;

// -------------------------
//  Template Construction
// -------------------------

impl Template {
    /// Creates a new template node by parsing the token stream.
    ///
    /// This is the main public entry point. It delegates to:
    /// 1. `parse_template_head` — head directives, expressions, style config
    /// 2. `parse_template_body` — body string tokens, nested templates, slots
    /// 3. Composition — child wrapper application, head-chain resolution
    /// 4. Formatting — style-directed body formatting
    /// 5. Validation — directive-owned warnings and slot insertion checks
    #[allow(clippy::result_large_err)]
    pub(crate) fn new_with_type_interner(
        token_stream: &mut FileTokens,
        context: &ScopeContext,
        type_interner: &mut AstTypeInterner<'_>,
        templates_inherited: Vec<Template>,
        string_table: &mut StringTable,
    ) -> Result<Template, CompilerDiagnostic> {
        let inheritance = TemplateInheritance::from_parent_wrappers(templates_inherited);
        Self::new_nested_template(
            token_stream,
            context,
            type_interner,
            inheritance,
            string_table,
            TemplateParsingMode::Standard,
        )
    }

    #[cfg(test)]
    #[allow(clippy::result_large_err)]
    pub fn new(
        token_stream: &mut FileTokens,
        context: &ScopeContext,
        templates_inherited: Vec<Template>,
        string_table: &mut StringTable,
    ) -> Result<Template, CompilerDiagnostic> {
        let mut type_environment =
            crate::compiler_frontend::datatypes::environment::TypeEnvironment::new();
        let mut compatibility_cache = TypeCompatibilityCache::new();
        let mut type_interner =
            AstTypeInterner::new(&mut type_environment, &mut compatibility_cache);
        Self::new_with_type_interner(
            token_stream,
            context,
            &mut type_interner,
            templates_inherited,
            string_table,
        )
    }

    /// Internal constructor that supports doc comment context propagation.
    /// Called recursively for nested templates in the body parser.
    #[allow(clippy::result_large_err)]
    pub(crate) fn new_nested_template(
        token_stream: &mut FileTokens,
        context: &ScopeContext,
        type_interner: &mut AstTypeInterner<'_>,
        inheritance: TemplateInheritance,
        string_table: &mut StringTable,
        parsing_mode: TemplateParsingMode,
    ) -> Result<Template, CompilerDiagnostic> {
        let direct_child_wrappers = inheritance.direct_child_wrappers.to_owned();

        // These are variables or special keywords passed into the template head.
        // Nested templates do not inherit formatter/style state by default.
        let mut template = Self::empty();

        // Capture the opening token location early so style/directive errors can
        // still point at the template even if parsing later advances deeply.
        template.location = token_stream.current_location();

        // Templates that call any functions or have children that call functions
        // cannot be folded at compile time because the output may change at runtime.
        // If the entire template can be folded, it becomes a plain string after the AST stage.
        let mut can_fold = true;

        // Stage 1: Parse the template head (directives, expressions, style config)
        parse_template_head(
            token_stream,
            context,
            type_interner,
            &mut template,
            &mut can_fold,
            string_table,
        )?;

        if parsing_mode == TemplateParsingMode::DocComment {
            apply_doc_comment_defaults(&mut template);
        }

        // Stage 2: Parse the template body (strings, nested templates, slots)
        parse_template_body(
            token_stream,
            context,
            type_interner,
            &mut template,
            &direct_child_wrappers,
            &mut can_fold,
            string_table,
        )?;

        let requires_post_format_recomposition =
            template_requires_post_format_recomposition(&template);

        // Stage 3: build the composed pre-format snapshot.
        build_unformatted_template_content(
            &mut template,
            &mut can_fold,
            string_table,
            requires_post_format_recomposition,
        )
        .map_err(TemplateError::into_diagnostic)?;

        // Stage 4: format body-origin text and produce a structured render plan.
        let BodyFormattingResult {
            plan: render_plan,
            content_changed,
            ..
        } = format_template_body(&template, context, string_table)
            .map_err(TemplateError::into_diagnostic)?;

        // Stage 5: rebuild formatted content and re-run composition.
        finalize_template_after_formatting(
            &mut template,
            render_plan,
            content_changed,
            &mut can_fold,
            string_table,
            requires_post_format_recomposition,
        )
        .map_err(TemplateError::into_diagnostic)?;

        template.content_needs_formatting = false;

        // Stage 6: Post-parse validation
        if matches!(
            template.kind,
            TemplateType::Comment(CommentDirectiveKind::Doc)
        ) && !template.content.is_const_evaluable_value()
        {
            return Err(CompilerDiagnostic::invalid_template_structure(
                InvalidTemplateStructureReason::NonFoldableDocComment,
                template.location.clone(),
            ));
        }

        // `$insert(...)` helpers are allowed to survive while a template still has
        // unresolved `$slot` markers, because that template may later compose into
        // an immediate parent and contribute upward. Once a template has no slots
        // left, any remaining `$insert(...)` is out of scope and must error.
        if !matches!(template.kind, TemplateType::SlotInsert(_)) && !template.has_unresolved_slots()
        {
            ensure_no_slot_insertions_remain(&template.content, &template.location, string_table)
                .map_err(TemplateError::from)
                .map_err(TemplateError::into_diagnostic)?;
        }

        if can_fold
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

// -------------------------
//  Pipeline Stage Helpers
// -------------------------

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
    can_fold: &mut bool,
    string_table: &StringTable,
    requires_post_format_recomposition: bool,
) -> Result<(), TemplateError> {
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
        compose_template_head_chain(&template.unformatted_content, can_fold, string_table)?;

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
) -> Result<BodyFormattingResult, TemplateError> {
    let formatter_result =
        match apply_body_formatter(&template.content, &template.style, string_table) {
            Ok(result) => result,

            Err(messages) => {
                let mut error_diagnostic = None;
                for diagnostic in messages.into_diagnostics() {
                    if diagnostic.severity == DiagnosticSeverity::Warning {
                        context.emit_warning(diagnostic);
                    } else if diagnostic.severity == DiagnosticSeverity::Error
                        && error_diagnostic.is_none()
                    {
                        error_diagnostic = Some(diagnostic);
                    }
                }

                return Err(error_diagnostic
                    .map(TemplateError::from)
                    .unwrap_or_else(|| {
                        CompilerError::compiler_error(
                            "Template formatter failed without returning a compiler error.",
                        )
                        .into()
                    }));
            }
        };

    for warning in &formatter_result.warnings {
        context.emit_warning(warning.clone());
    }

    Ok(formatter_result)
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
///
/// ## Metadata invariant on exit
///
/// When this function returns, `template.render_plan` is guaranteed to match
/// `template.content`. HIR lowering trusts this invariant for runtime templates.
fn finalize_template_after_formatting(
    template: &mut Template,
    render_plan: TemplateRenderPlan,
    content_changed: bool,
    can_fold: &mut bool,
    string_table: &StringTable,
    requires_post_format_recomposition: bool,
) -> Result<(), TemplateError> {
    // If formatting made no changes and no post-format recomposition is needed,
    // skip the expensive content -> plan -> content round-trip.
    if content_changed || requires_post_format_recomposition {
        template.content = render_plan.rebuild_content();

        if requires_post_format_recomposition {
            template.content = apply_inherited_child_templates_to_content(
                template.content.clone(),
                &template.style.child_templates,
                string_table,
            )?;

            template.content =
                compose_template_head_chain(&template.content, can_fold, string_table)?;
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

// -------------------------
//  Composition Predicates
// -------------------------

/// Returns true when the template contains head-origin atoms or child wrappers
/// that require recomposition after formatting.
///
/// WHY:
/// - Formatting may rewrite body text, but head-origin segments and `$children(..)`
///   wrappers are structural and must be reapplied to the rebuilt content.
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
