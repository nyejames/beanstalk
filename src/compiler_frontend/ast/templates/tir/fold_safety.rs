//! Shared safety gates for view-backed template folding.
//!
//! WHAT: centralizes the conservative checks used before callers fold a stable
//! `TirView` through the direct store-backed path.
//!
//! WHY: keeping these policies in TIR avoids slightly different finalization
//! and HIR-handoff gates as individual overlay dimensions become foldable.

use crate::compiler_frontend::ast::templates::error::TemplateError;
use crate::compiler_frontend::ast::templates::template::SlotKey;
use crate::compiler_frontend::ast::templates::tir::classification::tir_view_subtree_is_const_evaluable_value_with_expression_stack;
use crate::compiler_frontend::ast::templates::tir::ids::{
    TemplateIrId, TemplateIrNodeId, TemplateSlotPlanId, TemplateWrapperSetId,
};
use crate::compiler_frontend::ast::templates::tir::node::{TemplateIrNodeKind, TirSlotPlaceholder};
use crate::compiler_frontend::ast::templates::tir::overlays::{
    TemplateOverlaySet, TemplateOverlaySetId, TirSlotResolutionKind, TirWrapperContext,
};
use crate::compiler_frontend::ast::templates::tir::refs::{
    TemplateTirChildReference, TemplateWrapperReference,
};
use crate::compiler_frontend::ast::templates::tir::slot_composition::collect_tir_slot_schema;
use crate::compiler_frontend::ast::templates::tir::slot_plan::TemplateSlotSiteRenderPiece;
use crate::compiler_frontend::ast::templates::tir::store::TemplateIrStore;
use crate::compiler_frontend::ast::templates::tir::view::{TemplateTirPhase, TirView};
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::symbols::string_interning::StringTable;

use std::collections::HashSet;

/// Returns `Ok(true)` when a `TirView` can be folded through the narrow
/// shortcut used by callers that still expect empty-overlay behavior.
/// Malformed same-store authority returns `Err`; valid unsupported shapes
/// return `Ok(false)`.
///
/// Wrapper-context-only overlays are accepted here because wrapper application
/// is now a production fold/handoff responsibility. Expression and slot overlays
/// keep using the broader `tir_view_is_expression_overlay_linear_fold_safe`
/// entry point so this compatibility gate does not expand unrelated surfaces.
pub(crate) fn tir_view_is_empty_overlay_linear_fold_safe(
    view: &TirView<'_>,
    store: &TemplateIrStore,
) -> Result<bool, CompilerError> {
    let root = view.root_ref();

    let _authority = validate_view_fold_authority(view, store)?;
    let overlay_set = view.overlay_set()?;

    if overlay_set.is_empty() {
        let mut visiting = HashSet::new();
        return template_root_is_linear_fold_safe(view, store, root, &mut visiting);
    }

    if overlay_set.expression_overrides.is_none()
        && overlay_set.slot_resolution.is_none()
        && overlay_set.wrapper_context.is_some()
    {
        let string_table = StringTable::new();
        let mut walk = ViewNativeWalkContext {
            visiting_templates: HashSet::new(),
            slot_resolution_active: false,
            expression_overlay_stack: vec![view.overlay_set_id()],
        };
        return Ok(check_template_root_view_native_overlay_fold_safety(
            store,
            Some(view),
            view,
            root,
            false,
            &mut walk,
            &string_table,
        )?
        .is_none());
    }

    Ok(false)
}

/// Named reason why a `TirView` was rejected by the view-native fold safety
/// gate.
///
/// WHAT: attributes each store-backed fold fallback to a specific overlay or
/// structural shape so counter evidence can rank which shapes dominate the
/// remaining current-state materialization volume.
///
/// WHY: the generic `TirFinalizationFold*` counters only show how many
/// attempts fell back. Named reasons let the performance plan decide whether the
/// dominant blocker is an overlay dimension (potentially fixable through
/// Phase 3-5 fold shapes) or a structural tree shape (potentially needing
/// broader design work).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) enum TirFoldFallbackReason {
    // --- Overlay-level reasons ---
    /// The overlay set carries a wrapper-context overlay with an unsupported
    /// shape such as a non-virtual wrapper set, so the view-native
    /// fold walker cannot expand it.
    WrapperContextOverlay,

    /// An expression overlay is present but the view phase has not reached
    /// `Finalized`, so the normalized expression payload is not yet stable.
    ExpressionOverlayBelowFinalized,

    // --- Structural tree reasons (named by code shape) ---
    /// The template declares a runtime slot plan, which requires HIR/runtime
    /// lowering rather than compile-time folding.
    RuntimeSlotPlan,

    /// The template's conditional child wrapper set contains a wrapper subtree
    /// that the virtual wrapper fold path cannot handle.
    UnsafeWrapperTree,

    /// A text or dynamic-expression node carries a reactive subscription,
    /// which must stay on the structured handoff path.
    ReactiveSegment,

    /// A slot node is present but no slot-resolution overlay is active. The
    /// current TIR cannot yet distinguish a genuinely empty slot from one that
    /// needs AST/current-state insert contribution resolution.
    SlotWithoutResolution,

    /// A slot placeholder carries its own wrapper context, or a slot-resolution
    /// overlay is active on a template that declares `$children(..)` wrappers.
    /// Both shapes need wrapper-aware view folding (Phase 5).
    SlotWrapperContext,

    /// An insert-contribution node was not consumed by slot composition.
    InsertContribution,

    /// A runtime slot site node requires HIR/runtime lowering.
    RuntimeSlotSite,

    /// An aggregate-output marker appears outside an aggregate-wrapper subtree.
    AggregateOutputOutsideWrapper,

    /// The safety walk detected a recursive child-template path that cannot
    /// produce one finite compile-time value.
    ChildTemplateCycle,
}

/// Mutable context threaded through the view-native fold-safety walk.
///
/// WHAT: bundles the child-template cycle guard and the constant
///       `slot_resolution_active` flag.
///
/// WHY: the walk functions previously took `slot_resolution_active` and
///      `visiting` as separate parameters. Bundling them into one context keeps
///      the parameter list readable. The `visiting` set is owned (not borrowed)
///      so the shared wrapper-safety helpers can receive the same cycle stack
///      without double-reference issues.
struct ViewNativeWalkContext {
    visiting_templates: HashSet<TemplateIrId>,
    slot_resolution_active: bool,
    expression_overlay_stack: Vec<TemplateOverlaySetId>,
}

impl ViewNativeWalkContext {
    /// Resolves the expression-overlay stack at one child or wrapper boundary.
    ///
    /// WHAT: mirrors the fold walk's root-first stack transitions for
    ///       below-Composed references.
    /// WHY: wrapper eligibility must classify effective expressions with the
    ///      same outer overrides that the eventual fold or handoff consumes.
    fn expression_stack_for_boundary(
        &self,
        phase: TemplateTirPhase,
        overlay_set_id: TemplateOverlaySetId,
    ) -> Vec<TemplateOverlaySetId> {
        if phase.is_at_least(TemplateTirPhase::Composed) {
            let mut stack = self.expression_overlay_stack.clone();
            stack.push(overlay_set_id);
            stack
        } else {
            self.expression_overlay_stack.clone()
        }
    }
}

struct VirtualWrapperNodeSafetyContext<'a> {
    in_aggregate_wrapper: bool,
    fill_target_key: Option<&'a SlotKey>,
    walk: &'a mut ViewNativeWalkContext,
    string_table: &'a StringTable,
}

/// Validates every current-store authority reachable from a fold boundary.
///
/// WHAT: checks roots, nodes, templates, wrapper sets, overlays and runtime
///       slot-plan nodes without deciding whether any shape is foldable.
/// WHY: eligibility walkers may stop at the first valid fallback and the fold
///      walker may skip untaken branches, empty loops or cached roots. This
///      pass keeps required authority validation independent from those choices.
///
/// A `None` view describes the direct store-local fold path. When a view is
/// available, it still validates exact store and overlay identity for nested
/// references.
pub(crate) enum FoldAuthorityResult {
    Valid(FoldAuthorityToken),
    ChildTemplateCycle,
}

/// The only fold modes that preparation may authorize.
///
/// WHAT: keeps read-only folding, supported direct folding, and semantic
///       non-folding distinct instead of treating every unlisted reason as
///       eligible.
/// WHY: a fallback reason is not itself proof that the fold path preserves the
///      associated runtime or structural semantics.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PreparedTirFoldDecision {
    ReadOnly,
    Direct,
    SemanticFallback(TirFoldFallbackReason),
}

#[derive(Clone)]
pub(crate) struct FoldAuthorityToken {}

/// Authority and shape facts prepared for one top-level view fold.
///
/// WHAT: carries the exhaustive authority result alongside the read-only
///       eligibility decision so finalization can fold without preflighting the
///       same graph a second time.
/// WHY: authority belongs to `FoldAuthorityWalk`; the eligibility walkers only
///      inspect shapes after this preparation has completed.
pub(crate) struct PreparedTirViewFold {
    root: TemplateIrId,
    phase: TemplateTirPhase,
    overlay_set_id: TemplateOverlaySetId,
    authority: FoldAuthorityResult,
    decision: PreparedTirFoldDecision,
}

impl PreparedTirViewFold {
    pub(crate) fn read_only_safe(&self) -> bool {
        matches!(self.decision, PreparedTirFoldDecision::ReadOnly)
    }

    pub(crate) fn fold_eligible(&self) -> bool {
        !matches!(self.decision, PreparedTirFoldDecision::SemanticFallback(_))
    }

    pub(crate) fn fallback_reason(&self) -> Option<TirFoldFallbackReason> {
        match self.decision {
            PreparedTirFoldDecision::SemanticFallback(reason) => Some(reason),
            PreparedTirFoldDecision::ReadOnly | PreparedTirFoldDecision::Direct => None,
        }
    }

    pub(crate) fn validate_identity(
        &self,
        view: &TirView<'_>,
        store: &TemplateIrStore,
    ) -> Result<(), CompilerError> {
        if self.root != view.root_ref() {
            return Err(CompilerError::compiler_error(format!(
                "TIR fold preparation root {} does not match supplied view root {}.",
                self.root,
                view.root_ref()
            )));
        }
        if self.phase != view.phase() {
            return Err(CompilerError::compiler_error(format!(
                "TIR fold preparation phase {} does not match supplied view phase {}.",
                self.phase,
                view.phase()
            )));
        }
        if self.overlay_set_id != view.overlay_set_id() {
            return Err(CompilerError::compiler_error(format!(
                "TIR fold preparation overlay set {} does not match supplied view overlay set {}.",
                self.overlay_set_id,
                view.overlay_set_id()
            )));
        }
        let _ = store;

        Ok(())
    }

    pub(crate) fn into_authority(self) -> FoldAuthorityResult {
        self.authority
    }
}

fn validate_view_fold_authority(
    view: &TirView<'_>,
    store: &TemplateIrStore,
) -> Result<FoldAuthorityResult, CompilerError> {
    let root = view.root_ref();
    validate_tir_fold_authority(Some(view), store, root)
}

pub(crate) fn prepare_tir_view_fold(
    view: &TirView<'_>,
    store: &TemplateIrStore,
    string_table: &StringTable,
) -> Result<PreparedTirViewFold, CompilerError> {
    let authority = validate_view_fold_authority(view, store)?;
    let view_native_fallback =
        classify_view_native_fold_safety_after_authority(view, store, string_table)?;
    let read_only_safe = if view.overlay_set()?.is_empty() {
        let mut visiting = HashSet::new();
        template_root_is_read_only_fold_safe(
            store,
            Some(view),
            view,
            view.root_ref(),
            false,
            &mut visiting,
        )?
    } else {
        false
    };

    let decision = match view_native_fallback {
        None if read_only_safe => PreparedTirFoldDecision::ReadOnly,
        None => PreparedTirFoldDecision::Direct,
        Some(reason) if direct_fold_preserves(reason) => PreparedTirFoldDecision::Direct,
        Some(reason) => PreparedTirFoldDecision::SemanticFallback(reason),
    };

    Ok(PreparedTirViewFold {
        root: view.root_ref(),
        phase: view.phase(),
        overlay_set_id: view.overlay_set_id(),
        authority,
        decision,
    })
}

fn direct_fold_preserves(reason: TirFoldFallbackReason) -> bool {
    matches!(reason, TirFoldFallbackReason::SlotWithoutResolution)
}

pub(crate) fn validate_tir_fold_authority(
    view: Option<&TirView<'_>>,
    store: &TemplateIrStore,
    template_id: TemplateIrId,
) -> Result<FoldAuthorityResult, CompilerError> {
    // A structural child below Composed intentionally has no `TirView`, but
    // its descendants still resolve through the same module-local store.
    let mut walk = FoldAuthorityWalk {
        visiting_templates: HashSet::new(),
        visiting_nodes: HashSet::new(),
        visiting_slot_plans: HashSet::new(),
        child_template_cycle: false,
    };

    walk.validate_root(store, template_id, view)?;

    if walk.child_template_cycle {
        Ok(FoldAuthorityResult::ChildTemplateCycle)
    } else {
        Ok(FoldAuthorityResult::Valid(FoldAuthorityToken {}))
    }
}

struct FoldAuthorityWalk {
    visiting_templates: HashSet<TemplateIrId>,
    visiting_nodes: HashSet<TemplateIrNodeId>,
    visiting_slot_plans: HashSet<TemplateSlotPlanId>,
    child_template_cycle: bool,
}

impl FoldAuthorityWalk {
    fn validate_root(
        &mut self,
        store: &TemplateIrStore,
        template_id: TemplateIrId,
        view: Option<&TirView<'_>>,
    ) -> Result<(), CompilerError> {
        self.validate_template(store, template_id, view)
    }

    fn validate_template(
        &mut self,
        store: &TemplateIrStore,
        template_id: TemplateIrId,
        view: Option<&TirView<'_>>,
    ) -> Result<(), CompilerError> {
        if !self.visiting_templates.insert(template_id) {
            // Recursive child-template references are semantic cycle
            // re-entry. The first visit already validates this template's
            // current-store authority.
            self.child_template_cycle = true;
            return Ok(());
        }

        let result = (|| {
            let template = store
                .get_template(template_id)
                .ok_or_else(|| missing_template_error(template_id))?;

            self.validate_template_identity(store, template_id, template.root, view)?;
            self.validate_node_exists(store, template.root)?;

            if let Some(wrapper_set_id) = template.conditional_child_wrapper_set {
                self.validate_wrapper_set(store, wrapper_set_id, view)?;
            }

            if let Some(slot_plan_id) = template.runtime_slot_plan {
                self.validate_slot_plan(store, slot_plan_id, view)?;
            }

            self.validate_node(store, template.root, view)
        })();

        self.visiting_templates.remove(&template_id);
        result
    }

    fn validate_template_identity(
        &self,
        store: &TemplateIrStore,
        template_id: TemplateIrId,
        root_node_id: TemplateIrNodeId,
        view: Option<&TirView<'_>>,
    ) -> Result<(), CompilerError> {
        if let Some(view) = view {
            if view.root_ref() != template_id {
                return Err(CompilerError::compiler_error(format!(
                    "TIR fold safety: view root {} does not match walked template {}.",
                    view.root_ref(),
                    template_id
                )));
            }

            let view_template = view.root_template()?;
            if view_template.root != root_node_id {
                return Err(CompilerError::compiler_error(format!(
                    "TIR fold safety: view root {} does not match supplied template root node {}.",
                    view.root_ref(),
                    root_node_id
                )));
            }

            let overlay_set = view.overlay_set()?;
            validate_overlay_set_dimensions(view.store(), view.overlay_set_id(), overlay_set)?;
            return Ok(());
        }

        let template = store
            .get_template(template_id)
            .ok_or_else(|| missing_template_error(template_id))?;
        if template.root != root_node_id {
            return Err(CompilerError::compiler_error(format!(
                "TIR fold safety: root for template {} does not match supplied template root node {}.",
                template_id, root_node_id
            )));
        }

        Ok(())
    }

    fn validate_node_exists(
        &self,
        store: &TemplateIrStore,
        node_id: TemplateIrNodeId,
    ) -> Result<(), CompilerError> {
        store
            .get_node(node_id)
            .ok_or_else(|| missing_node_error(node_id))
            .map(|_| ())
    }

    fn validate_node(
        &mut self,
        store: &TemplateIrStore,
        node_id: TemplateIrNodeId,
        view: Option<&TirView<'_>>,
    ) -> Result<(), CompilerError> {
        if !self.visiting_nodes.insert(node_id) {
            return Err(CompilerError::compiler_error(format!(
                "TIR fold safety: node {} is recursively referenced directly.",
                node_id
            )));
        }

        let result = (|| {
            let node = store
                .get_node(node_id)
                .ok_or_else(|| missing_node_error(node_id))?;

            match &node.kind {
                TemplateIrNodeKind::Sequence { children } => {
                    for child in children {
                        self.validate_node(store, *child, view)?;
                    }
                }

                TemplateIrNodeKind::ChildTemplate {
                    reference,
                    occurrence_id,
                } => {
                    if let Some(view) = view
                        && let Some(context) = view.effective_wrapper_context(*occurrence_id)?
                        && let Some(wrapper_set_ref) = context.inherited_wrapper_set
                    {
                        self.validate_wrapper_set(store, wrapper_set_ref, Some(view))?;
                    }

                    let child_template_id = reference.root;

                    let child_view = if reference.phase.is_at_least(TemplateTirPhase::Composed) {
                        self.validate_overlay_reference(store, reference.overlay_set_id)?;
                        self.child_view_for_reference(store, view, reference)?
                    } else {
                        None
                    };

                    self.validate_template(store, child_template_id, child_view.as_ref())?;
                }

                TemplateIrNodeKind::Slot { placeholder } => {
                    for wrapper_set_id in [
                        placeholder.applied_child_wrapper_set,
                        placeholder.child_wrapper_set,
                    ]
                    .into_iter()
                    .flatten()
                    {
                        self.validate_wrapper_set(store, wrapper_set_id, view)?;
                    }

                    if let Some(view) = view
                        && let Some(resolution) =
                            view.effective_slot_resolution(placeholder.occurrence_id)?
                        && let TirSlotResolutionKind::Resolved { sources } = &resolution.kind
                    {
                        for source in sources {
                            let source_view = view.child_view(
                                *source,
                                TemplateTirPhase::Composed,
                                TemplateOverlaySetId::empty(),
                            )?;
                            self.validate_template(store, *source, Some(&source_view))?;
                        }
                    }
                }

                TemplateIrNodeKind::InsertContribution { template } => {
                    // An insert contribution carries a local helper template,
                    // not a child occurrence that inherits the walked root's
                    // view identity. Validate its own root while leaving the
                    // parent view out of the nested template identity check.
                    self.validate_template(store, *template, None)?;
                }

                TemplateIrNodeKind::BranchChain { branches, fallback } => {
                    for branch in branches {
                        self.validate_node(store, branch.body, view)?;
                    }
                    if let Some(fallback) = fallback {
                        self.validate_node(store, *fallback, view)?;
                    }
                }

                TemplateIrNodeKind::Loop {
                    body,
                    aggregate_wrapper,
                    ..
                } => {
                    self.validate_node(store, *body, view)?;
                    if let Some(aggregate_wrapper) = aggregate_wrapper {
                        self.validate_node(store, *aggregate_wrapper, view)?;
                    }
                }

                TemplateIrNodeKind::RuntimeSlotSite { plan, .. } => {
                    self.validate_slot_plan(store, *plan, view)?;
                }

                TemplateIrNodeKind::Text { .. }
                | TemplateIrNodeKind::DynamicExpression { .. }
                | TemplateIrNodeKind::AggregateOutput
                | TemplateIrNodeKind::LoopControl { .. } => {}
            }

            Ok(())
        })();

        self.visiting_nodes.remove(&node_id);
        result
    }

    fn validate_slot_plan(
        &mut self,
        store: &TemplateIrStore,
        slot_plan_id: TemplateSlotPlanId,
        view: Option<&TirView<'_>>,
    ) -> Result<(), CompilerError> {
        if !self.visiting_slot_plans.insert(slot_plan_id) {
            return Ok(());
        }

        let result = (|| {
            let slot_plan = store
                .get_slot_plan(slot_plan_id)
                .ok_or_else(|| missing_slot_plan_error(slot_plan_id))?;

            for source in &slot_plan.contribution_sources {
                self.validate_node(store, source.render_root, view)?;
            }

            for site in &slot_plan.slot_sites {
                for piece in &site.render_plan.pieces {
                    if let TemplateSlotSiteRenderPiece::Render(node_id) = piece {
                        self.validate_node(store, *node_id, view)?;
                    }
                }
            }

            Ok(())
        })();

        self.visiting_slot_plans.remove(&slot_plan_id);
        result
    }

    fn validate_wrapper_set(
        &mut self,
        store: &TemplateIrStore,
        wrapper_set_id: TemplateWrapperSetId,
        view: Option<&TirView<'_>>,
    ) -> Result<(), CompilerError> {
        let wrapper_set = store
            .get_wrapper_set(wrapper_set_id)
            .ok_or_else(|| missing_wrapper_set_error(wrapper_set_id))?;

        for wrapper in &wrapper_set.wrappers {
            let wrapper_view = if wrapper.phase.is_at_least(TemplateTirPhase::Composed) {
                self.validate_overlay_reference(store, wrapper.overlay_set_id)?;
                self.wrapper_view_for_reference(store, view, wrapper)?
            } else {
                None
            };
            self.validate_template(store, wrapper.root, wrapper_view.as_ref())?;
        }

        Ok(())
    }

    fn validate_overlay_reference(
        &self,
        store: &TemplateIrStore,
        overlay_set_id: TemplateOverlaySetId,
    ) -> Result<(), CompilerError> {
        validate_overlay_set_authority(store, overlay_set_id)
    }

    fn child_view_for_reference<'store>(
        &self,
        store: &'store TemplateIrStore,
        view: Option<&TirView<'store>>,
        reference: &TemplateTirChildReference,
    ) -> Result<Option<TirView<'store>>, CompilerError> {
        if reference.phase.is_at_least(TemplateTirPhase::Composed) {
            if let Some(view) = view {
                return view
                    .child_view(reference.root, reference.phase, reference.overlay_set_id)
                    .map(Some);
            }

            return TirView::new(
                store,
                reference.root,
                reference.phase,
                reference.overlay_set_id,
            )
            .map(Some);
        }

        Ok(None)
    }

    fn wrapper_view_for_reference<'store>(
        &self,
        store: &'store TemplateIrStore,
        view: Option<&TirView<'store>>,
        reference: &TemplateWrapperReference,
    ) -> Result<Option<TirView<'store>>, CompilerError> {
        if !reference.phase.is_at_least(TemplateTirPhase::Composed) {
            return Ok(None);
        }

        if let Some(view) = view {
            return view
                .child_view(reference.root, reference.phase, reference.overlay_set_id)
                .map(Some);
        }

        TirView::new(
            store,
            reference.root,
            reference.phase,
            reference.overlay_set_id,
        )
        .map(Some)
    }
}

fn classify_view_native_fold_safety_after_authority(
    view: &TirView<'_>,
    store: &TemplateIrStore,
    string_table: &StringTable,
) -> Result<Option<TirFoldFallbackReason>, CompilerError> {
    let root = view.root_ref();
    let overlay_set = view.overlay_set()?;

    let has_expression_overlay = overlay_set.expression_overrides.is_some();
    let has_slot_overlay = overlay_set.slot_resolution.is_some();

    // Expression overrides require at least Finalized so the normalized
    // expression payload is stable before folding reads it. Keep the result
    // pending until after the structural walk so malformed nested authority
    // still propagates instead of being hidden by this valid fallback reason.
    let expression_overlay_below_finalized =
        has_expression_overlay && !view.phase().is_at_least(TemplateTirPhase::Finalized);

    // When no overlay dimension is active, the view behaves as an empty-overlay
    // view. The view-native fold walker handles empty-overlay views correctly
    // (every overlay lookup returns `None`, so it falls back to structural
    // reads). Use the view-native fold-safety check so empty-overlay views with
    // branches, loops, and child templates can borrow the live store instead of
    // falling back to current-state materialization.
    let slot_resolution_active = has_slot_overlay;

    let mut walk = ViewNativeWalkContext {
        visiting_templates: HashSet::new(),
        slot_resolution_active,
        expression_overlay_stack: vec![view.overlay_set_id()],
    };

    let reason = check_template_root_view_native_overlay_fold_safety(
        store,
        Some(view),
        view,
        root,
        false,
        &mut walk,
        string_table,
    )?;

    if expression_overlay_below_finalized {
        Ok(Some(TirFoldFallbackReason::ExpressionOverlayBelowFinalized))
    } else {
        Ok(reason)
    }
}

fn template_root_is_linear_fold_safe(
    view: &TirView<'_>,
    store: &TemplateIrStore,
    template_id: TemplateIrId,
    visiting: &mut HashSet<TemplateIrId>,
) -> Result<bool, CompilerError> {
    if !visiting.insert(template_id) {
        return Ok(false);
    }

    let template_ir = store
        .get_template(template_id)
        .ok_or_else(|| missing_template_error(template_id))?;

    let is_linear = tir_node_is_linear_fold_safe(view, store, template_ir.root, visiting);
    visiting.remove(&template_id);
    is_linear
}

fn tir_node_is_linear_fold_safe(
    view: &TirView<'_>,
    store: &TemplateIrStore,
    node_id: TemplateIrNodeId,
    visiting: &mut HashSet<TemplateIrId>,
) -> Result<bool, CompilerError> {
    let node = store
        .get_node(node_id)
        .ok_or_else(|| missing_node_error(node_id))?;

    let is_linear = match &node.kind {
        TemplateIrNodeKind::Sequence { children } => {
            let mut is_linear = true;
            for child in children {
                is_linear &= tir_node_is_linear_fold_safe(view, store, *child, visiting)?;
            }
            is_linear
        }

        // Reactive segments must stay on the structured handoff path until
        // folded output can carry equivalent subscription metadata.
        TemplateIrNodeKind::Text { .. } => store.node_reactive_subscription(node_id).is_none(),

        TemplateIrNodeKind::DynamicExpression {
            reactive_subscription,
            ..
        } => reactive_subscription.is_none(),

        TemplateIrNodeKind::AggregateOutput | TemplateIrNodeKind::LoopControl { .. } => false,

        TemplateIrNodeKind::BranchChain { branches, fallback } => {
            for branch in branches {
                tir_node_is_linear_fold_safe(view, store, branch.body, visiting)?;
            }
            if let Some(fallback) = fallback {
                tir_node_is_linear_fold_safe(view, store, *fallback, visiting)?;
            }
            false
        }

        TemplateIrNodeKind::Loop {
            body,
            aggregate_wrapper,
            ..
        } => {
            tir_node_is_linear_fold_safe(view, store, *body, visiting)?;
            if let Some(aggregate_wrapper) = aggregate_wrapper {
                tir_node_is_linear_fold_safe(view, store, *aggregate_wrapper, visiting)?;
            }
            false
        }

        TemplateIrNodeKind::ChildTemplate { reference, .. } => {
            template_root_is_linear_fold_safe(view, store, reference.root, visiting)?;
            false
        }

        TemplateIrNodeKind::InsertContribution { template } => {
            template_root_is_linear_fold_safe(view, store, *template, visiting)?;
            false
        }

        TemplateIrNodeKind::Slot { .. } => false,

        TemplateIrNodeKind::RuntimeSlotSite { .. } => false,
    };

    Ok(is_linear)
}

/// Checks one template root for view-native overlay fold safety, returning the
/// first named rejection reason or `None` when the root is safe.
///
/// WHAT: mirrors `template_root_is_read_only_fold_safe` but allows expression
///       and slot overlays on the root view. Composed-or-later children enter
///       their exact views, while below-Composed children use the structural
///       fold path without consuming their recorded overlay.
///
/// The walk context carries the cycle guard (`visiting`) and the constant
/// `slot_resolution_active` flag.
fn check_template_root_view_native_overlay_fold_safety(
    store: &TemplateIrStore,
    view: Option<&TirView<'_>>,
    module_view: &TirView<'_>,
    template_id: TemplateIrId,
    in_aggregate_wrapper: bool,
    walk: &mut ViewNativeWalkContext,
    string_table: &StringTable,
) -> Result<Option<TirFoldFallbackReason>, CompilerError> {
    // A template already on the visiting stack would recurse indefinitely in
    // the fold walker, which has no child-template cycle guard.
    let template_ref = template_id;
    if !walk.visiting_templates.insert(template_ref) {
        return Ok(Some(TirFoldFallbackReason::ChildTemplateCycle));
    }

    let result = (|| {
        let template = store
            .get_template(template_id)
            .ok_or_else(|| missing_template_error(template_id))?;

        // Runtime slot plans require HIR/runtime lowering, not compile-time folding.
        if template.runtime_slot_plan.is_some() {
            return Ok(Some(TirFoldFallbackReason::RuntimeSlotPlan));
        }

        // Conditional child wrappers are folded through a virtual wrapper path that
        // does not push synthetic nodes into the store. Keep the gate matched to
        // the shapes that path can fold so fallback handles still-unsupported
        // wrapper subtrees while malformed authority propagates as an error.
        if let Some(wrapper_set_id) = template.conditional_child_wrapper_set
            && !wrapper_set_is_virtual_fold_safe(
                store,
                view,
                module_view,
                wrapper_set_id,
                walk,
                string_table,
            )?
        {
            return Ok(Some(TirFoldFallbackReason::UnsafeWrapperTree));
        }

        // Slot overlays on templates that declare `$children(..)` wrappers need
        // wrapper context while folding resolved slot sources. Keep these shapes on
        // the current-state fallback until slot-source folding can expand wrapper
        // context locally.
        if walk.slot_resolution_active && template.summary.wrapper_count > 0 {
            return Ok(Some(TirFoldFallbackReason::SlotWrapperContext));
        }

        check_tir_node_view_native_overlay_fold_safety(
            store,
            view,
            module_view,
            template.root,
            in_aggregate_wrapper,
            walk,
            string_table,
        )
    })();
    walk.visiting_templates.remove(&template_ref);
    result
}

/// Checks one TIR node subtree for view-native overlay fold safety, returning
/// the first named rejection reason or `None` when the subtree is safe.
///
/// WHAT: the view-native fold walker reads effective expressions and slot
///       resolutions from `TirView` during folding, so branches, loops, and
///       child templates are safe as long as they don't require store mutation.
///       Reactive segments, insert contributions, runtime slot sites, and
///       aggregate-output markers outside wrapper context are rejected.
///
/// The walk context carries the constant `slot_resolution_active` flag and
/// the cycle guard.
fn check_tir_node_view_native_overlay_fold_safety(
    store: &TemplateIrStore,
    view: Option<&TirView<'_>>,
    module_view: &TirView<'_>,
    node_id: TemplateIrNodeId,
    in_aggregate_wrapper: bool,
    walk: &mut ViewNativeWalkContext,
    string_table: &StringTable,
) -> Result<Option<TirFoldFallbackReason>, CompilerError> {
    let node = store
        .get_node(node_id)
        .ok_or_else(|| missing_node_error(node_id))?;

    match &node.kind {
        TemplateIrNodeKind::Sequence { children } => {
            for child in children {
                if let Some(reason) = check_tir_node_view_native_overlay_fold_safety(
                    store,
                    view,
                    module_view,
                    *child,
                    in_aggregate_wrapper,
                    walk,
                    string_table,
                )? {
                    return Ok(Some(reason));
                }
            }
            Ok(None)
        }

        // Reactive segments must stay on the structured handoff path.
        TemplateIrNodeKind::Text { .. } => {
            if store.node_reactive_subscription(node_id).is_some() {
                Ok(Some(TirFoldFallbackReason::ReactiveSegment))
            } else {
                Ok(None)
            }
        }

        TemplateIrNodeKind::DynamicExpression {
            reactive_subscription,
            ..
        } => {
            if reactive_subscription.is_some() {
                Ok(Some(TirFoldFallbackReason::ReactiveSegment))
            } else {
                Ok(None)
            }
        }

        TemplateIrNodeKind::Slot { placeholder } => {
            if !walk.slot_resolution_active {
                return Ok(Some(TirFoldFallbackReason::SlotWithoutResolution));
            }

            // Slot-local wrapper context changes how resolved sources are
            // rendered. Keep these shapes on the current-state fallback until
            // slot-source folding can expand wrapper context locally.
            if slot_placeholder_has_wrapper_context(placeholder) {
                Ok(Some(TirFoldFallbackReason::SlotWrapperContext))
            } else {
                Ok(None)
            }
        }

        TemplateIrNodeKind::BranchChain { branches, fallback } => {
            for branch in branches {
                if let Some(reason) = check_tir_node_view_native_overlay_fold_safety(
                    store,
                    view,
                    module_view,
                    branch.body,
                    in_aggregate_wrapper,
                    walk,
                    string_table,
                )? {
                    return Ok(Some(reason));
                }
            }
            if let Some(fallback_id) = fallback
                && let Some(reason) = check_tir_node_view_native_overlay_fold_safety(
                    store,
                    view,
                    module_view,
                    *fallback_id,
                    in_aggregate_wrapper,
                    walk,
                    string_table,
                )?
            {
                return Ok(Some(reason));
            }
            Ok(None)
        }

        TemplateIrNodeKind::Loop {
            body,
            aggregate_wrapper,
            ..
        } => {
            if let Some(reason) = check_tir_node_view_native_overlay_fold_safety(
                store,
                view,
                module_view,
                *body,
                false,
                walk,
                string_table,
            )? {
                return Ok(Some(reason));
            }
            if let Some(wrapper_id) = aggregate_wrapper
                && let Some(reason) = check_tir_node_view_native_overlay_fold_safety(
                    store,
                    view,
                    module_view,
                    *wrapper_id,
                    true,
                    walk,
                    string_table,
                )?
            {
                return Ok(Some(reason));
            }
            Ok(None)
        }

        TemplateIrNodeKind::ChildTemplate {
            reference,
            occurrence_id,
            ..
        } => {
            // Wrapper-context overlays apply inherited `$children(..)` wrappers
            // at the child occurrence boundary. Resolve them from the active
            // parent view before any child-view transition. They are safe when
            // the wrappers are virtual-fold-safe and carry no unsupported modes.
            let effective_wrapper_context = view
                .map(|view| view.effective_wrapper_context(*occurrence_id))
                .transpose()?
                .flatten();
            if let Some(view) = view
                && let Some(context) = effective_wrapper_context
                && !wrapper_context_is_view_native_fold_safe(
                    store,
                    view,
                    context,
                    walk,
                    string_table,
                )?
            {
                return Ok(Some(TirFoldFallbackReason::WrapperContextOverlay));
            }

            let child_template_id = reference.root;

            // Below-Composed children fold from their structural roots. Their
            // recorded overlay ID is not consumed by that path, so it must not
            // become an authority requirement here.
            if !reference.phase.is_at_least(TemplateTirPhase::Composed) {
                let saved_slot_resolution_active = walk.slot_resolution_active;
                walk.slot_resolution_active = false;
                let result = check_template_root_view_native_overlay_fold_safety(
                    store,
                    None,
                    module_view,
                    child_template_id,
                    in_aggregate_wrapper,
                    walk,
                    string_table,
                );
                walk.slot_resolution_active = saved_slot_resolution_active;
                return result;
            }

            // Composed-or-later children retain exact overlay authority and get
            // a child view so expression overrides and slot resolution within
            // the child's subtree are visible during the safety walk.
            let child_view = match view {
                Some(view) => {
                    view.child_view(reference.root, reference.phase, reference.overlay_set_id)?
                }
                None => module_view.child_view(
                    reference.root,
                    reference.phase,
                    reference.overlay_set_id,
                )?,
            };
            let child_has_slot_resolution = child_view.slot_resolution_overlay()?.is_some();
            // Update slot-resolution activity for the exact child view so slots
            // do not inherit the parent's overlay dimensions.
            let saved_slot_resolution_active = walk.slot_resolution_active;
            let child_expression_overlay_stack =
                walk.expression_stack_for_boundary(reference.phase, reference.overlay_set_id);
            let saved_expression_overlay_stack = std::mem::replace(
                &mut walk.expression_overlay_stack,
                child_expression_overlay_stack,
            );
            walk.slot_resolution_active = child_has_slot_resolution;
            let result = check_template_root_view_native_overlay_fold_safety(
                store,
                Some(&child_view),
                module_view,
                child_template_id,
                in_aggregate_wrapper,
                walk,
                string_table,
            );
            walk.slot_resolution_active = saved_slot_resolution_active;
            walk.expression_overlay_stack = saved_expression_overlay_stack;
            result
        }

        // Loop-control signals are safe: the fold walker just returns them.
        TemplateIrNodeKind::LoopControl { .. } => Ok(None),

        // AggregateOutput markers are valid only inside aggregate wrapper
        // subtrees, where the wrapper fold path consumes them.
        TemplateIrNodeKind::AggregateOutput => {
            if in_aggregate_wrapper {
                Ok(None)
            } else {
                Ok(Some(TirFoldFallbackReason::AggregateOutputOutsideWrapper))
            }
        }

        // Insert contributions should have been consumed by slot composition.
        // Validate the referenced local helper before preserving the ordinary
        // insert-contribution fallback for a well-formed store.
        TemplateIrNodeKind::InsertContribution { .. } => {
            Ok(Some(TirFoldFallbackReason::InsertContribution))
        }

        // Runtime slot sites require HIR/runtime lowering.
        TemplateIrNodeKind::RuntimeSlotSite { .. } => {
            Ok(Some(TirFoldFallbackReason::RuntimeSlotSite))
        }
    }
}

fn slot_placeholder_has_wrapper_context(placeholder: &TirSlotPlaceholder) -> bool {
    placeholder.applied_child_wrapper_set.is_some()
        || placeholder.child_wrapper_set.is_some()
        || placeholder.skip_parent_child_wrappers
}

fn validate_overlay_set_dimensions(
    store: &TemplateIrStore,
    overlay_set_id: TemplateOverlaySetId,
    overlay_set: &TemplateOverlaySet,
) -> Result<(), CompilerError> {
    if let Some(overlay_id) = overlay_set.expression_overrides
        && store.expression_overlay(overlay_id).is_none()
    {
        return Err(missing_overlay_dimension_error(
            overlay_set_id,
            "expression",
            overlay_id,
        ));
    }

    if let Some(overlay_id) = overlay_set.slot_resolution
        && store.slot_resolution_overlay(overlay_id).is_none()
    {
        return Err(missing_overlay_dimension_error(
            overlay_set_id,
            "slot-resolution",
            overlay_id,
        ));
    }

    if let Some(overlay_id) = overlay_set.wrapper_context
        && store.wrapper_context_overlay(overlay_id).is_none()
    {
        return Err(missing_overlay_dimension_error(
            overlay_set_id,
            "wrapper-context",
            overlay_id,
        ));
    }

    Ok(())
}

fn validate_overlay_set_authority(
    store: &TemplateIrStore,
    overlay_set_id: TemplateOverlaySetId,
) -> Result<(), CompilerError> {
    let overlay_set = store
        .overlay_set(overlay_set_id)
        .ok_or_else(|| missing_overlay_set_error(overlay_set_id))?;
    validate_overlay_set_dimensions(store, overlay_set_id, overlay_set)
}

fn missing_template_error(template_id: TemplateIrId) -> CompilerError {
    CompilerError::compiler_error(format!(
        "TIR fold safety: template {} is not present in the module store.",
        template_id
    ))
}

fn missing_node_error(node_id: TemplateIrNodeId) -> CompilerError {
    CompilerError::compiler_error(format!(
        "TIR fold safety: node {} is not present in the module store.",
        node_id
    ))
}

fn missing_wrapper_set_error(wrapper_set_id: TemplateWrapperSetId) -> CompilerError {
    CompilerError::compiler_error(format!(
        "TIR fold safety: wrapper set {} is not present in the module store.",
        wrapper_set_id
    ))
}

fn missing_slot_plan_error(slot_plan_id: TemplateSlotPlanId) -> CompilerError {
    CompilerError::compiler_error(format!(
        "TIR fold safety: slot plan {} is not present in the module store.",
        slot_plan_id
    ))
}

fn missing_overlay_set_error(overlay_set_id: TemplateOverlaySetId) -> CompilerError {
    CompilerError::compiler_error(format!(
        "TIR fold safety: overlay set {} is not present in the module store.",
        overlay_set_id
    ))
}

fn missing_overlay_dimension_error(
    overlay_set_id: TemplateOverlaySetId,
    dimension: &str,
    overlay_id: impl std::fmt::Display,
) -> CompilerError {
    CompilerError::compiler_error(format!(
        "TIR fold safety: overlay set {} references missing {} overlay {}.",
        overlay_set_id, dimension, overlay_id
    ))
}

// -------------------------
//  Read-only (snapshot-free) fold safety
// -------------------------

/// Checks one template root for read-only fold safety, crossing into same-store
/// child templates recursively.
fn template_root_is_read_only_fold_safe(
    store: &TemplateIrStore,
    view: Option<&TirView<'_>>,
    module_view: &TirView<'_>,
    template_id: TemplateIrId,
    in_aggregate_wrapper: bool,
    visiting: &mut HashSet<TemplateIrId>,
) -> Result<bool, CompilerError> {
    // A template already on the visiting stack would recurse indefinitely in
    // the fold walker, which has no child-template cycle guard.
    if !visiting.insert(template_id) {
        return Ok(false);
    }

    let template = store
        .get_template(template_id)
        .ok_or_else(|| missing_template_error(template_id))?;

    // Runtime slot plans require HIR/runtime lowering, not compile-time folding.
    if template.runtime_slot_plan.is_some() {
        return Ok(false);
    }

    // Conditional child wrappers are folded through a virtual wrapper path that
    // does not push synthetic nodes into the store. Keep the gate matched to
    // the shapes that path can fold so fallback handles still-unsupported
    // wrapper subtrees while malformed authority propagates as an error.
    if let Some(wrapper_set_id) = template.conditional_child_wrapper_set {
        let mut wrapper_walk = ViewNativeWalkContext {
            visiting_templates: visiting.iter().copied().collect(),
            slot_resolution_active: false,
            expression_overlay_stack: view
                .map(|view| vec![view.overlay_set_id()])
                .unwrap_or_default(),
        };
        let string_table = StringTable::new();
        if !wrapper_set_is_virtual_fold_safe(
            store,
            view,
            module_view,
            wrapper_set_id,
            &mut wrapper_walk,
            &string_table,
        )? {
            return Ok(false);
        }
    }

    let safe = tir_node_is_read_only_fold_safe(
        store,
        view,
        module_view,
        template.root,
        in_aggregate_wrapper,
        visiting,
    );
    visiting.remove(&template_id);
    safe
}

/// Checks one TIR node subtree for read-only fold safety.
fn tir_node_is_read_only_fold_safe(
    store: &TemplateIrStore,
    view: Option<&TirView<'_>>,
    module_view: &TirView<'_>,
    node_id: TemplateIrNodeId,
    in_aggregate_wrapper: bool,
    visiting: &mut HashSet<TemplateIrId>,
) -> Result<bool, CompilerError> {
    let node = store
        .get_node(node_id)
        .ok_or_else(|| missing_node_error(node_id))?;

    match &node.kind {
        TemplateIrNodeKind::Sequence { children } => {
            let mut safe = true;
            for child in children {
                safe &= tir_node_is_read_only_fold_safe(
                    store,
                    view,
                    module_view,
                    *child,
                    in_aggregate_wrapper,
                    visiting,
                )?;
            }
            Ok(safe)
        }

        // Reactive segments must stay on the structured handoff path.
        TemplateIrNodeKind::Text { .. } => Ok(store.node_reactive_subscription(node_id).is_none()),

        TemplateIrNodeKind::DynamicExpression {
            reactive_subscription,
            ..
        } => Ok(reactive_subscription.is_none()),

        TemplateIrNodeKind::ChildTemplate { reference, .. } => {
            // Only same-store child references can be folded read-only.
            let child_template_id = reference.root;
            // The child's overlay set must also be empty. Look it up through
            // the store that backs the parent view.
            if !reference.phase.is_at_least(TemplateTirPhase::Composed) {
                return template_root_is_read_only_fold_safe(
                    store,
                    None,
                    module_view,
                    child_template_id,
                    in_aggregate_wrapper,
                    visiting,
                );
            }

            let child_view = match view {
                Some(view) => {
                    view.child_view(reference.root, reference.phase, reference.overlay_set_id)?
                }
                None => module_view.child_view(
                    reference.root,
                    reference.phase,
                    reference.overlay_set_id,
                )?,
            };
            // Even when a child overlay would make this template ineligible
            // for the read-only shortcut, a missing child remains an authority
            // failure rather than a normal read-only fallback.
            if store.get_template(child_template_id).is_none() {
                return Err(missing_template_error(child_template_id));
            }
            let child_overlay_safe = child_view.overlay_set()?;
            if !child_overlay_safe.is_empty() {
                return Ok(false);
            }

            template_root_is_read_only_fold_safe(
                store,
                Some(&child_view),
                module_view,
                child_template_id,
                in_aggregate_wrapper,
                visiting,
            )
        }

        TemplateIrNodeKind::BranchChain { branches, fallback } => {
            let mut safe = true;
            for branch in branches {
                safe &= tir_node_is_read_only_fold_safe(
                    store,
                    view,
                    module_view,
                    branch.body,
                    in_aggregate_wrapper,
                    visiting,
                )?;
            }
            if let Some(fallback_id) = fallback {
                safe &= tir_node_is_read_only_fold_safe(
                    store,
                    view,
                    module_view,
                    *fallback_id,
                    in_aggregate_wrapper,
                    visiting,
                )?;
            }

            Ok(safe)
        }

        TemplateIrNodeKind::Loop {
            body,
            aggregate_wrapper,
            ..
        } => {
            let mut safe =
                tir_node_is_read_only_fold_safe(store, view, module_view, *body, false, visiting)?;
            if let Some(wrapper_id) = aggregate_wrapper {
                safe &= tir_node_is_read_only_fold_safe(
                    store,
                    view,
                    module_view,
                    *wrapper_id,
                    true,
                    visiting,
                )?;
            }
            Ok(safe)
        }

        // Loop-control signals are safe: the fold walker just returns them.
        TemplateIrNodeKind::LoopControl { .. } => Ok(true),

        // Empty-overlay slots are rejected because read-only folding cannot
        // tell whether the slot is genuinely empty or still needs insert
        // contribution resolution. The view-native path only accepts slots
        // with an explicit slot-resolution overlay.
        TemplateIrNodeKind::Slot { .. } => Ok(false),

        // AggregateOutput markers are valid only inside aggregate wrapper
        // subtrees, where the wrapper fold path consumes them.
        TemplateIrNodeKind::AggregateOutput => Ok(in_aggregate_wrapper),

        // Insert contributions should have been consumed by slot composition.
        TemplateIrNodeKind::InsertContribution { template } => {
            template_root_is_read_only_fold_safe(
                store,
                view,
                module_view,
                *template,
                false,
                visiting,
            )?;
            Ok(false)
        }

        // Runtime slot sites require HIR/runtime lowering.
        TemplateIrNodeKind::RuntimeSlotSite { .. } => Ok(false),
    }
}

/// Returns `Ok(true)` when a wrapper-context overlay entry can be folded around
/// a child-template emission without falling back to current-state
/// materialization. Malformed authority returns `Err`; valid unsupported
/// shapes return `Ok(false)`.
/// WHAT: validates the same-store wrapper set referenced by the overlay and
///       walks each wrapper template through the node shapes that the virtual
///       wrapper fold path supports. `IfChildEmits` is safe because the fold
///       helper receives the already-folded child emission and can decide from
///       that structural result. Non-virtual wrapper sets remain on the semantic
///       fallback path because this safety walk is scoped to the current store.
/// WHY: wrapper-context overlays are now a supported overlay dimension, but
///      only virtual wrapper trees are admitted by this safety walk today, so
///      this gate keeps non-virtual shapes on the semantic fallback path.
fn wrapper_context_is_view_native_fold_safe(
    store: &TemplateIrStore,
    view: &TirView<'_>,
    context: &TirWrapperContext,
    walk: &mut ViewNativeWalkContext,
    string_table: &StringTable,
) -> Result<bool, CompilerError> {
    // `$fresh` suppresses parent-applied wrappers, so there is nothing to fold.
    if context.skip_parent_child_wrappers {
        return Ok(true);
    }

    let Some(wrapper_set_ref) = context.inherited_wrapper_set else {
        return Ok(true);
    };

    wrapper_set_is_virtual_fold_safe(store, Some(view), view, wrapper_set_ref, walk, string_table)
}

/// Returns `Ok(true)` when every wrapper in the set can be folded by the
/// virtual conditional-wrapper path. Malformed authority returns `Err`; valid
/// unsupported shapes return `Ok(false)`.
///
/// WHAT: checks same-store identity first, then walks each wrapper template
///       through the exact node shapes supported by
///       `fold_tir_wrapper_around_child_output`.
/// WHY: same-store identity alone is not enough. Slot placeholders with their
///      own wrapper context and slots inside loop aggregate-wrapper subtrees
///      still need the current-state fallback because the virtual wrapper fold
///      does not expand those contexts.
fn wrapper_set_is_virtual_fold_safe(
    store: &TemplateIrStore,
    view: Option<&TirView<'_>>,
    module_view: &TirView<'_>,
    wrapper_set_id: TemplateWrapperSetId,
    walk: &mut ViewNativeWalkContext,
    string_table: &StringTable,
) -> Result<bool, CompilerError> {
    let wrapper_set = store
        .get_wrapper_set(wrapper_set_id)
        .ok_or_else(|| missing_wrapper_set_error(wrapper_set_id))?;
    for wrapper in &wrapper_set.wrappers {
        // Wrapper references own their slot, expression, and nested-wrapper
        // dimensions. Match folding by entering the exact wrapper view before
        // walking its root; Parsed wrappers deliberately keep structural reads.
        let wrapper_view = if wrapper.phase.is_at_least(TemplateTirPhase::Composed) {
            Some(match view {
                Some(view) => {
                    view.child_view(wrapper.root, wrapper.phase, wrapper.overlay_set_id)?
                }
                None => {
                    module_view.child_view(wrapper.root, wrapper.phase, wrapper.overlay_set_id)?
                }
            })
        } else {
            None
        };
        let wrapper_expression_overlay_stack =
            walk.expression_stack_for_boundary(wrapper.phase, wrapper.overlay_set_id);
        let saved_expression_overlay_stack = std::mem::replace(
            &mut walk.expression_overlay_stack,
            wrapper_expression_overlay_stack,
        );
        let result = wrapper_template_is_virtual_fold_safe(
            store,
            wrapper_view.as_ref(),
            module_view,
            wrapper.root,
            false,
            walk,
            string_table,
        );
        walk.expression_overlay_stack = saved_expression_overlay_stack;
        if !result? {
            return Ok(false);
        }
    }

    Ok(true)
}

fn wrapper_template_is_virtual_fold_safe(
    store: &TemplateIrStore,
    view: Option<&TirView<'_>>,
    module_view: &TirView<'_>,
    template_id: TemplateIrId,
    in_aggregate_wrapper: bool,
    walk: &mut ViewNativeWalkContext,
    string_table: &StringTable,
) -> Result<bool, CompilerError> {
    let template_ref = template_id;
    if !walk.visiting_templates.insert(template_ref) {
        return Ok(false);
    }

    let safe = (|| {
        let template = store
            .get_template(template_id)
            .ok_or_else(|| missing_template_error(template_id))?;

        if template.runtime_slot_plan.is_some()
            || matches!(
                template.kind,
                crate::compiler_frontend::ast::templates::template::TemplateType::SlotInsert(_)
            )
        {
            return Ok(false);
        }

        // The classification owner resolves effective dynamic expressions,
        // branch selectors, loop headers, and nested expression templates.
        if !wrapper_expression_tree_is_const_evaluable(
            store,
            view,
            module_view,
            template_id,
            template.root,
            &walk.expression_overlay_stack,
        )? {
            return Ok(false);
        }

        let schema = collect_tir_slot_schema(store, template_id).map_err(|error| {
            CompilerError::compiler_error(format!(
                "TIR fold safety: wrapper slot schema could not be resolved: {error:?}"
            ))
        })?;
        let fill_target_key = schema.loose_fill_target_key();

        let mut node_context = VirtualWrapperNodeSafetyContext {
            in_aggregate_wrapper,
            fill_target_key: fill_target_key.as_ref(),
            walk,
            string_table,
        };
        wrapper_node_is_virtual_fold_safe(
            store,
            view,
            module_view,
            template.root,
            &mut node_context,
        )
    })();

    walk.visiting_templates.remove(&template_ref);
    safe
}

fn wrapper_expression_tree_is_const_evaluable(
    store: &TemplateIrStore,
    view: Option<&TirView<'_>>,
    module_view: &TirView<'_>,
    template_id: TemplateIrId,
    root_node_id: TemplateIrNodeId,
    expression_overlay_stack: &[TemplateOverlaySetId],
) -> Result<bool, CompilerError> {
    let structural_view;
    let expression_view = match view {
        Some(view) => view,
        None => {
            structural_view = module_view.child_view(
                template_id,
                TemplateTirPhase::Parsed,
                TemplateOverlaySetId::empty(),
            )?;
            &structural_view
        }
    };

    match tir_view_subtree_is_const_evaluable_value_with_expression_stack(
        expression_view,
        store,
        root_node_id,
        &[],
        expression_overlay_stack,
    ) {
        Ok(is_const) => Ok(is_const),
        Err(TemplateError::Infrastructure(error)) => Err(*error),
        Err(TemplateError::Diagnostic(diagnostic)) => Err(CompilerError::compiler_error(format!(
            "TIR fold safety: wrapper expression classification produced a source diagnostic: {diagnostic:?}"
        ))),
    }
}

/// Checks one non-injected resolved slot source through the exact source view
/// used by folding (`Composed` plus the canonical empty overlay).
fn resolved_slot_source_is_virtual_fold_safe(
    module_view: &TirView<'_>,
    source: TemplateIrId,
    walk: &mut ViewNativeWalkContext,
    string_table: &StringTable,
) -> Result<bool, CompilerError> {
    let source_store = module_view.store();
    let source_template = source_store
        .get_template(source)
        .ok_or_else(|| missing_template_error(source))?;

    if matches!(
        source_template.kind,
        crate::compiler_frontend::ast::templates::template::TemplateType::SlotInsert(_)
    ) {
        return Ok(false);
    }

    let source_view = module_view.child_view(
        source,
        TemplateTirPhase::Composed,
        TemplateOverlaySetId::empty(),
    )?;
    let source_expression_overlay_stack = walk
        .expression_stack_for_boundary(TemplateTirPhase::Composed, TemplateOverlaySetId::empty());
    let saved_expression_overlay_stack = std::mem::replace(
        &mut walk.expression_overlay_stack,
        source_expression_overlay_stack,
    );
    let result = (|| {
        if !wrapper_expression_tree_is_const_evaluable(
            source_store,
            Some(&source_view),
            module_view,
            source,
            source_template.root,
            &walk.expression_overlay_stack,
        )? {
            return Ok(false);
        }
        let saved_slot_resolution_active = walk.slot_resolution_active;
        walk.slot_resolution_active = false;
        let result = check_template_root_view_native_overlay_fold_safety(
            source_store,
            Some(&source_view),
            module_view,
            source,
            false,
            walk,
            string_table,
        );
        walk.slot_resolution_active = saved_slot_resolution_active;
        Ok(result?.is_none())
    })();
    walk.expression_overlay_stack = saved_expression_overlay_stack;
    result
}

fn wrapper_node_is_virtual_fold_safe(
    store: &TemplateIrStore,
    view: Option<&TirView<'_>>,
    module_view: &TirView<'_>,
    node_id: TemplateIrNodeId,
    context: &mut VirtualWrapperNodeSafetyContext<'_>,
) -> Result<bool, CompilerError> {
    let in_aggregate_wrapper = context.in_aggregate_wrapper;
    let fill_target_key = context.fill_target_key;
    let node = store
        .get_node(node_id)
        .ok_or_else(|| missing_node_error(node_id))?;

    match &node.kind {
        TemplateIrNodeKind::Sequence { children } => {
            let mut safe = true;
            for child in children {
                safe &=
                    wrapper_node_is_virtual_fold_safe(store, view, module_view, *child, context)?;
            }
            Ok(safe)
        }

        TemplateIrNodeKind::Text { .. } => Ok(store.node_reactive_subscription(node_id).is_none()),

        TemplateIrNodeKind::DynamicExpression {
            reactive_subscription,
            ..
        } => Ok(reactive_subscription.is_none()),

        TemplateIrNodeKind::Slot { placeholder } => {
            if in_aggregate_wrapper || slot_placeholder_has_wrapper_context(placeholder) {
                return Ok(false);
            }

            // Injection wins over overlay sources at the exact loose-fill
            // target, so do not inspect a source list that folding ignores.
            if fill_target_key.is_some_and(|key| placeholder.key == *key) {
                return Ok(true);
            }

            if let Some(view) = view
                && let Some(resolution) =
                    view.effective_slot_resolution(placeholder.occurrence_id)?
                && let TirSlotResolutionKind::Resolved { sources } = &resolution.kind
            {
                for source in sources {
                    if !resolved_slot_source_is_virtual_fold_safe(
                        module_view,
                        *source,
                        context.walk,
                        context.string_table,
                    )? {
                        return Ok(false);
                    }
                }
            }

            Ok(true)
        }

        TemplateIrNodeKind::ChildTemplate {
            reference,
            occurrence_id,
            ..
        } => {
            let child_template_id = reference.root;
            // Nested virtual-wrapper children can carry their own occurrence
            // context. Validate it through the existing wrapper-context owner
            // before deciding that the child subtree is virtual-fold-safe.
            if let Some(view) = view
                && let Some(occurrence_context) = view.effective_wrapper_context(*occurrence_id)?
                && !wrapper_context_is_view_native_fold_safe(
                    store,
                    view,
                    occurrence_context,
                    context.walk,
                    context.string_table,
                )?
            {
                return Ok(false);
            }

            // Construct the exact child view only once its phase makes the
            // recorded overlay authoritative. A structural below-Composed
            // child must not inherit the wrapper's slot or context dimensions.
            let child_view = if reference.phase.is_at_least(TemplateTirPhase::Composed) {
                Some(match view {
                    Some(view) => {
                        view.child_view(reference.root, reference.phase, reference.overlay_set_id)?
                    }
                    None => module_view.child_view(
                        reference.root,
                        reference.phase,
                        reference.overlay_set_id,
                    )?,
                })
            } else {
                None
            };

            let child_has_slot_resolution = child_view
                .as_ref()
                .map(|view| view.slot_resolution_overlay())
                .transpose()?
                .flatten()
                .is_some();
            let saved_slot_resolution_active = context.walk.slot_resolution_active;
            let child_expression_overlay_stack = context
                .walk
                .expression_stack_for_boundary(reference.phase, reference.overlay_set_id);
            let saved_expression_overlay_stack = std::mem::replace(
                &mut context.walk.expression_overlay_stack,
                child_expression_overlay_stack,
            );
            context.walk.slot_resolution_active = child_has_slot_resolution;

            let result = wrapper_template_is_virtual_fold_safe(
                store,
                child_view.as_ref(),
                module_view,
                child_template_id,
                in_aggregate_wrapper,
                context.walk,
                context.string_table,
            );
            context.walk.slot_resolution_active = saved_slot_resolution_active;
            context.walk.expression_overlay_stack = saved_expression_overlay_stack;
            result
        }

        TemplateIrNodeKind::BranchChain { branches, fallback } => {
            if in_aggregate_wrapper {
                return Ok(false);
            }

            let mut safe = true;
            for branch in branches {
                safe &= wrapper_node_is_virtual_fold_safe(
                    store,
                    view,
                    module_view,
                    branch.body,
                    context,
                )?;
            }
            if let Some(fallback_id) = fallback {
                safe &= wrapper_node_is_virtual_fold_safe(
                    store,
                    view,
                    module_view,
                    *fallback_id,
                    context,
                )?;
            }
            Ok(safe)
        }

        TemplateIrNodeKind::Loop {
            body,
            aggregate_wrapper,
            ..
        } => {
            if in_aggregate_wrapper {
                return Ok(false);
            }

            let original_in_aggregate_wrapper = context.in_aggregate_wrapper;
            context.in_aggregate_wrapper = false;
            let mut safe =
                wrapper_node_is_virtual_fold_safe(store, view, module_view, *body, context)?;
            context.in_aggregate_wrapper = original_in_aggregate_wrapper;
            if let Some(wrapper_id) = aggregate_wrapper {
                context.in_aggregate_wrapper = true;
                safe &= wrapper_node_is_virtual_fold_safe(
                    store,
                    view,
                    module_view,
                    *wrapper_id,
                    context,
                )?;
                context.in_aggregate_wrapper = original_in_aggregate_wrapper;
            }
            Ok(safe)
        }

        TemplateIrNodeKind::AggregateOutput => Ok(in_aggregate_wrapper),

        TemplateIrNodeKind::InsertContribution { .. }
        | TemplateIrNodeKind::LoopControl { .. }
        | TemplateIrNodeKind::RuntimeSlotSite { .. } => Ok(false),
    }
}
