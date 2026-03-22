//! Template node construction orchestrator.
//!
//! WHAT: Provides `Template::new()` — the main entry point for creating a
//! template AST node from a token stream. Delegates to focused submodules
//! for head parsing, body parsing, composition, formatting, and folding.
//!
//! WHY: This file used to contain ALL template logic (~1700 lines). It has
//! been refactored into an orchestrator that coordinates the pipeline stages
//! defined in sibling modules while keeping the overall flow readable.

use crate::compiler_frontend::ast::ast::ScopeContext;
use crate::compiler_frontend::ast::templates::template::{CommentDirectiveKind, TemplateType};
use crate::compiler_frontend::ast::templates::template_body_parser::parse_template_body;
use crate::compiler_frontend::ast::templates::template_composition::compose_template_head_chain;
use crate::compiler_frontend::ast::templates::template_formatting::apply_body_formatter;
use crate::compiler_frontend::ast::templates::template_head_parser::{
    apply_doc_comment_defaults, emit_css_template_warnings, emit_html_template_warnings,
    parse_template_head,
};
use crate::compiler_frontend::ast::templates::template_slots::ensure_no_slot_insertions_remain;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::FileTokens;
use crate::return_syntax_error;

// Re-export `Template` from its canonical module for backward compatibility.
// All existing `use crate::...::create_template_node::Template` imports
// continue to resolve without changes.
pub use crate::compiler_frontend::ast::templates::template_types::Template;
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
    /// 5. Validation — CSS/HTML warnings, slot insertion checks
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
        // These are variables or special keywords passed into the template head
        let mut template = Self::create_default_with_inherited_style(inheritance.recursive_style);
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

        // Stage 3: Composition — apply child wrappers and resolve head-chain
        template.content = apply_inherited_child_templates_to_content(
            template.content,
            &template.style.child_templates,
            string_table,
        )?;
        template.content =
            compose_template_head_chain(&template.content, &mut foldable, string_table)?;
        template.unformatted_content = template.content.clone();

        template.content_needs_formatting = false;

        // Stage 4: Formatting — normalize body content before folding/lowering.
        // This keeps runtime templates simple: only compile-time-known body strings
        // are rewritten, while dynamic chunks remain untouched and keep their order.
        template.render_plan = Some(apply_body_formatter(
            &template.content,
            &template.style,
            string_table,
        ));

        // Stage 5: Post-parse validation
        if matches!(
            template.kind,
            TemplateType::Comment(CommentDirectiveKind::Doc)
        ) && !template.content.is_const_evaluable_value()
        {
            return_syntax_error!(
                "'$doc' comments can only contain compile-time values.",
                template.location.to_error_location(string_table),
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

        emit_css_template_warnings(&template, context, string_table);
        emit_html_template_warnings(&template, context, string_table);

        Ok(template)
    }
}

#[cfg(test)]
#[path = "tests/create_template_node_tests.rs"]
mod create_template_node_tests;
