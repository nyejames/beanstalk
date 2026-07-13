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
//! It finalizes the parser-owned TIR root and refreshes the template kind before
//! returning. AST finalization consumes that TIR reference rather than rebuilding
//! parser structure.

use crate::compiler_frontend::ast::ScopeContext;
use crate::compiler_frontend::ast::templates::error::TemplateError;
use crate::compiler_frontend::ast::templates::styles::markdown::markdown_formatter;
use crate::compiler_frontend::ast::templates::template::{
    BodyWhitespacePolicy, CommentDirectiveKind, Style, TemplateParsingMode, TemplateType,
};
use crate::compiler_frontend::ast::templates::template_body_parser::{
    NestedTemplateParseOptions, TemplateBodyParseRequest, parse_template_body,
};
use crate::compiler_frontend::ast::templates::template_build_state::TemplateBuildState;
use crate::compiler_frontend::ast::templates::template_control_flow::{
    TemplateControlFlowValidationMode, validate_const_required_template_control_flow,
    validate_runtime_template_control_flow_slot_artifacts,
};
use crate::compiler_frontend::ast::templates::template_head_parser::{
    ParsedTemplateHead, TemplateHeadParseRequest, apply_doc_comment_defaults, parse_template_head,
};
use crate::compiler_frontend::ast::templates::template_render_units::{
    ControlFlowRenderUnitRequest, install_formatted_tir_reference_for_linear_template,
    prepare_control_flow_render_units, template_contains_control_flow,
};
use crate::compiler_frontend::ast::templates::template_types::Template;
use crate::compiler_frontend::ast::templates::tir::{
    ChildTemplateOccurrenceId, TemplateConstructionContext, TemplateIr, TemplateIrId,
    TemplateIrNodeId, TemplateIrNodeKind, TemplateIrRegistry, TemplateIrStore, TemplateOverlaySet,
    TemplateRef, TemplateTirPhase, TemplateTirReference, TemplateWrapperReference,
    TemplateWrapperSetRef, TirWrapperApplicationMode, TirWrapperContext, TirWrapperContextOverlay,
    TirWrapperContextOverlayId, classify_materialized_current_tir_template,
    compose_tir_head_chain_with_overlays, merge_tir_slot_resolution_overlay_sets,
};

use crate::compiler_frontend::ast::type_interner::AstTypeInterner;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::compiler_messages::{
    CompilerDiagnostic, InvalidTemplateSlotReason, InvalidTemplateStructureReason,
};
use crate::compiler_frontend::instrumentation::{
    AstCounter, FrontendCounter, add_ast_counter, increment_frontend_counter,
};
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::FileTokens;
#[cfg(test)]
use crate::compiler_frontend::{
    datatypes::environment::TypeEnvironment, type_coercion::compatibility::TypeCompatibilityCache,
};
use crate::libraries::SourceFileKind;
use std::rc::Rc;

const SYNTHETIC_CONTENT_CONSTANT_NAME: &str = "content";

/// Boxed diagnostic result for the template construction family.
///
/// Template construction owns this large diagnostic boundary. Plain diagnostics are boxed once
/// here and existing boxed callers propagate without an unbox/rebox cycle.
type TemplateConstructionResult = Result<Template, Box<CompilerDiagnostic>>;

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
    pub(crate) fn new_with_type_interner(
        token_stream: &mut FileTokens,
        context: &ScopeContext,
        type_interner: &mut AstTypeInterner<'_>,
        direct_child_wrappers: Vec<TemplateWrapperReference>,
        string_table: &mut StringTable,
    ) -> TemplateConstructionResult {
        let default_style = default_nested_style_for_source_path(token_stream, string_table);
        Self::new_nested_template(
            token_stream,
            context,
            type_interner,
            direct_child_wrappers,
            string_table,
            NestedTemplateParseOptions::runtime_capable().with_default_style(default_style),
        )
    }

    /// Creates a template for a context that must fold during AST construction.
    ///
    /// Const-required callers need the structured control-flow template so AST
    /// folding can select branches and produce source diagnostics before the
    /// template reaches runtime lowering.
    pub(crate) fn new_const_required_with_type_interner(
        token_stream: &mut FileTokens,
        context: &ScopeContext,
        type_interner: &mut AstTypeInterner<'_>,
        direct_child_wrappers: Vec<TemplateWrapperReference>,
        string_table: &mut StringTable,
    ) -> TemplateConstructionResult {
        let default_style = default_nested_style_for_source_path(token_stream, string_table);
        let template = Self::new_nested_template(
            token_stream,
            context,
            type_interner,
            direct_child_wrappers,
            string_table,
            NestedTemplateParseOptions::const_required().with_default_style(default_style),
        )?;

        {
            validate_const_required_template_control_flow(
                &template,
                &context.template_ir_registry.borrow(),
                string_table,
            )?;
        }

        Ok(template)
    }

    #[cfg(test)]
    pub(crate) fn new(
        token_stream: &mut FileTokens,
        context: &ScopeContext,
        templates_inherited: Vec<TemplateWrapperReference>,
        string_table: &mut StringTable,
    ) -> TemplateConstructionResult {
        let mut type_environment = TypeEnvironment::new();
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
    pub(crate) fn new_const_required(
        token_stream: &mut FileTokens,
        context: &ScopeContext,
        templates_inherited: Vec<TemplateWrapperReference>,
        string_table: &mut StringTable,
    ) -> TemplateConstructionResult {
        let mut type_environment = TypeEnvironment::new();
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
    pub(crate) fn new_nested_template(
        token_stream: &mut FileTokens,
        context: &ScopeContext,
        type_interner: &mut AstTypeInterner<'_>,
        direct_child_wrappers: Vec<TemplateWrapperReference>,
        string_table: &mut StringTable,
        parse_options: NestedTemplateParseOptions,
    ) -> TemplateConstructionResult {
        let NestedTemplateParseOptions {
            parsing_mode,
            control_flow_validation,
            control_context,
            default_style,
        } = parse_options;

        // The parser-local build state accumulates head/body metadata while
        // parsing. The durable `Template` is constructed once after
        // authoritative TIR identity exists, not mutated throughout parsing.
        let mut build_state = TemplateBuildState::new();

        // Capture the opening token location on the construction context; it
        // remains the sole location owner so style/directive errors still point
        // at the template even if parsing later advances deeply.
        let mut construction_context = TemplateConstructionContext::new(
            Rc::clone(&context.template_ir_store),
            context.template_ir_store_id,
            Rc::clone(&context.template_ir_registry),
            token_stream.current_location(),
        );

        // ---------------------
        //  Parse template head
        // ---------------------
        //
        // Directives, expressions, and style config.
        let parsed_head = parse_template_head(
            token_stream,
            TemplateHeadParseRequest {
                context,
                type_interner,
                build_state: &mut build_state,
                construction_context: &mut construction_context,
                control_flow_validation,
                string_table,
            },
        )?;

        apply_default_style_if_needed(&mut build_state, &parsed_head, default_style.as_ref());

        let body_mode = parsed_head.body_mode;

        if parsing_mode == TemplateParsingMode::DocComment {
            apply_doc_comment_defaults(&mut build_state);
        }

        // Stage 2: Parse the template body (strings, nested templates, slots)
        parse_template_body(
            token_stream,
            &mut build_state,
            &mut construction_context,
            TemplateBodyParseRequest {
                context,
                type_interner,
                body_mode,
                direct_child_wrappers: &direct_child_wrappers,
                control_flow_validation,
                control_context,
                string_table,
                default_style: default_style.clone(),
            },
        )?;

        // Stage 3-5: render-unit shaping.
        //
        // Linear templates always install a TIR-formatted root. Control-flow
        // templates keep branch/body units structured so later folding/lowering
        // can stay lazy.
        let style = build_state.style.to_owned();
        let child_wrappers = build_state.child_wrappers.to_owned();
        if build_state.control_flow.is_some() {
            prepare_control_flow_render_units(
                &mut build_state,
                &mut construction_context,
                ControlFlowRenderUnitRequest {
                    style: &style,
                    child_wrappers: &child_wrappers,
                    context,
                    string_table,
                },
            )
            .map_err(TemplateError::into_diagnostic)?;
        }

        // Finish the parser builder-state TIR with a provisional kind. The
        // kind is updated after classification once the TIR-native composition
        // block below has produced the final post-composition reference.
        //
        // Prepared control-flow owner roots are at Formatted phase because
        // render-unit preparation has installed formatted body content. Linear
        // templates start at Parsed; linear formatting installs the formatted
        // reference below.
        let has_control_flow = build_state.control_flow.is_some();
        let owner_phase = if has_control_flow {
            TemplateTirPhase::Formatted
        } else {
            TemplateTirPhase::Parsed
        };
        let mut tir_reference = construction_context.finish(
            build_state.style.to_owned(),
            build_state.kind.to_owned(),
            owner_phase,
            construction_context.location().to_owned(),
        );
        let style = build_state.style.to_owned();
        install_formatted_tir_reference_for_linear_template(
            &mut tir_reference,
            has_control_flow,
            &style,
            context,
            string_table,
        )
        .map_err(TemplateError::into_diagnostic)?;

        {
            // Head-chain composition materializes slot routing as needed, while
            // `$children(..)` direct-child wrappers are represented as
            // wrapper-context overlays. Both passes update the parser-owned TIR
            // reference directly. There is no content-to-TIR fallback here.
            //
            // Overlay threading: head-chain composition returns a `ComposedTirRoot`
            // with an optional non-empty slot-resolution overlay-set ID. Wrapper
            // context is attached after head-chain composition so it uses the
            // final child occurrence IDs. The store borrow is released during
            // registry-level calls so the registry can access the same store
            // through its internal `RefCell` without a borrow conflict.

            let store_id = context.template_ir_store_id;
            let template_id = tir_reference.root.template_id;

            // --- Phase 1: head-chain composition ---

            add_ast_counter(AstCounter::TemplateTirHeadChainCompositionCalls, 1);

            let original_root = {
                let store = context.template_ir_store.borrow();
                store
                    .get_template(template_id)
                    .map(|t| t.root)
                    .ok_or_else(|| {
                        TemplateError::from(CompilerError::compiler_error(
                            "Template head-chain composition started from a missing TIR root.",
                        ))
                    })
                    .map_err(TemplateError::into_diagnostic)?
            };

            // Run registry-level head-chain composition. The store borrow is
            // released so the registry can access the same store through its
            // internal `RefCell`.
            let composed = compose_tir_head_chain_with_overlays(
                &context.template_ir_registry,
                store_id,
                template_id,
                string_table,
                matches!(
                    control_flow_validation,
                    TemplateControlFlowValidationMode::RuntimeCapable
                ),
            )?;

            if composed.root != original_root {
                add_ast_counter(AstCounter::TemplateTirHeadChainCompositionHits, 1);

                // Thread the non-empty overlay set from head-chain
                // composition. When child-wrapper composition already
                // produced a slot-resolution overlay, merge the two payloads
                // through the slot-composition owner instead of composing
                // overlay sets and overwriting one slot-resolution dimension.
                let previous_overlay_set_id = tir_reference.overlay_set_id;

                let overlay_set_id = if let Some(slot_overlay_set_id) = composed.slot_overlay_set_id
                {
                    merge_tir_slot_resolution_overlay_sets(
                        &mut context.template_ir_registry.borrow_mut(),
                        previous_overlay_set_id,
                        slot_overlay_set_id,
                    )?
                } else {
                    previous_overlay_set_id
                };

                let mut template_ir_store = context.template_ir_store.borrow_mut();
                let original_template = template_ir_store
                    .get_template(template_id)
                    .cloned()
                    .ok_or_else(|| {
                        TemplateError::from(CompilerError::compiler_error(
                            "Template head-chain composition lost its source TIR template.",
                        ))
                    })
                    .map_err(TemplateError::into_diagnostic)?;
                let composed_template_id = template_ir_store.push_template(TemplateIr::new(
                    composed.root,
                    original_template.style,
                    original_template.kind,
                    original_template.summary,
                    original_template.location,
                ));

                let phase = if tir_reference.phase.is_at_least(TemplateTirPhase::Formatted) {
                    TemplateTirPhase::Formatted
                } else {
                    TemplateTirPhase::Composed
                };

                tir_reference = TemplateTirReference {
                    root: TemplateRef::new(store_id, composed_template_id),
                    store_owner: template_ir_store.owner(),
                    is_composed: true,
                    // Head-chain composition consumes the already formatted
                    // body root only when Phase 8 installed one earlier in
                    // this constructor flow. Otherwise this remains a
                    // Composed root for the later formatter cutover.
                    phase,
                    overlay_set_id,
                };
            }

            let wrapper_context_owns_direct_children = !build_state.child_wrappers.is_empty();

            // --- Phase 2: wrapper-context overlay ---
            //
            // Record `$fresh` suppression and inherited wrapper-set context on
            // the final authoritative root after head-chain composition so the
            // occurrence keys match the structural root consumed downstream.
            if wrapper_context_owns_direct_children {
                attach_wrapper_context_overlay_to_template(
                    &mut tir_reference,
                    &build_state.child_wrappers,
                    context,
                );
            }
        }

        // Stage 6: Classification from the final TIR reference (post-composition).
        //
        // The reference is now either a composed root with slots expanded and
        // inserts consumed, or a formatted linear root. Classification reads
        // that authoritative reference without a separate TIR allocation.
        let template_classification = {
            let mut template_ir_store = context.template_ir_store.borrow_mut();
            let template_id = tir_reference.root.template_id;
            classify_materialized_current_tir_template(
                &build_state.kind,
                &mut template_ir_store,
                template_id,
                string_table,
            )
            .map_err(TemplateError::into_diagnostic)?
        };

        build_state.refresh_kind_from_tir_classification(&template_classification);

        // Post-parse validation
        if matches!(
            build_state.kind,
            TemplateType::Comment(CommentDirectiveKind::Doc)
        ) && !template_classification.shape_const_evaluable
        {
            return Err(Box::new(CompilerDiagnostic::invalid_template_structure(
                InvalidTemplateStructureReason::NonFoldableDocComment,
                construction_context.location().to_owned(),
            )));
        }

        // Construct the durable `Template` now that authoritative TIR identity
        // and classified kind exist. The build state fields are moved into it.
        let template = Template {
            control_flow: build_state.control_flow,
            kind: build_state.kind,
            style: build_state.style,
            child_wrappers: build_state.child_wrappers,
            tir_reference: Some(tir_reference),
            id: build_state.id,
            location: construction_context.location().to_owned(),
        };

        if matches!(
            control_flow_validation,
            TemplateControlFlowValidationMode::RuntimeCapable
        ) {
            let registry = context.template_ir_registry.borrow();
            let template_ir_store = context.template_ir_store.borrow();

            if template_contains_control_flow(
                &template,
                &template_ir_store,
                Some(construction_context.builder()),
            ) {
                validate_runtime_template_control_flow_slot_artifacts(
                    &template,
                    &registry,
                    &template_ir_store,
                    Some(construction_context.builder()),
                )
                .map_err(TemplateError::into_diagnostic)?;
            }
        }

        // `$insert(...)` helpers are allowed to survive while a template still has
        // unresolved `$slot` markers, because that template may later compose into
        // an immediate parent and contribute upward. Once a template has no slots
        // left, any remaining `$insert(...)` is out of scope and must error.
        //
        // Composed templates are exempt: head-chain composition routes insert
        // contributions into the receiving wrapper's slots, leaving
        // `InsertContribution` nodes in the composed tree. These are not
        // orphaned — they were consumed by composition — so the check must not
        // fire on a composed reference.
        if !matches!(template.kind, TemplateType::SlotInsert(_))
            && !template_classification.has_unresolved_slots
            && template_classification.has_slot_insertions
            && !template
                .tir_reference
                .as_ref()
                .is_some_and(|reference| reference.is_composed)
        {
            return Err(Box::new(CompilerDiagnostic::invalid_template_slot(
                InvalidTemplateSlotReason::InsertOutsideParentSlot,
                None,
                template.location.clone(),
            )));
        }

        // Align the final TIR entry's kind with the classification result.
        // `finish()` was called with a provisional kind before composition; this
        // ensures the authoritative TIR entry carries the classified kind.
        if let Some(template_id) = template.tir_template_id() {
            let mut template_ir_store = context.template_ir_store.borrow_mut();
            template_ir_store.set_template_kind(template_id, template.kind.to_owned());
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

fn default_nested_style_for_source_path(
    token_stream: &FileTokens,
    string_table: &StringTable,
) -> Option<Style> {
    if !is_beandown_content_constant_path(token_stream, string_table) {
        return None;
    }

    Some(markdown_default_style())
}

fn is_beandown_content_constant_path(
    token_stream: &FileTokens,
    string_table: &StringTable,
) -> bool {
    if token_stream.src_path.name_str(string_table) != Some(SYNTHETIC_CONTENT_CONSTANT_NAME) {
        return false;
    }

    token_stream
        .src_path
        .parent()
        .and_then(|parent| parent.name_str(string_table).map(str::to_owned))
        .is_some_and(|source_name| {
            source_name.ends_with(SourceFileKind::Beandown.extension_suffix())
        })
}

fn markdown_default_style() -> Style {
    let mut style = Style::default();
    style.id = "markdown";
    style.formatter = Some(markdown_formatter());
    style.body_whitespace_policy = BodyWhitespacePolicy::StyleDirectiveControlled;
    style
}

fn apply_default_style_if_needed(
    build_state: &mut TemplateBuildState,
    parsed_head: &ParsedTemplateHead,
    default_style: Option<&Style>,
) {
    if parsed_head.has_explicit_template_directive {
        return;
    }

    if !matches!(
        build_state.kind,
        TemplateType::String | TemplateType::StringFunction
    ) {
        return;
    }

    if let Some(default_style) = default_style {
        build_state.apply_style(default_style.to_owned());
    }
}

fn attach_wrapper_context_overlay_to_template(
    tir_reference: &mut TemplateTirReference,
    child_wrappers: &[TemplateWrapperReference],
    context: &ScopeContext,
) {
    let template_id = tir_reference.root.template_id;

    let wrapper_overlay_id = {
        let mut store = context.template_ir_store.borrow_mut();
        let mut registry = context.template_ir_registry.borrow_mut();
        build_wrapper_context_overlay_for_template(
            &mut store,
            template_id,
            Some(child_wrappers),
            &mut registry,
        )
    };

    let Some(wrapper_overlay_id) = wrapper_overlay_id else {
        return;
    };

    let mut registry = context.template_ir_registry.borrow_mut();
    let current_overlay_set_id = tir_reference.overlay_set_id;

    let wrapper_only_overlay_set_id = registry.allocate_overlay_set(TemplateOverlaySet {
        expression_overrides: None,
        slot_resolution: None,
        wrapper_context: Some(wrapper_overlay_id),
    });

    if let Ok(merged_overlay_set_id) =
        registry.compose_overlay_sets(&[current_overlay_set_id, wrapper_only_overlay_set_id])
    {
        tir_reference.overlay_set_id = merged_overlay_set_id;
        tir_reference.is_composed = true;
        if !tir_reference.phase.is_at_least(TemplateTirPhase::Composed) {
            tir_reference.phase = TemplateTirPhase::Composed;
        }
    }
}

/// Builds a wrapper-context overlay for a template's child-template occurrences.
///
/// WHAT: recursively walks the template's TIR tree, finds `ChildTemplate` nodes,
///       and records `$fresh` suppression for preserved children. For templates
///       where structural child-wrapper wrapping was deferred, also records
///       inherited wrapper context for non-fresh direct children.
/// WHY: this is the Phase C Step 3 production path for wrapper-context overlays.
fn build_wrapper_context_overlay_for_template(
    store: &mut TemplateIrStore,
    template_id: TemplateIrId,
    inherited_wrapper_refs: Option<&[TemplateWrapperReference]>,
    registry: &mut TemplateIrRegistry,
) -> Option<TirWrapperContextOverlayId> {
    let template = store.get_template(template_id)?;
    let mut contexts = Vec::new();
    collect_wrapper_contexts(store, template.root, inherited_wrapper_refs, &mut contexts);
    if contexts.is_empty() {
        None
    } else {
        Some(registry.allocate_wrapper_context_overlay(TirWrapperContextOverlay { contexts }))
    }
}

fn collect_wrapper_contexts(
    store: &mut TemplateIrStore,
    node_id: TemplateIrNodeId,
    inherited_wrapper_refs: Option<&[TemplateWrapperReference]>,
    contexts: &mut Vec<(ChildTemplateOccurrenceId, TirWrapperContext)>,
) {
    let node = match store.get_node(node_id) {
        Some(node) => node.clone(),
        None => return,
    };

    match &node.kind {
        TemplateIrNodeKind::ChildTemplate {
            reference,
            occurrence_id,
        } => {
            if let Some(child_id) = reference.template_id_in_store(store.store_id())
                && let Some(child_template) = store.get_template(child_id).cloned()
            {
                if child_template.style.skip_parent_child_wrappers {
                    contexts.push((
                        *occurrence_id,
                        TirWrapperContext {
                            inherited_wrapper_set: None,
                            skip_parent_child_wrappers: true,
                            application_mode: TirWrapperApplicationMode::Always,
                        },
                    ));
                } else if let Some(wrapper_refs) = inherited_wrapper_refs
                    && !wrapper_refs.is_empty()
                {
                    // Record inherited context only after all wrappers normalize
                    // to TIR refs; partial wrapper sets would create a silent
                    // parallel composition path.
                    let wrapper_set_id = store.push_or_reuse_wrapper_set(wrapper_refs.to_vec());
                    let wrapper_set_ref =
                        TemplateWrapperSetRef::new(store.store_id(), wrapper_set_id);
                    let application_mode = if child_template.summary.has_control_flow {
                        TirWrapperApplicationMode::IfChildEmits
                    } else {
                        TirWrapperApplicationMode::Always
                    };
                    contexts.push((
                        *occurrence_id,
                        TirWrapperContext {
                            inherited_wrapper_set: Some(wrapper_set_ref),
                            skip_parent_child_wrappers: false,
                            application_mode,
                        },
                    ));
                }
            }
        }
        TemplateIrNodeKind::Sequence { children } => {
            for child_id in children {
                collect_wrapper_contexts(store, *child_id, inherited_wrapper_refs, contexts);
            }
        }
        TemplateIrNodeKind::BranchChain { branches, fallback } => {
            for branch in branches {
                collect_wrapper_contexts(store, branch.body, inherited_wrapper_refs, contexts);
            }
            if let Some(fallback_id) = fallback {
                collect_wrapper_contexts(store, *fallback_id, inherited_wrapper_refs, contexts);
            }
        }
        TemplateIrNodeKind::Loop {
            body,
            aggregate_wrapper,
            ..
        } => {
            collect_wrapper_contexts(store, *body, inherited_wrapper_refs, contexts);
            if let Some(wrapper_id) = aggregate_wrapper {
                collect_wrapper_contexts(store, *wrapper_id, inherited_wrapper_refs, contexts);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
#[path = "tests/create_template_node/mod.rs"]
mod create_template_node_tests;
