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
    CommentDirectiveKind, TemplateParsingMode, TemplateType,
};
use crate::compiler_frontend::ast::templates::template_body_parser::{
    NestedTemplateParseOptions, TemplateBodyParseRequest, parse_template_body,
};
use crate::compiler_frontend::ast::templates::template_control_flow::{
    TemplateControlFlowValidationMode, validate_const_required_template_control_flow,
    validate_runtime_template_control_flow_slot_artifacts,
};
use crate::compiler_frontend::ast::templates::template_head_parser::{
    apply_doc_comment_defaults, parse_template_head,
};
use crate::compiler_frontend::ast::templates::template_render_units::{
    prepare_control_flow_render_units, prepare_template_render_unit, template_contains_control_flow,
};
use crate::compiler_frontend::ast::templates::template_slots::{
    SlotResolutionMode, ensure_no_slot_insertions_remain,
};
use crate::compiler_frontend::ast::templates::template_types::Template;
use crate::compiler_frontend::ast::type_interner::AstTypeInterner;
use crate::compiler_frontend::compiler_messages::{
    CompilerDiagnostic, InvalidTemplateStructureReason,
};
use crate::compiler_frontend::instrumentation::{FrontendCounter, increment_frontend_counter};
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::FileTokens;
#[cfg(test)]
use crate::compiler_frontend::type_coercion::compatibility::TypeCompatibilityCache;

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
            NestedTemplateParseOptions::runtime_capable(),
        )
    }

    /// Creates a template for a context that must fold during AST construction.
    ///
    /// Const-required callers need the structured control-flow template so AST
    /// folding can select branches and produce source diagnostics before the
    /// template reaches runtime lowering.
    #[allow(clippy::result_large_err)]
    pub(crate) fn new_const_required_with_type_interner(
        token_stream: &mut FileTokens,
        context: &ScopeContext,
        type_interner: &mut AstTypeInterner<'_>,
        templates_inherited: Vec<Template>,
        string_table: &mut StringTable,
    ) -> Result<Template, CompilerDiagnostic> {
        let inheritance = TemplateInheritance::from_parent_wrappers(templates_inherited);
        let template = Self::new_nested_template(
            token_stream,
            context,
            type_interner,
            inheritance,
            string_table,
            NestedTemplateParseOptions::const_required(),
        )?;

        validate_const_required_template_control_flow(&template, &template.location)?;

        Ok(template)
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

    #[cfg(test)]
    #[allow(clippy::result_large_err)]
    pub fn new_const_required(
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
        Self::new_const_required_with_type_interner(
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
        parse_options: NestedTemplateParseOptions,
    ) -> Result<Template, CompilerDiagnostic> {
        let NestedTemplateParseOptions {
            parsing_mode,
            control_flow_validation,
            control_context,
        } = parse_options;

        let direct_child_wrappers = inheritance.direct_child_wrappers.to_owned();
        let slot_resolution_mode = match control_flow_validation {
            TemplateControlFlowValidationMode::RuntimeCapable => {
                SlotResolutionMode::AllowRuntimePlans
            }
            TemplateControlFlowValidationMode::ConstRequired => SlotResolutionMode::ComposeOnly,
        };

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
        let parsed_head = parse_template_head(
            token_stream,
            context,
            type_interner,
            &mut template,
            &mut can_fold,
            control_flow_validation,
            string_table,
        )?;

        let body_mode = parsed_head.body_mode;

        if parsing_mode == TemplateParsingMode::DocComment {
            apply_doc_comment_defaults(&mut template);
        }

        // Stage 2: Parse the template body (strings, nested templates, slots)
        parse_template_body(
            token_stream,
            &mut template,
            TemplateBodyParseRequest {
                context,
                type_interner,
                body_mode,
                direct_child_wrappers: &direct_child_wrappers,
                control_flow_validation,
                control_context,
                foldable: &mut can_fold,
                string_table,
            },
        )?;

        // Stage 3-5: render-unit shaping.
        //
        // Linear templates produce one finalized render unit. Control-flow templates
        // keep branch/body units structured so later folding/lowering can stay lazy.
        if let Some(control_flow) = &mut template.control_flow {
            let shared_head_prefix = template.content.to_owned();
            prepare_control_flow_render_units(
                control_flow,
                &shared_head_prefix,
                &template.style,
                context,
                &mut can_fold,
                string_table,
                slot_resolution_mode,
            )
            .map_err(TemplateError::into_diagnostic)?;

            // Keep the shared head prefix on the control-flow owner. Template `loop`
            // applies it once around the aggregate in later folding/lowering; template
            // `if` branches already carry their selected-branch prefix units.
            let prepared_head = prepare_template_render_unit(
                shared_head_prefix,
                &template.style,
                context,
                &mut can_fold,
                string_table,
                slot_resolution_mode,
            )
            .map_err(TemplateError::into_diagnostic)?;

            template.content = prepared_head.content;
            template.unformatted_content = prepared_head.unformatted_content;
            template.render_plan = Some(prepared_head.render_plan);
        } else {
            let prepared = prepare_template_render_unit(
                template.content.to_owned(),
                &template.style,
                context,
                &mut can_fold,
                string_table,
                slot_resolution_mode,
            )
            .map_err(TemplateError::into_diagnostic)?;

            template.content = prepared.content;
            template.unformatted_content = prepared.unformatted_content;
            template.render_plan = Some(prepared.render_plan);
        }

        template.content_needs_formatting = false;
        template.refresh_kind_from_content();

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

        if template_contains_control_flow(&template)
            && matches!(
                control_flow_validation,
                TemplateControlFlowValidationMode::RuntimeCapable
            )
        {
            validate_runtime_template_control_flow_slot_artifacts(&template)?;
        }

        // `$insert(...)` helpers are allowed to survive while a template still has
        // unresolved `$slot` markers, because that template may later compose into
        // an immediate parent and contribute upward. Once a template has no slots
        // left, any remaining `$insert(...)` is out of scope and must error.
        if !matches!(template.kind, TemplateType::SlotInsert(_)) && !template.has_unresolved_slots()
        {
            ensure_no_slot_insertions_remain(&template)
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

        increment_frontend_counter(FrontendCounter::TemplateCount);
        match control_flow_validation {
            TemplateControlFlowValidationMode::ConstRequired => {
                increment_frontend_counter(FrontendCounter::ConstTemplateCount);
            }
            TemplateControlFlowValidationMode::RuntimeCapable => {
                increment_frontend_counter(FrontendCounter::RuntimeTemplateCount);
            }
        }

        Ok(template)
    }
}

#[cfg(test)]
#[path = "tests/create_template_node/mod.rs"]
mod create_template_node_tests;
