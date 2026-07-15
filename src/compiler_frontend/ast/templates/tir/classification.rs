//! TIR-backed template shape queries for store-aware classification.
//!
//! WHAT: answers unresolved-slot, escaped-insert, and const-evaluable questions
//! from TIR trees in the module-scoped `TemplateIrStore`.
//!
//! Callers classify a stable registry-backed `TirView` whose root, phase and
//! overlay set carry the authoritative reference identity.
//!
//! WHY: normalization and folding should classify from the TIR root they
//! already trust instead of reconstructing template structure through a
//! compatibility representation.
//!
//! ## Ownership contract
//!
//! Classification remains inside the AST template subsystem and never crosses
//! the HIR boundary.

use std::collections::HashSet;
use std::sync::Arc;

use crate::compiler_frontend::ast::expressions::expression::Expression;
use crate::compiler_frontend::ast::expressions::expression::ExpressionKind;
use crate::compiler_frontend::ast::expressions::expression_rpn::ExpressionRpnItem;
use crate::compiler_frontend::ast::templates::error::TemplateError;
use crate::compiler_frontend::ast::templates::template::Template;
use crate::compiler_frontend::ast::templates::template::{
    TemplateConstValueKind, TemplateSegmentOrigin, TemplateType,
};
use crate::compiler_frontend::ast::templates::template_control_flow::{
    TemplateBranchSelector, TemplateLoopHeader,
};
use crate::compiler_frontend::ast::templates::template_control_flow::{
    collect_option_capture_binding_path, loop_body_const_evaluation_bindings,
};
use crate::compiler_frontend::ast::templates::tir::ids::{
    ExpressionSiteId, TemplateIrId, TemplateIrNodeId,
};
use crate::compiler_frontend::ast::templates::tir::node::{
    TemplateIrNodeKind, TemplateLoopHeaderExpressionSites,
};
use crate::compiler_frontend::ast::templates::tir::overlays::{
    TemplateOverlaySetId, TirSlotResolutionKind,
};
use crate::compiler_frontend::ast::templates::tir::refs::{TemplateRef, TemplateTirChildReference};
use crate::compiler_frontend::ast::templates::tir::store::TemplateIrStore;
use crate::compiler_frontend::ast::templates::tir::view::{TemplateTirPhase, TirView};
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::symbols::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::StringTable;

/// Classification result from one TIR template root.
///
/// WHAT: bundles const-value kind, shape const-evaluability, and escaped-insert
///       detection for one registry-backed template tree.
/// WHY: lets callers classify from TIR once and reuse the combined answer
///      without rebuilding the tree or running separate tree walks.
pub(crate) struct TirTemplateClassification {
    pub(crate) const_value_kind: TemplateConstValueKind,
    pub(crate) shape_const_evaluable: bool,
    pub(crate) has_unresolved_slots: bool,
    pub(crate) has_slot_insertions: bool,
}

#[derive(Clone, Copy)]
enum StringFunctionChildConstPolicy {
    Strict,
    StructuralHeadFunction,
}

struct TirViewConstEvaluationContext<'view, 'store> {
    view: TirView<'view>,
    store: &'store TemplateIrStore,
    string_table: &'view StringTable,
    visiting_templates: HashSet<TemplateRef>,
    string_function_child_policy: StringFunctionChildConstPolicy,
}

// -------------------------
//  Same-store ownership proof
// -------------------------

/// Test-only proof that a preserved TIR reference belongs to a given store.
///
/// WHAT: compares the `TemplateTirReference.store_owner` token against the
/// store's owner token using `Arc::ptr_eq`.
/// WHY: tests need to prove same-store behavior without an unrelated template
/// in another store masking the result.
#[cfg(test)]
pub(crate) fn same_store_tir_id(
    template: &Template,
    store: &TemplateIrStore,
) -> Option<TemplateIrId> {
    let reference = &template.tir_reference;
    if Arc::ptr_eq(&reference.store_owner, &store.owner()) {
        Some(reference.root.template_id)
    } else {
        None
    }
}

/// Read-only unresolved-slot query over an existing same-store TIR subtree.
///
/// WHAT: walks the subtree rooted at `root` (recursing through child templates
///       and nested control-flow bodies) for unresolved `Slot` nodes. The
///       caller already holds a finalized same-store root.
/// WHY: runtime control-flow slot-artifact validation prefers the finalized
///      body root after render-unit preparation.
pub(crate) fn tir_subtree_has_unresolved_slots(
    store: &TemplateIrStore,
    root: TemplateIrNodeId,
) -> bool {
    tir_tree_has_slots(store, root, &mut HashSet::new())
}

/// Classifies an existing effective `TirView`, applying supported overlays.
///
/// WHAT: answers the const-value question by reading expression-bearing sites
///       through `TirView`
///       so finalization classifies the exact effective view produced by
///       expression-overlay normalization.
/// WHY: finalization stores normalized dynamic-expression, branch-selector,
///      and loop-header payloads in registry-owned overlays. Classifying the
///      structural root alone would ignore those effective expressions.
///      Slot-resolution overlays are supported:
///      resolved slots are folded through their source templates at fold time,
///      and missing slots fold to empty. Wrapper-context overlays are supported
///      during folding, but they do not change the classification outcome
///      because inherited wrappers wrap child-template emissions without
///      affecting the parent template's own const-value shape.
pub(crate) fn classify_effective_tir_view_template(
    view: &TirView<'_>,
    store: &TemplateIrStore,
    string_table: &StringTable,
) -> Result<TirTemplateClassification, TemplateError> {
    if !view.phase().is_at_least(TemplateTirPhase::Composed) {
        return Err(TemplateError::from(CompilerError::compiler_error(format!(
            "classify_effective_tir_view_template: root {} is at phase {}, but classification requires Composed or later",
            view.root_ref(),
            view.phase()
        ))));
    }

    if view.root_ref().store_id != store.store_id() {
        return Err(TemplateError::from(CompilerError::compiler_error(format!(
            "classify_effective_tir_view_template: view root {} does not belong to supplied store {}",
            view.root_ref(),
            store.store_id()
        ))));
    }

    let overlay_set = view.overlay_set()?;
    if overlay_set.expression_overrides.is_some()
        && !view.phase().is_at_least(TemplateTirPhase::Finalized)
    {
        return Err(TemplateError::from(CompilerError::compiler_error(format!(
            "classify_effective_tir_view_template: root {} has expression overlays at phase {}, but expression-overlay classification requires Finalized",
            view.root_ref(),
            view.phase()
        ))));
    }

    // Read the root and authoritative kind from the caller-provided store
    // instead of borrowing it again through the registry. Effective
    // classification stays read-only so callers can retain the active fold
    // borrow while classifying a nested template from that same registry store.
    let (store_root, template_kind) = store
        .get_template(view.root_ref().template_id)
        .map(|template| (template.root, template.kind.clone()))
        .ok_or_else(|| {
            TemplateError::from(CompilerError::compiler_error(format!(
                "classify_effective_tir_view_template: root {} is missing from supplied store",
                view.root_ref()
            )))
        })?;

    let shape_const_evaluable =
        tir_view_template_is_const_evaluable_value(view, store, string_table)?;
    let has_unresolved_slots = tir_tree_has_slots(store, store_root, &mut HashSet::new());
    let has_slot_insertions =
        tir_tree_has_slot_insert_children(store, store_root, &mut HashSet::new());

    // Structural `Slot` nodes that are not covered by the view's slot-resolution
    // overlay (or are covered only by `Missing`/`Unresolved` entries) fold to no
    // output per the language rules, so they do not force the runtime handoff
    // wrapper path. Only slots that resolve to actual contribution sources turn
    // a const-evaluable `String` template into a `WrapperTemplate`.
    let has_resolved_slot_sources = if has_unresolved_slots {
        tir_view_has_resolved_slots(view, store, store_root, &mut HashSet::new())?
    } else {
        false
    };

    let const_value_kind = classify_tir_const_value(
        &template_kind,
        store,
        store_root,
        shape_const_evaluable,
        has_resolved_slot_sources,
        has_slot_insertions,
    );

    Ok(TirTemplateClassification {
        const_value_kind,
        shape_const_evaluable,
        has_unresolved_slots,
        has_slot_insertions,
    })
}

/// Refreshes a template kind from a TIR classification result.
///
/// WHAT: updates the generic `String` / `StringFunction` classification while
///       preserving semantic markers (`SlotInsert`, `SlotDefinition`, `Comment`)
///       that must not be overwritten by generic cleanup.
/// WHY: `TemplateIr.kind` is the authoritative post-construction kind owner.
///      The parser-local build state uses the same rule before the durable
///      cache exists, while later refreshes go through the template's single
///      synchronization method.
pub(crate) fn refresh_kind_from_classification(
    kind: &mut TemplateType,
    classification: &TirTemplateClassification,
) {
    if matches!(
        *kind,
        TemplateType::SlotInsert(_) | TemplateType::SlotDefinition(_) | TemplateType::Comment(_)
    ) {
        return;
    }

    *kind = if classification.shape_const_evaluable && !classification.has_slot_insertions {
        TemplateType::String
    } else {
        TemplateType::StringFunction
    };
}

fn classify_tir_const_value(
    template_kind: &TemplateType,
    store: &TemplateIrStore,
    root: TemplateIrNodeId,
    shape_const_evaluable: bool,
    has_resolved_slot_sources: bool,
    has_slot_insertions: bool,
) -> TemplateConstValueKind {
    if tir_tree_is_loop_control_signal(store, root) {
        return TemplateConstValueKind::LoopControlSignal;
    }

    if !shape_const_evaluable {
        return TemplateConstValueKind::NonConst;
    }

    if matches!(template_kind, TemplateType::SlotInsert(_)) {
        // Slot-insert helper templates are compile-time wrapper values. Escaped
        // nested `$insert(...)` children make them NonConst. Fresh TIR makes
        // this check authoritative — the TIR kind is the sole owner.
        if has_slot_insertions {
            return TemplateConstValueKind::NonConst;
        }
        return TemplateConstValueKind::SlotInsertHelper;
    }

    if matches!(template_kind, TemplateType::SlotDefinition(_)) {
        return TemplateConstValueKind::NonConst;
    }

    if !matches!(template_kind, TemplateType::String) {
        return TemplateConstValueKind::NonConst;
    }

    // Resolved slot sources still require wrapper application at fold time.
    // Missing and uncovered slots emit no output, so they don't make the
    // effective value a wrapper on their own.
    if has_resolved_slot_sources {
        return TemplateConstValueKind::WrapperTemplate;
    }

    if has_slot_insertions {
        return TemplateConstValueKind::NonConst;
    }

    TemplateConstValueKind::RenderableString
}

/// Classifies an already-built TIR node as a structural const value.
///
/// WHAT: lets runtime slot planning reuse the node it has just built for HIR
/// handoff instead of building a temporary value solely for the
/// wrapper-rendering policy.
/// WHY: runtime slot contribution sources are not always full `Template`
/// values, but TIR owns their structural constness checks.
pub(crate) fn tir_node_is_const_evaluable_value(
    store: &mut TemplateIrStore,
    node_id: TemplateIrNodeId,
    string_table: &StringTable,
) -> bool {
    tir_tree_is_const_evaluable_standalone_value(store, node_id, string_table, &mut HashSet::new())
}

/// Classifies an already-built TIR node as a structural const value with loop
/// bindings in scope.
///
/// WHAT: walks a store-owned body root with the same binding-aware rules used
/// for freshly built const template control flow.
/// WHY: render-unit preparation installs control-flow bodies into TIR before
/// const-required validation recurses through them; validation can therefore
/// read the finalized TIR body root for the body-level constness predicate.
#[cfg(test)]
pub(crate) fn tir_node_is_const_evaluable_value_with_bindings(
    store: &mut TemplateIrStore,
    node_id: TemplateIrNodeId,
    loop_binding_paths: &[InternedPath],
    string_table: &StringTable,
) -> bool {
    tir_tree_is_const_evaluable_value(
        store,
        node_id,
        loop_binding_paths,
        string_table,
        &mut HashSet::new(),
        StringFunctionChildConstPolicy::StructuralHeadFunction,
    )
}

// -------------------------
//  Recursive tree walkers
// -------------------------

/// Recursively checks whether a TIR subtree contains `Slot` nodes.
///
/// Follows `ChildTemplate` references into the same store with a visited set to
/// prevent infinite recursion on cyclic references.
fn tir_tree_has_slots(
    store: &TemplateIrStore,
    node_id: TemplateIrNodeId,
    visited: &mut HashSet<TemplateIrId>,
) -> bool {
    let Some(node) = store.get_node(node_id) else {
        return false;
    };

    match &node.kind {
        TemplateIrNodeKind::Sequence { children } => children
            .iter()
            .any(|child| tir_tree_has_slots(store, *child, visited)),

        TemplateIrNodeKind::Slot { .. } => true,

        TemplateIrNodeKind::ChildTemplate { reference, .. } => {
            if let Some(template_id) = reference.template_id_in_store(store.store_id()) {
                visit_child_template(store, template_id, visited, tir_tree_has_slots)
            } else {
                false
            }
        }

        TemplateIrNodeKind::InsertContribution { template } => {
            visit_child_template(store, *template, visited, tir_tree_has_slots)
        }

        TemplateIrNodeKind::BranchChain { branches, fallback } => {
            branches
                .iter()
                .any(|branch| tir_tree_has_slots(store, branch.body, visited))
                || fallback
                    .as_ref()
                    .is_some_and(|fallback_id| tir_tree_has_slots(store, *fallback_id, visited))
        }

        TemplateIrNodeKind::Loop {
            body,
            aggregate_wrapper,
            ..
        } => {
            tir_tree_has_slots(store, *body, visited)
                || aggregate_wrapper
                    .as_ref()
                    .is_some_and(|wrapper_id| tir_tree_has_slots(store, *wrapper_id, visited))
        }

        TemplateIrNodeKind::Text { .. }
        | TemplateIrNodeKind::DynamicExpression { .. }
        | TemplateIrNodeKind::AggregateOutput
        | TemplateIrNodeKind::LoopControl { .. }
        | TemplateIrNodeKind::RuntimeSlotSite { .. } => false,
    }
}

/// Recursively checks whether a TIR subtree contains any slot occurrence that is
/// resolved to contribution sources by the given `TirView`.
///
/// WHAT: walks the same structural tree as `tir_tree_has_slots`, but asks the
///       view's slot-resolution overlay for each `Slot` occurrence instead of
///       treating every `Slot` node as unresolved. Returns `Ok(true)` as soon as
///       one occurrence maps to `TirSlotResolutionKind::Resolved`.
/// WHY: unresolved/unfilled slots fold to no output under the language rules,
///      so a const-evaluable template whose structural slots are all uncovered
///      (or covered only by `Missing`/`Unresolved` entries) classifies as a
///      renderable string. Only slots that actually resolve to source templates
///      turn the template into a `WrapperTemplate`.
fn tir_view_has_resolved_slots(
    view: &TirView<'_>,
    store: &TemplateIrStore,
    node_id: TemplateIrNodeId,
    visited: &mut HashSet<TemplateIrId>,
) -> Result<bool, TemplateError> {
    let Some(node) = store.get_node(node_id) else {
        return Ok(false);
    };

    match &node.kind {
        TemplateIrNodeKind::Sequence { children } => {
            for child in children {
                if tir_view_has_resolved_slots(view, store, *child, visited)? {
                    return Ok(true);
                }
            }
            Ok(false)
        }

        TemplateIrNodeKind::Slot { placeholder } => {
            let resolution = view.effective_slot_resolution(placeholder.occurrence_id)?;
            Ok(resolution.is_some_and(|resolved| {
                matches!(resolved.kind, TirSlotResolutionKind::Resolved { .. })
            }))
        }

        TemplateIrNodeKind::ChildTemplate { reference, .. } => {
            if let Some(template_id) = reference.template_id_in_store(store.store_id()) {
                tir_view_visit_child_template(view, store, template_id, visited)
            } else {
                Ok(false)
            }
        }

        TemplateIrNodeKind::InsertContribution { template } => {
            tir_view_visit_child_template(view, store, *template, visited)
        }

        TemplateIrNodeKind::BranchChain { branches, fallback } => {
            for branch in branches {
                if tir_view_has_resolved_slots(view, store, branch.body, visited)? {
                    return Ok(true);
                }
            }
            if let Some(fallback_id) = fallback
                && tir_view_has_resolved_slots(view, store, *fallback_id, visited)?
            {
                return Ok(true);
            }
            Ok(false)
        }

        TemplateIrNodeKind::Loop {
            body,
            aggregate_wrapper,
            ..
        } => {
            if tir_view_has_resolved_slots(view, store, *body, visited)? {
                return Ok(true);
            }
            if let Some(wrapper_id) = aggregate_wrapper
                && tir_view_has_resolved_slots(view, store, *wrapper_id, visited)?
            {
                return Ok(true);
            }
            Ok(false)
        }

        TemplateIrNodeKind::Text { .. }
        | TemplateIrNodeKind::DynamicExpression { .. }
        | TemplateIrNodeKind::AggregateOutput
        | TemplateIrNodeKind::LoopControl { .. }
        | TemplateIrNodeKind::RuntimeSlotSite { .. } => Ok(false),
    }
}

/// Cycle-guarded child-template descent for `tir_view_has_resolved_slots`.
///
/// WHAT: matches `visit_child_template` but carries the view and propagates the
///       `TemplateError` from overlay resolution.
fn tir_view_visit_child_template(
    view: &TirView<'_>,
    store: &TemplateIrStore,
    template_id: TemplateIrId,
    visited: &mut HashSet<TemplateIrId>,
) -> Result<bool, TemplateError> {
    if !visited.insert(template_id) {
        return Ok(false);
    }

    let resolved = if let Some(child_template) = store.get_template(template_id) {
        tir_view_has_resolved_slots(view, store, child_template.root, visited)?
    } else {
        false
    };

    visited.remove(&template_id);
    Ok(resolved)
}

/// Recursively checks whether a TIR subtree contains child templates whose
/// `TemplateType` is `SlotInsert(_)`.
///
/// WHAT: walks `ChildTemplate` nodes (and `InsertContribution` nodes for
///       robustness) and inspects the referenced template's `kind` field to
///       detect escaped `$insert(...)` helpers. `TemplateIr.kind` is the sole
///       post-construction owner of this semantic marker.
/// WHY: TIR trees may represent an escaped insert through either a
///      `ChildTemplate` or `InsertContribution`, so both forms must be checked.
fn tir_tree_has_slot_insert_children(
    store: &TemplateIrStore,
    node_id: TemplateIrNodeId,
    visited: &mut HashSet<TemplateIrId>,
) -> bool {
    let Some(node) = store.get_node(node_id) else {
        return false;
    };

    match &node.kind {
        TemplateIrNodeKind::Sequence { children } => children
            .iter()
            .any(|child| tir_tree_has_slot_insert_children(store, *child, visited)),

        // Both `ChildTemplate` and `InsertContribution` reference a child
        // template. Check whether the child's kind is `SlotInsert(_)` before
        // recursing into the child's root. The kind check happens before the
        // visited-set guard so a shared child is still detected on first
        // encounter; subsequent re-encounters short-circuit via the visited set.
        TemplateIrNodeKind::ChildTemplate { reference, .. } => {
            let Some(template_id) = reference.template_id_in_store(store.store_id()) else {
                return false;
            };
            let is_slot_insert = store
                .get_template(template_id)
                .is_some_and(|child| matches!(child.kind, TemplateType::SlotInsert(_)));

            is_slot_insert
                || visit_child_template(
                    store,
                    template_id,
                    visited,
                    tir_tree_has_slot_insert_children,
                )
        }

        TemplateIrNodeKind::InsertContribution { template } => {
            let is_slot_insert = store
                .get_template(*template)
                .is_some_and(|child| matches!(child.kind, TemplateType::SlotInsert(_)));

            is_slot_insert
                || visit_child_template(
                    store,
                    *template,
                    visited,
                    tir_tree_has_slot_insert_children,
                )
        }

        TemplateIrNodeKind::BranchChain { branches, fallback } => {
            branches
                .iter()
                .any(|branch| tir_tree_has_slot_insert_children(store, branch.body, visited))
                || fallback.as_ref().is_some_and(|fallback_id| {
                    tir_tree_has_slot_insert_children(store, *fallback_id, visited)
                })
        }

        TemplateIrNodeKind::Loop {
            body,
            aggregate_wrapper,
            ..
        } => {
            tir_tree_has_slot_insert_children(store, *body, visited)
                || aggregate_wrapper.as_ref().is_some_and(|wrapper_id| {
                    tir_tree_has_slot_insert_children(store, *wrapper_id, visited)
                })
        }

        TemplateIrNodeKind::Text { .. }
        | TemplateIrNodeKind::DynamicExpression { .. }
        | TemplateIrNodeKind::Slot { .. }
        | TemplateIrNodeKind::AggregateOutput
        | TemplateIrNodeKind::LoopControl { .. }
        | TemplateIrNodeKind::RuntimeSlotSite { .. } => false,
    }
}

fn tir_view_template_is_const_evaluable_value(
    view: &TirView<'_>,
    store: &TemplateIrStore,
    string_table: &StringTable,
) -> Result<bool, TemplateError> {
    // Read the root from the caller-provided store instead of reborrowing it
    // through the registry's RefCell. This keeps classification compatible with
    // the live immutable store borrow held by the view-native fold path.
    let root = store
        .get_template(view.root_ref().template_id)
        .map(|template| template.root)
        .ok_or_else(|| {
            CompilerError::compiler_error(format!(
                "tir_view_template_is_const_evaluable_value: root {} is missing from store",
                view.root_ref()
            ))
        })?;
    let mut context = TirViewConstEvaluationContext {
        view: view.clone(),
        store,
        string_table,
        visiting_templates: HashSet::new(),
        string_function_child_policy: StringFunctionChildConstPolicy::Strict,
    };

    tir_view_child_template_is_const_evaluable_value(&mut context, view.root_ref(), root, &[])
}

/// Checks whether one TIR subtree rooted at `node_id` is a const-evaluable value,
/// reading effective expressions from the supplied `TirView`.
///
/// WHAT: exposes the view-based const-evaluability walker used by
///       `template_control_flow` validation so it can ask the same question for
///       branch bodies, loop bodies, and aggregate wrappers without duplicating
///       the overlay-aware traversal.
/// WHY: keeps the overlay-aware const-evaluability logic in `classification.rs`
///      while letting the validator emit diagnostics at the right source locations.
pub(crate) fn tir_view_subtree_is_const_evaluable_value(
    view: &TirView<'_>,
    store: &TemplateIrStore,
    string_table: &StringTable,
    node_id: TemplateIrNodeId,
    loop_binding_paths: &[InternedPath],
) -> Result<bool, TemplateError> {
    let mut context = TirViewConstEvaluationContext {
        view: view.clone(),
        store,
        string_table,
        visiting_templates: HashSet::new(),
        string_function_child_policy: StringFunctionChildConstPolicy::Strict,
    };

    tir_view_tree_is_const_evaluable_value(&mut context, node_id, loop_binding_paths)
}

/// Classifies an expression through the same effective view used by its TIR node.
pub(crate) fn tir_view_expression_is_const_evaluable_value_with_bindings(
    view: &TirView<'_>,
    store: &TemplateIrStore,
    expression: &Expression,
    loop_binding_paths: &[InternedPath],
    string_table: &StringTable,
) -> Result<bool, TemplateError> {
    let mut context = TirViewConstEvaluationContext {
        view: view.clone(),
        store,
        string_table,
        visiting_templates: HashSet::new(),
        string_function_child_policy: StringFunctionChildConstPolicy::StructuralHeadFunction,
    };

    tir_view_expression_is_const_evaluable(&mut context, expression, loop_binding_paths)
}

/// Checks whether an option-capture scrutinee is decidable from an effective view.
pub(crate) fn tir_view_option_capture_presence_is_const_decidable(
    view: &TirView<'_>,
    store: &TemplateIrStore,
    scrutinee: &Expression,
    loop_binding_paths: &[InternedPath],
    string_table: &StringTable,
) -> Result<bool, TemplateError> {
    let mut context = TirViewConstEvaluationContext {
        view: view.clone(),
        store,
        string_table,
        visiting_templates: HashSet::new(),
        string_function_child_policy: StringFunctionChildConstPolicy::StructuralHeadFunction,
    };

    tir_view_option_capture_presence_is_const_decidable_with_context(
        &mut context,
        scrutinee,
        loop_binding_paths,
    )
}

fn tir_child_template_is_const_evaluable_value(
    store: &mut TemplateIrStore,
    template_id: TemplateIrId,
    root: TemplateIrNodeId,
    loop_binding_paths: &[InternedPath],
    string_table: &StringTable,
    visiting_templates: &mut HashSet<TemplateIrId>,
    string_function_child_policy: StringFunctionChildConstPolicy,
) -> bool {
    if !visiting_templates.insert(template_id) {
        return false;
    }

    let is_const_evaluable = tir_tree_is_const_evaluable_value(
        store,
        root,
        loop_binding_paths,
        string_table,
        visiting_templates,
        string_function_child_policy,
    );

    visiting_templates.remove(&template_id);
    is_const_evaluable
}

fn tir_view_child_template_is_const_evaluable_value(
    context: &mut TirViewConstEvaluationContext<'_, '_>,
    template_ref: TemplateRef,
    root: TemplateIrNodeId,
    loop_binding_paths: &[InternedPath],
) -> Result<bool, TemplateError> {
    if !context.visiting_templates.insert(template_ref) {
        return Ok(false);
    }

    let is_const_evaluable =
        tir_view_tree_is_const_evaluable_value(context, root, loop_binding_paths)?;

    context.visiting_templates.remove(&template_ref);
    Ok(is_const_evaluable)
}

/// Follows one store-qualified child through its exact registry view.
///
/// Same-store children reuse the active store borrow. Foreign children borrow
/// their registered store for the read-only classification walk. Both paths
/// preserve the original root, phase and overlay identity without rebuilding
/// template content.
fn tir_view_qualified_child_is_const_evaluable_value(
    context: &mut TirViewConstEvaluationContext<'_, '_>,
    reference: TemplateTirChildReference,
    expected_store_owner: Option<&Arc<super::store::TemplateIrStoreOwner>>,
    loop_binding_paths: &[InternedPath],
) -> Result<bool, TemplateError> {
    let child_view =
        context
            .view
            .child_view(reference.root, reference.phase, reference.overlay_set_id)?;

    if reference.root.store_id == context.store.store_id() {
        if let Some(expected_store_owner) = expected_store_owner
            && !Arc::ptr_eq(expected_store_owner, &context.store.owner())
        {
            return Ok(false);
        }

        let Some((child_kind, child_root)) = context
            .store
            .get_template(reference.root.template_id)
            .map(|template_ir| (template_ir.kind.clone(), template_ir.root))
        else {
            return Ok(false);
        };
        if matches!(
            context.string_function_child_policy,
            StringFunctionChildConstPolicy::StructuralHeadFunction
        ) && matches!(child_kind, TemplateType::StringFunction)
            && string_function_child_is_structural_const_value(context.store, child_root)
        {
            return Ok(true);
        }

        let parent_view = std::mem::replace(&mut context.view, child_view);
        let result = tir_view_child_template_is_const_evaluable_value(
            context,
            reference.root,
            child_root,
            loop_binding_paths,
        );
        context.view = parent_view;
        return result;
    }

    let child_store = context
        .view
        .registry_ref()
        .store(reference.root.store_id)
        .ok_or_else(|| {
            CompilerError::compiler_error(format!(
                "TIR const classification child store {} is not registered.",
                reference.root.store_id
            ))
        })?;
    if let Some(expected_store_owner) = expected_store_owner
        && !Arc::ptr_eq(expected_store_owner, &child_store.owner())
    {
        return Ok(false);
    }
    let Some((child_kind, child_root)) = child_store
        .get_template(reference.root.template_id)
        .map(|template_ir| (template_ir.kind.clone(), template_ir.root))
    else {
        return Ok(false);
    };
    if matches!(
        context.string_function_child_policy,
        StringFunctionChildConstPolicy::StructuralHeadFunction
    ) && matches!(child_kind, TemplateType::StringFunction)
        && string_function_child_is_structural_const_value(&child_store, child_root)
    {
        return Ok(true);
    }

    let visiting_templates = std::mem::take(&mut context.visiting_templates);
    let mut child_context = TirViewConstEvaluationContext {
        view: child_view,
        store: &child_store,
        string_table: context.string_table,
        visiting_templates,
        string_function_child_policy: context.string_function_child_policy,
    };
    let result = tir_view_child_template_is_const_evaluable_value(
        &mut child_context,
        reference.root,
        child_root,
        loop_binding_paths,
    );
    context.visiting_templates = child_context.visiting_templates;

    result
}

fn tir_tree_is_const_evaluable_value(
    store: &mut TemplateIrStore,
    node_id: TemplateIrNodeId,
    loop_binding_paths: &[InternedPath],
    string_table: &StringTable,
    visiting_templates: &mut HashSet<TemplateIrId>,
    string_function_child_policy: StringFunctionChildConstPolicy,
) -> bool {
    let Some(node_kind) = store.get_node(node_id).map(|node| node.kind.clone()) else {
        return false;
    };

    match node_kind {
        TemplateIrNodeKind::Sequence { children } => children.into_iter().all(|child| {
            tir_tree_is_const_evaluable_value(
                store,
                child,
                loop_binding_paths,
                string_table,
                visiting_templates,
                string_function_child_policy,
            )
        }),

        TemplateIrNodeKind::Text { .. } => {
            // A Text node carrying a reactive subscription in the side table is
            // runtime content: its output must be invalidated when the source
            // changes, so it is not const-evaluable even though the literal text
            // itself is static.
            store.node_reactive_subscription(node_id).is_none()
        }
        TemplateIrNodeKind::Slot { .. }
        | TemplateIrNodeKind::AggregateOutput
        | TemplateIrNodeKind::LoopControl { .. } => true,

        TemplateIrNodeKind::DynamicExpression { expression, .. } => {
            tir_dynamic_expression_is_const_evaluable(
                store,
                &expression,
                loop_binding_paths,
                string_table,
                visiting_templates,
                string_function_child_policy,
            )
        }

        TemplateIrNodeKind::ChildTemplate { reference, .. } => {
            let Some(template_id) = reference.template_id_in_store(store.store_id()) else {
                return false;
            };
            let Some(child_template) = store.get_template(template_id) else {
                return false;
            };

            if matches!(
                string_function_child_policy,
                StringFunctionChildConstPolicy::StructuralHeadFunction
            ) && matches!(child_template.kind, TemplateType::StringFunction)
                && string_function_child_is_structural_const_value(store, child_template.root)
            {
                return true;
            }

            let root = child_template.root;
            tir_child_template_is_const_evaluable_value(
                store,
                template_id,
                root,
                loop_binding_paths,
                string_table,
                visiting_templates,
                string_function_child_policy,
            )
        }

        TemplateIrNodeKind::InsertContribution { template } => {
            let Some(child_template) = store.get_template(template) else {
                return false;
            };

            if matches!(
                string_function_child_policy,
                StringFunctionChildConstPolicy::StructuralHeadFunction
            ) && matches!(child_template.kind, TemplateType::StringFunction)
                && string_function_child_is_structural_const_value(store, child_template.root)
            {
                return true;
            }

            let root = child_template.root;
            tir_child_template_is_const_evaluable_value(
                store,
                template,
                root,
                loop_binding_paths,
                string_table,
                visiting_templates,
                string_function_child_policy,
            )
        }

        TemplateIrNodeKind::BranchChain { branches, fallback } => {
            branches.into_iter().all(|branch| {
                let Some(branch_binding_paths) =
                    branch_selector_const_evaluation_bindings_with_tir_templates(
                        &branch.selector,
                        loop_binding_paths,
                        store,
                        string_table,
                        visiting_templates,
                        string_function_child_policy,
                    )
                else {
                    return false;
                };

                tir_tree_is_const_evaluable_value(
                    store,
                    branch.body,
                    &branch_binding_paths,
                    string_table,
                    visiting_templates,
                    string_function_child_policy,
                )
            }) && fallback.is_none_or(|fallback_id| {
                tir_tree_is_const_evaluable_value(
                    store,
                    fallback_id,
                    loop_binding_paths,
                    string_table,
                    visiting_templates,
                    string_function_child_policy,
                )
            })
        }

        TemplateIrNodeKind::Loop {
            header,
            body,
            aggregate_wrapper,
            ..
        } => {
            if !tir_loop_header_is_const_evaluable_value(
                &header,
                store,
                string_table,
                visiting_templates,
                string_function_child_policy,
            ) {
                return false;
            }

            let body_binding_paths =
                loop_body_const_evaluation_bindings(&header, loop_binding_paths);
            tir_tree_is_const_evaluable_value(
                store,
                body,
                &body_binding_paths,
                string_table,
                visiting_templates,
                string_function_child_policy,
            ) && aggregate_wrapper.is_none_or(|wrapper_id| {
                tir_tree_is_const_evaluable_value(
                    store,
                    wrapper_id,
                    loop_binding_paths,
                    string_table,
                    visiting_templates,
                    string_function_child_policy,
                )
            })
        }

        TemplateIrNodeKind::RuntimeSlotSite { .. } => false,
    }
}

fn tir_view_tree_is_const_evaluable_value(
    context: &mut TirViewConstEvaluationContext<'_, '_>,
    node_id: TemplateIrNodeId,
    loop_binding_paths: &[InternedPath],
) -> Result<bool, TemplateError> {
    // Read the structural node from the store directly. The view is only used
    // for overlay-effective expression lookups, not for node traversal, so
    // this avoids borrowing the store through the registry's RefCell.
    let node_kind = context
        .store
        .get_node(node_id)
        .map(|node| node.kind.clone())
        .ok_or_else(|| {
            CompilerError::compiler_error(format!(
                "tir_view_tree_is_const_evaluable_value: node {} is missing from store",
                node_id
            ))
        })?;

    match node_kind {
        TemplateIrNodeKind::Sequence { children } => {
            for child in children {
                if !tir_view_tree_is_const_evaluable_value(context, child, loop_binding_paths)? {
                    return Ok(false);
                }
            }

            Ok(true)
        }

        TemplateIrNodeKind::Text { .. } => {
            // A Text node carrying a reactive subscription in the side table is
            // runtime content, not a const-evaluable value.
            Ok(context.store.node_reactive_subscription(node_id).is_none())
        }
        TemplateIrNodeKind::Slot { .. }
        | TemplateIrNodeKind::AggregateOutput
        | TemplateIrNodeKind::LoopControl { .. } => Ok(true),

        TemplateIrNodeKind::DynamicExpression {
            expression,
            site_id,
            ..
        } => {
            let effective_expression = context
                .view
                .effective_expression_for_site(site_id)?
                .unwrap_or(expression.as_ref());

            tir_view_expression_is_const_evaluable(
                context,
                effective_expression,
                loop_binding_paths,
            )
        }

        TemplateIrNodeKind::ChildTemplate { reference, .. } => {
            tir_view_qualified_child_is_const_evaluable_value(
                context,
                reference,
                None,
                loop_binding_paths,
            )
        }

        TemplateIrNodeKind::InsertContribution { template } => {
            let Some(child_template) = context.store.get_template(template) else {
                return Ok(false);
            };
            let child_kind = child_template.kind.clone();
            let child_root = child_template.root;

            if matches!(
                context.string_function_child_policy,
                StringFunctionChildConstPolicy::StructuralHeadFunction
            ) && matches!(child_kind, TemplateType::StringFunction)
                && string_function_child_is_structural_const_value(context.store, child_root)
            {
                return Ok(true);
            }

            tir_view_child_template_is_const_evaluable_value(
                context,
                TemplateRef::new(context.store.store_id(), template),
                child_root,
                loop_binding_paths,
            )
        }

        TemplateIrNodeKind::BranchChain { branches, fallback } => {
            for branch in branches {
                let selector = effective_branch_selector_for_view(
                    &context.view,
                    &branch.selector,
                    branch.selector_site_id,
                )?;
                let Some(branch_binding_paths) =
                    tir_view_branch_selector_const_evaluation_bindings(
                        context,
                        &selector,
                        loop_binding_paths,
                    )?
                else {
                    return Ok(false);
                };

                if !tir_view_tree_is_const_evaluable_value(
                    context,
                    branch.body,
                    &branch_binding_paths,
                )? {
                    return Ok(false);
                }
            }

            if let Some(fallback_id) = fallback {
                return tir_view_tree_is_const_evaluable_value(
                    context,
                    fallback_id,
                    loop_binding_paths,
                );
            }

            Ok(true)
        }

        TemplateIrNodeKind::Loop {
            header,
            header_sites,
            body,
            aggregate_wrapper,
            ..
        } => {
            let effective_header =
                effective_loop_header_for_view(&context.view, &header, header_sites)?;
            if !tir_view_loop_header_is_const_evaluable(context, &effective_header)? {
                return Ok(false);
            }

            let body_binding_paths =
                loop_body_const_evaluation_bindings(&effective_header, loop_binding_paths);
            if !tir_view_tree_is_const_evaluable_value(context, body, &body_binding_paths)? {
                return Ok(false);
            }

            if let Some(wrapper_id) = aggregate_wrapper {
                return tir_view_tree_is_const_evaluable_value(
                    context,
                    wrapper_id,
                    loop_binding_paths,
                );
            }

            Ok(true)
        }

        TemplateIrNodeKind::RuntimeSlotSite { .. } => Ok(false),
    }
}

pub(crate) fn effective_branch_selector_for_view(
    view: &TirView<'_>,
    selector: &TemplateBranchSelector,
    site_id: ExpressionSiteId,
) -> Result<TemplateBranchSelector, TemplateError> {
    let Some(expression) = view.effective_expression_for_site(site_id)? else {
        return Ok(selector.clone());
    };

    Ok(match selector {
        TemplateBranchSelector::Bool(_) => TemplateBranchSelector::Bool(expression.clone()),
        TemplateBranchSelector::OptionPresentCapture { pattern, .. } => {
            TemplateBranchSelector::OptionPresentCapture {
                scrutinee: expression.clone(),
                pattern: pattern.clone(),
            }
        }
    })
}

pub(crate) fn effective_loop_header_for_view(
    view: &TirView<'_>,
    header: &TemplateLoopHeader,
    header_sites: TemplateLoopHeaderExpressionSites,
) -> Result<TemplateLoopHeader, TemplateError> {
    Ok(match (header, header_sites) {
        (
            TemplateLoopHeader::Conditional { condition },
            TemplateLoopHeaderExpressionSites::Conditional { condition: site_id },
        ) => TemplateLoopHeader::Conditional {
            condition: Box::new(
                view.effective_expression_for_site(site_id)?
                    .cloned()
                    .unwrap_or_else(|| condition.as_ref().clone()),
            ),
        },

        (
            TemplateLoopHeader::Range { bindings, range },
            TemplateLoopHeaderExpressionSites::Range { start, end, step },
        ) => {
            let mut range = range.as_ref().clone();
            if let Some(expression) = view.effective_expression_for_site(start)? {
                range.start = expression.clone();
            }
            if let Some(expression) = view.effective_expression_for_site(end)? {
                range.end = expression.clone();
            }
            if let Some(step_site_id) = step
                && let Some(expression) = view.effective_expression_for_site(step_site_id)?
            {
                range.step = Some(expression.clone());
            }

            TemplateLoopHeader::Range {
                bindings: bindings.clone(),
                range: Box::new(range),
            }
        }

        (
            TemplateLoopHeader::Collection { bindings, iterable },
            TemplateLoopHeaderExpressionSites::Collection { iterable: site_id },
        ) => TemplateLoopHeader::Collection {
            bindings: bindings.clone(),
            iterable: Box::new(
                view.effective_expression_for_site(site_id)?
                    .cloned()
                    .unwrap_or_else(|| iterable.as_ref().clone()),
            ),
        },

        _ => header.clone(),
    })
}

fn string_function_child_is_structural_const_value(
    store: &TemplateIrStore,
    root: TemplateIrNodeId,
) -> bool {
    let Some(node_kind) = store.get_node(root).map(|node| node.kind.clone()) else {
        return false;
    };

    match node_kind {
        TemplateIrNodeKind::Sequence { children } => children
            .into_iter()
            .all(|child| string_function_child_is_structural_const_value(store, child)),

        TemplateIrNodeKind::DynamicExpression {
            expression, origin, ..
        } => {
            matches!(origin, TemplateSegmentOrigin::Head)
                && matches!(expression.kind, ExpressionKind::FunctionCall { .. })
        }

        TemplateIrNodeKind::Text { .. } => store.node_reactive_subscription(root).is_none(),
        TemplateIrNodeKind::Slot { .. }
        | TemplateIrNodeKind::AggregateOutput
        | TemplateIrNodeKind::LoopControl { .. } => true,

        TemplateIrNodeKind::ChildTemplate { .. }
        | TemplateIrNodeKind::InsertContribution { .. }
        | TemplateIrNodeKind::BranchChain { .. }
        | TemplateIrNodeKind::Loop { .. }
        | TemplateIrNodeKind::RuntimeSlotSite { .. } => false,
    }
}

fn tir_tree_is_const_evaluable_standalone_value(
    store: &mut TemplateIrStore,
    node_id: TemplateIrNodeId,
    string_table: &StringTable,
    visiting_templates: &mut HashSet<TemplateIrId>,
) -> bool {
    let Some(node_kind) = store.get_node(node_id).map(|node| node.kind.clone()) else {
        return false;
    };

    match node_kind {
        TemplateIrNodeKind::Sequence { children } => children.into_iter().all(|child| {
            tir_tree_is_const_evaluable_standalone_value(
                store,
                child,
                string_table,
                visiting_templates,
            )
        }),

        TemplateIrNodeKind::Text { .. } => store.node_reactive_subscription(node_id).is_none(),
        TemplateIrNodeKind::Slot { .. }
        | TemplateIrNodeKind::AggregateOutput
        | TemplateIrNodeKind::LoopControl { .. } => true,

        TemplateIrNodeKind::DynamicExpression { expression, .. } => expression
            .const_value_kind_with_template_classifier(&mut |template| {
                Ok(
                    if tir_embedded_template_is_const_evaluable(
                        store,
                        template,
                        &[],
                        string_table,
                        visiting_templates,
                        StringFunctionChildConstPolicy::Strict,
                    ) {
                        TemplateConstValueKind::RenderableString
                    } else {
                        TemplateConstValueKind::NonConst
                    },
                )
            })
            .is_ok_and(|kind| kind.is_compile_time_value()),

        TemplateIrNodeKind::ChildTemplate { reference, .. } => {
            let Some(template_id) = reference.template_id_in_store(store.store_id()) else {
                return false;
            };
            let Some(root) = store
                .get_template(template_id)
                .map(|template| template.root)
            else {
                return false;
            };

            if !visiting_templates.insert(template_id) {
                return false;
            }

            let is_const_evaluable = tir_tree_is_const_evaluable_standalone_value(
                store,
                root,
                string_table,
                visiting_templates,
            );
            visiting_templates.remove(&template_id);
            is_const_evaluable
        }

        TemplateIrNodeKind::InsertContribution { template } => {
            let Some(root) = store.get_template(template).map(|template| template.root) else {
                return false;
            };

            if !visiting_templates.insert(template) {
                return false;
            }

            let is_const_evaluable = tir_tree_is_const_evaluable_standalone_value(
                store,
                root,
                string_table,
                visiting_templates,
            );
            visiting_templates.remove(&template);
            is_const_evaluable
        }

        TemplateIrNodeKind::BranchChain { .. } | TemplateIrNodeKind::Loop { .. } => {
            tir_tree_is_const_evaluable_value(
                store,
                node_id,
                &[],
                string_table,
                visiting_templates,
                StringFunctionChildConstPolicy::Strict,
            )
        }

        TemplateIrNodeKind::RuntimeSlotSite { .. } => false,
    }
}

fn tir_dynamic_expression_is_const_evaluable(
    store: &mut TemplateIrStore,
    expression: &Expression,
    loop_binding_paths: &[InternedPath],
    string_table: &StringTable,
    visiting_templates: &mut HashSet<TemplateIrId>,
    string_function_child_policy: StringFunctionChildConstPolicy,
) -> bool {
    expression_is_const_evaluable_with_bindings_and_tir_templates(
        expression,
        loop_binding_paths,
        store,
        string_table,
        visiting_templates,
        string_function_child_policy,
    )
}

/// View-aware expression constness for TIR payloads.
///
/// This mirrors the compile-time value shapes owned by `Expression`, but routes
/// every embedded template through its exact registry-qualified TIR reference.
/// Keeping the recursion here ensures composite values retain the exact TIR
/// identity and overlay context of embedded templates.
fn tir_view_expression_is_const_evaluable(
    context: &mut TirViewConstEvaluationContext<'_, '_>,
    expression: &Expression,
    loop_binding_paths: &[InternedPath],
) -> Result<bool, TemplateError> {
    match &expression.kind {
        ExpressionKind::Int(_)
        | ExpressionKind::Float(_)
        | ExpressionKind::StringSlice(_)
        | ExpressionKind::Bool(_)
        | ExpressionKind::Char(_) => Ok(true),

        #[cfg(test)]
        ExpressionKind::Path(_) => Ok(true),

        ExpressionKind::Reference(path) => Ok(loop_binding_paths.iter().any(|known| known == path)),

        #[cfg(test)]
        ExpressionKind::FallibleCarrierConstruct { value, .. } => {
            tir_view_expression_is_const_evaluable(context, value, loop_binding_paths)
        }

        ExpressionKind::Coerced { value, .. } => {
            tir_view_expression_is_const_evaluable(context, value, loop_binding_paths)
        }

        ExpressionKind::Runtime(rpn) => {
            for item in &rpn.items {
                if let ExpressionRpnItem::Operand(operand) = item
                    && !tir_view_expression_is_const_evaluable(
                        context,
                        operand,
                        loop_binding_paths,
                    )?
                {
                    return Ok(false);
                }
            }

            Ok(true)
        }

        ExpressionKind::Template(template) => {
            let reference = &template.tir_reference;
            let child_reference = TemplateTirChildReference::new(
                reference.root,
                reference.phase,
                reference.overlay_set_id,
            );

            tir_view_qualified_child_is_const_evaluable_value(
                context,
                child_reference,
                Some(&reference.store_owner),
                loop_binding_paths,
            )
        }

        ExpressionKind::ChoiceConstruct { fields, .. } | ExpressionKind::StructInstance(fields) => {
            for field in fields {
                if !tir_view_expression_is_const_evaluable(
                    context,
                    &field.value,
                    loop_binding_paths,
                )? {
                    return Ok(false);
                }
            }

            Ok(true)
        }

        ExpressionKind::Collection(items) => {
            for item in items {
                if !tir_view_expression_is_const_evaluable(context, item, loop_binding_paths)? {
                    return Ok(false);
                }
            }

            Ok(true)
        }

        ExpressionKind::Range(start, end) => {
            Ok(
                tir_view_expression_is_const_evaluable(context, start, loop_binding_paths)?
                    && tir_view_expression_is_const_evaluable(context, end, loop_binding_paths)?,
            )
        }

        ExpressionKind::NoValue
        | ExpressionKind::OptionNone
        | ExpressionKind::Copy(_)
        | ExpressionKind::Function(_)
        | ExpressionKind::FunctionCall { .. }
        | ExpressionKind::FieldAccess { .. }
        | ExpressionKind::MethodCall { .. }
        | ExpressionKind::CollectionBuiltinCall { .. }
        | ExpressionKind::MapBuiltinCall { .. }
        | ExpressionKind::HandledFallibleFunctionCall { .. }
        | ExpressionKind::HandledFallibleHostFunctionCall { .. }
        | ExpressionKind::Cast(_)
        | ExpressionKind::HandledFallibleExpression { .. }
        | ExpressionKind::OptionPropagation { .. }
        | ExpressionKind::HostFunctionCall { .. }
        | ExpressionKind::RuntimeTemplateHandoff(_)
        | ExpressionKind::RuntimeSlotApplicationHandoff(_)
        | ExpressionKind::MapLiteral(_)
        | ExpressionKind::StructDefinition(_)
        | ExpressionKind::ValueBlock { .. } => Ok(false),
    }
}

fn tir_view_branch_selector_const_evaluation_bindings(
    context: &mut TirViewConstEvaluationContext<'_, '_>,
    selector: &TemplateBranchSelector,
    inherited_binding_paths: &[InternedPath],
) -> Result<Option<Vec<InternedPath>>, TemplateError> {
    let mut binding_paths = inherited_binding_paths.to_vec();

    match selector {
        TemplateBranchSelector::Bool(condition) => {
            if !tir_view_expression_is_const_evaluable(context, condition, inherited_binding_paths)?
            {
                return Ok(None);
            }
        }

        TemplateBranchSelector::OptionPresentCapture { scrutinee, pattern } => {
            if !tir_view_option_capture_presence_is_const_decidable_with_context(
                context,
                scrutinee,
                inherited_binding_paths,
            )? {
                return Ok(None);
            }

            collect_option_capture_binding_path(pattern, &mut binding_paths);
        }
    }

    Ok(Some(binding_paths))
}

fn tir_view_option_capture_presence_is_const_decidable_with_context(
    context: &mut TirViewConstEvaluationContext<'_, '_>,
    scrutinee: &Expression,
    loop_binding_paths: &[InternedPath],
) -> Result<bool, TemplateError> {
    match &scrutinee.kind {
        ExpressionKind::OptionNone => Ok(true),

        ExpressionKind::Coerced { value, .. } => {
            tir_view_expression_is_const_evaluable(context, value, loop_binding_paths)
        }

        ExpressionKind::Reference(path) => Ok(loop_binding_paths.iter().any(|known| known == path)),

        _ => Ok(false),
    }
}

fn tir_view_loop_header_is_const_evaluable(
    context: &mut TirViewConstEvaluationContext<'_, '_>,
    header: &TemplateLoopHeader,
) -> Result<bool, TemplateError> {
    match header {
        TemplateLoopHeader::Conditional { condition } => {
            tir_view_expression_is_const_evaluable(context, condition, &[])
        }

        TemplateLoopHeader::Range { range, .. } => {
            if !tir_view_expression_is_const_evaluable(context, &range.start, &[])?
                || !tir_view_expression_is_const_evaluable(context, &range.end, &[])?
            {
                return Ok(false);
            }

            if let Some(step) = &range.step {
                return tir_view_expression_is_const_evaluable(context, step, &[]);
            }

            Ok(true)
        }

        TemplateLoopHeader::Collection { iterable, .. } => {
            tir_view_expression_is_const_evaluable(context, iterable, &[])
        }
    }
}

/// Store-aware binding-aware expression const classifier for the TIR tree walker.
///
/// WHAT: threads the module-scoped store through the complete expression walk
///       and follows embedded templates through their exact same-store TIR
///       references.
/// WHY: TIR structural recursion must follow the authoritative same-store
///      reference when it encounters a nested template expression.
fn expression_is_const_evaluable_with_bindings_and_tir_templates(
    expression: &Expression,
    loop_binding_paths: &[InternedPath],
    store: &mut TemplateIrStore,
    string_table: &StringTable,
    visiting_templates: &mut HashSet<TemplateIrId>,
    string_function_child_policy: StringFunctionChildConstPolicy,
) -> bool {
    match &expression.kind {
        ExpressionKind::Reference(path) => loop_binding_paths.iter().any(|known| known == path),

        #[cfg(test)]
        ExpressionKind::FallibleCarrierConstruct { value, .. } => {
            expression_is_const_evaluable_with_bindings_and_tir_templates(
                value,
                loop_binding_paths,
                store,
                string_table,
                visiting_templates,
                string_function_child_policy,
            )
        }

        ExpressionKind::Coerced { value, .. } => {
            expression_is_const_evaluable_with_bindings_and_tir_templates(
                value,
                loop_binding_paths,
                store,
                string_table,
                visiting_templates,
                string_function_child_policy,
            )
        }

        ExpressionKind::Runtime(rpn) => rpn.items.iter().all(|item| match item {
            ExpressionRpnItem::Operand(operand) => {
                expression_is_const_evaluable_with_bindings_and_tir_templates(
                    operand,
                    loop_binding_paths,
                    store,
                    string_table,
                    visiting_templates,
                    string_function_child_policy,
                )
            }
            ExpressionRpnItem::Operator { .. } => true,
        }),

        ExpressionKind::Template(template) => tir_embedded_template_is_const_evaluable(
            store,
            template,
            loop_binding_paths,
            string_table,
            visiting_templates,
            string_function_child_policy,
        ),

        ExpressionKind::Int(_)
        | ExpressionKind::Float(_)
        | ExpressionKind::StringSlice(_)
        | ExpressionKind::Bool(_)
        | ExpressionKind::Char(_) => true,

        #[cfg(test)]
        ExpressionKind::Path(_) => true,

        ExpressionKind::ChoiceConstruct { fields, .. } | ExpressionKind::StructInstance(fields) => {
            fields.iter().all(|field| {
                expression_is_const_evaluable_with_bindings_and_tir_templates(
                    &field.value,
                    loop_binding_paths,
                    store,
                    string_table,
                    visiting_templates,
                    string_function_child_policy,
                )
            })
        }

        ExpressionKind::Collection(items) => items.iter().all(|item| {
            expression_is_const_evaluable_with_bindings_and_tir_templates(
                item,
                loop_binding_paths,
                store,
                string_table,
                visiting_templates,
                string_function_child_policy,
            )
        }),

        ExpressionKind::Range(start, end) => {
            expression_is_const_evaluable_with_bindings_and_tir_templates(
                start,
                loop_binding_paths,
                store,
                string_table,
                visiting_templates,
                string_function_child_policy,
            ) && expression_is_const_evaluable_with_bindings_and_tir_templates(
                end,
                loop_binding_paths,
                store,
                string_table,
                visiting_templates,
                string_function_child_policy,
            )
        }

        ExpressionKind::NoValue
        | ExpressionKind::OptionNone
        | ExpressionKind::Copy(_)
        | ExpressionKind::Function(_)
        | ExpressionKind::FunctionCall { .. }
        | ExpressionKind::FieldAccess { .. }
        | ExpressionKind::MethodCall { .. }
        | ExpressionKind::CollectionBuiltinCall { .. }
        | ExpressionKind::MapBuiltinCall { .. }
        | ExpressionKind::HandledFallibleFunctionCall { .. }
        | ExpressionKind::HandledFallibleHostFunctionCall { .. }
        | ExpressionKind::Cast(_)
        | ExpressionKind::HandledFallibleExpression { .. }
        | ExpressionKind::OptionPropagation { .. }
        | ExpressionKind::HostFunctionCall { .. }
        | ExpressionKind::RuntimeTemplateHandoff(_)
        | ExpressionKind::RuntimeSlotApplicationHandoff(_)
        | ExpressionKind::MapLiteral(_)
        | ExpressionKind::StructDefinition(_)
        | ExpressionKind::ValueBlock { .. } => false,
    }
}

fn tir_loop_header_is_const_evaluable_value(
    header: &TemplateLoopHeader,
    store: &mut TemplateIrStore,
    string_table: &StringTable,
    visiting_templates: &mut HashSet<TemplateIrId>,
    string_function_child_policy: StringFunctionChildConstPolicy,
) -> bool {
    let mut expression_is_const = |expression: &Expression| {
        expression_is_const_evaluable_with_bindings_and_tir_templates(
            expression,
            &[],
            store,
            string_table,
            visiting_templates,
            string_function_child_policy,
        )
    };

    match header {
        TemplateLoopHeader::Conditional { condition } => expression_is_const(condition),
        TemplateLoopHeader::Range { range, .. } => {
            expression_is_const(&range.start)
                && expression_is_const(&range.end)
                && range.step.as_ref().is_none_or(&mut expression_is_const)
        }
        TemplateLoopHeader::Collection { iterable, .. } => expression_is_const(iterable),
    }
}

/// Store-aware branch-selector binding resolver for the TIR tree walker.
///
/// WHAT: classifies the branch condition and option-capture scrutinee through
///       `expression_is_const_evaluable_with_bindings_and_tir_templates` so
///       nested template expressions inside selectors use their authoritative
///       TIR references.
/// WHY: the old no-store helper classified the non-template leaf through a
///      no-store expression constness method. Routing selectors through the
///      store-aware TIR expression classifier keeps the TIR tree walker
///      consistent with the validation and kind-refresh paths.
fn branch_selector_const_evaluation_bindings_with_tir_templates(
    selector: &TemplateBranchSelector,
    inherited_binding_paths: &[InternedPath],
    store: &mut TemplateIrStore,
    string_table: &StringTable,
    visiting_templates: &mut HashSet<TemplateIrId>,
    string_function_child_policy: StringFunctionChildConstPolicy,
) -> Option<Vec<InternedPath>> {
    let mut binding_paths = inherited_binding_paths.to_vec();

    match selector {
        TemplateBranchSelector::Bool(condition) => {
            if !expression_is_const_evaluable_with_bindings_and_tir_templates(
                condition,
                inherited_binding_paths,
                store,
                string_table,
                visiting_templates,
                string_function_child_policy,
            ) {
                return None;
            }
        }

        TemplateBranchSelector::OptionPresentCapture { scrutinee, pattern } => {
            if !option_capture_presence_is_const_decidable_with_tir_templates(
                scrutinee,
                inherited_binding_paths,
                store,
                string_table,
                visiting_templates,
                string_function_child_policy,
            ) {
                return None;
            }

            collect_option_capture_binding_path(pattern, &mut binding_paths);
        }
    }

    Some(binding_paths)
}

/// Store-aware option-capture presence classifier for the TIR tree walker.
///
/// WHAT: routes the coerced scrutinee through the store-aware TIR expression
///       classifier so nested templates in the scrutinee classify through TIR.
fn option_capture_presence_is_const_decidable_with_tir_templates(
    scrutinee: &Expression,
    loop_binding_paths: &[InternedPath],
    store: &mut TemplateIrStore,
    string_table: &StringTable,
    visiting_templates: &mut HashSet<TemplateIrId>,
    string_function_child_policy: StringFunctionChildConstPolicy,
) -> bool {
    match &scrutinee.kind {
        ExpressionKind::OptionNone => true,

        // `T` in a `T?` context is represented as an explicit coercion. The
        // wrapped value is the present payload available to the then branch.
        ExpressionKind::Coerced { value, .. } => {
            expression_is_const_evaluable_with_bindings_and_tir_templates(
                value,
                loop_binding_paths,
                store,
                string_table,
                visiting_templates,
                string_function_child_policy,
            )
        }

        // Const loop bindings are resolved per iteration during folding, so
        // validation can accept the reference when the loop source itself is const.
        ExpressionKind::Reference(path) => loop_binding_paths.iter().any(|known| known == path),

        _ => false,
    }
}

fn tir_embedded_template_is_const_evaluable(
    store: &mut TemplateIrStore,
    template: &Template,
    loop_binding_paths: &[InternedPath],
    string_table: &StringTable,
    visiting_templates: &mut HashSet<TemplateIrId>,
    string_function_child_policy: StringFunctionChildConstPolicy,
) -> bool {
    let reference = &template.tir_reference;
    if reference.root.store_id != store.store_id()
        || !Arc::ptr_eq(&reference.store_owner, &store.owner())
        || reference.overlay_set_id != TemplateOverlaySetId::empty()
    {
        return false;
    }
    let template_id = reference.root.template_id;
    let Some(root) = store
        .get_template(template_id)
        .map(|template_ir| template_ir.root)
    else {
        return false;
    };

    tir_child_template_is_const_evaluable_value(
        store,
        template_id,
        root,
        loop_binding_paths,
        string_table,
        visiting_templates,
        string_function_child_policy,
    )
}

fn tir_tree_is_loop_control_signal(store: &TemplateIrStore, node_id: TemplateIrNodeId) -> bool {
    store
        .get_node(node_id)
        .is_some_and(|node| matches!(node.kind, TemplateIrNodeKind::LoopControl { .. }))
}

/// Follows a `ChildTemplate` or `InsertContribution` reference into the
/// referenced template's root, guarding against cycles with a visited set.
///
/// WHAT: shared recursion helper for the two tree walkers. The `walker` closure
///       lets each caller keep its own node-kind check without duplicating the
///       child-reference resolution logic.
/// WHY: both slot and insert-contribution walkers need to descend into the same
///      referenced template's root; extracting this step avoids duplicating the
///      visited-set guard and missing-template handling.
fn visit_child_template(
    store: &TemplateIrStore,
    template_id: TemplateIrId,
    visited: &mut HashSet<TemplateIrId>,
    walker: fn(&TemplateIrStore, TemplateIrNodeId, &mut HashSet<TemplateIrId>) -> bool,
) -> bool {
    if !visited.insert(template_id) {
        return false;
    }

    store
        .get_template(template_id)
        .is_some_and(|child_template| walker(store, child_template.root, visited))
}
