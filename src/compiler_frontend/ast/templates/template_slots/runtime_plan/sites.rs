//! Runtime wrapper slot-site planning.
//!
//! WHAT: assigns stable IDs to concrete wrapper `$slot` occurrences and builds
//! TIR-side slot-site render plans for the same routed contribution sources.
//!
//! WHY: runtime applications must evaluate each source once while repeated slot
//! placeholders can still carry different `$children(..)` and `$fresh`
//! metadata. The planner writes the TIR side table directly so production code
//! no longer builds a `TemplateRenderPlan` only to convert it back into TIR.

use super::types::{RuntimeSlotContributionSourceDraft, RuntimeSlotSiteId};
use crate::compiler_frontend::ast::templates::error::TemplateError;
use crate::compiler_frontend::ast::templates::template::SlotKey;
use crate::compiler_frontend::ast::templates::template_slots::error::TemplateSlotError;
use crate::compiler_frontend::ast::templates::tir::{
    TemplateIrId, TemplateIrNodeId, TemplateIrNodeKind, TemplateIrStore, TemplateSlotPlanId,
    TemplateSlotSitePlan, TemplateSlotSiteRenderPiece, TemplateSlotSiteRenderPlan,
    TemplateWrapperSetId, TirCopyState, TirSlotPlaceholder, TirSlotSchema,
    collect_tir_slot_placeholders_in_order, collect_tir_slot_schema,
    copy_tir_subtree_with_active_slot_plan, tir_subtree_has_unresolved_slots,
};
use crate::compiler_frontend::compiler_errors::{CompilerError, SourceLocation};
use crate::compiler_frontend::instrumentation::{AstCounter, add_ast_counter};
use crate::compiler_frontend::symbols::string_interning::StringTable;

pub(super) fn build_runtime_wrapper_site_plan(
    wrapper_tir_root: TemplateIrNodeId,
    sources: &[RuntimeSlotContributionSourceDraft],
    slot_plan_id: TemplateSlotPlanId,
    store: &mut TemplateIrStore,
    string_table: &StringTable,
    copy_state: &mut TirCopyState,
) -> Result<Vec<TemplateSlotSitePlan>, TemplateSlotError> {
    RuntimeWrapperSitePlanBuilder {
        sources,
        slot_plan_id,
        store,
        string_table,
        copy_state,
    }
    .build_slot_sites(wrapper_tir_root)
}

#[derive(Clone)]
struct RuntimeSlotSiteDraft {
    site: RuntimeSlotSiteId,
    key: SlotKey,
    placeholder: TirSlotPlaceholder,
    location: SourceLocation,
}

struct RuntimeWrapperSitePlanBuilder<'a> {
    sources: &'a [RuntimeSlotContributionSourceDraft],
    slot_plan_id: TemplateSlotPlanId,
    store: &'a mut TemplateIrStore,
    string_table: &'a StringTable,
    copy_state: &'a mut TirCopyState,
}

impl RuntimeWrapperSitePlanBuilder<'_> {
    fn build_slot_sites(
        mut self,
        wrapper_tir_root: TemplateIrNodeId,
    ) -> Result<Vec<TemplateSlotSitePlan>, TemplateSlotError> {
        let mut drafts = Vec::new();
        self.collect_site_drafts(wrapper_tir_root, &mut drafts)?;

        self.install_slot_site_headers(&drafts)?;

        let mut slot_sites = Vec::with_capacity(drafts.len());
        for draft in drafts {
            let render_plan = self.build_site_render_plan(&draft.placeholder)?;
            slot_sites.push(TemplateSlotSitePlan {
                site: draft.site,
                key: draft.key,
                render_plan,
                location: draft.location,
            });
        }

        Ok(slot_sites)
    }

    fn collect_site_drafts(
        &mut self,
        wrapper_tir_root: TemplateIrNodeId,
        drafts: &mut Vec<RuntimeSlotSiteDraft>,
    ) -> Result<(), TemplateSlotError> {
        // Walk the TIR tree in document order to discover every unresolved slot
        // placeholder, including those nested inside child templates, branch
        // chains, and loops. TIR is the sole authority for slot-placeholder
        // discovery.
        // Site IDs are assigned in traversal order, matching the cursor-based
        // assignment the final materialization pass will use.
        let placeholders = collect_tir_slot_placeholders_in_order(self.store, wrapper_tir_root)
            .map_err(TemplateSlotError::from)?;

        for placeholder in placeholders {
            let site = RuntimeSlotSiteId(drafts.len());
            drafts.push(RuntimeSlotSiteDraft {
                site,
                key: placeholder.key.clone(),
                placeholder: placeholder.clone(),
                location: placeholder.location.clone(),
            });
        }

        Ok(())
    }

    fn install_slot_site_headers(
        &mut self,
        drafts: &[RuntimeSlotSiteDraft],
    ) -> Result<(), TemplateSlotError> {
        let Some(slot_plan) = self.store.slot_plans.get_mut(self.slot_plan_id.index()) else {
            return Err(CompilerError::compiler_error(
                "Runtime slot site planning lost its TIR slot-plan entry.",
            )
            .into());
        };

        slot_plan.slot_sites = drafts
            .iter()
            .map(|draft| TemplateSlotSitePlan {
                site: draft.site,
                key: draft.key.clone(),
                render_plan: TemplateSlotSiteRenderPlan::default(),
                location: draft.location.clone(),
            })
            .collect();

        Ok(())
    }

    fn build_site_render_plan(
        &mut self,
        placeholder: &TirSlotPlaceholder,
    ) -> Result<TemplateSlotSiteRenderPlan, TemplateSlotError> {
        let mut render_plan = TemplateSlotSiteRenderPlan::default();

        for source in self
            .sources
            .iter()
            .filter(|source| source.source.target == placeholder.key)
        {
            let source_plan = TemplateSlotSiteRenderPlan {
                pieces: vec![TemplateSlotSiteRenderPiece::ContributionSource(
                    source.source.source,
                )],
            };
            let wrapped_plan = self.apply_site_wrappers(placeholder, source, source_plan)?;
            render_plan.pieces.extend(wrapped_plan.pieces);
        }

        Ok(render_plan)
    }

    fn apply_site_wrappers(
        &mut self,
        placeholder: &TirSlotPlaceholder,
        source: &RuntimeSlotContributionSourceDraft,
        source_plan: TemplateSlotSiteRenderPlan,
    ) -> Result<TemplateSlotSiteRenderPlan, TemplateSlotError> {
        let shape = source.shape.clone();

        // Source plans are distinct from site plans so repeated placeholders can
        // apply their own `$children(..)` metadata without re-evaluating the source.
        let (mut plan, wrapped_as_child) = match placeholder.child_wrapper_set {
            Some(child_wrapper_set)
                if !shape.skips_parent_child_wrappers && shape.is_child_template_contribution =>
            {
                (
                    self.wrap_site_plan_with_tir_child_wrappers(source_plan, child_wrapper_set)?,
                    true,
                )
            }

            _ => (source_plan, shape.is_child_template_contribution),
        };

        if !placeholder.skip_parent_child_wrappers
            && wrapped_as_child
            && let Some(applied_child_wrapper_set) = placeholder.applied_child_wrapper_set
        {
            plan = self.wrap_site_plan_with_tir_child_wrappers(plan, applied_child_wrapper_set)?;
        }

        Ok(plan)
    }

    /// Applies an ordered TIR wrapper set around a slot-site render plan.
    ///
    /// WHAT: resolves store-local wrapper template IDs from the wrapper-set side
    /// table and wraps from innermost to outermost using the TIR-native wrapper
    /// render-piece path.
    /// WHY: `TirSlotPlaceholder` no longer stores recursive `Template` values,
    /// so runtime slot-site planning must consume the same-store ID path.
    fn wrap_site_plan_with_tir_child_wrappers(
        &mut self,
        mut plan: TemplateSlotSiteRenderPlan,
        wrapper_set_id: TemplateWrapperSetId,
    ) -> Result<TemplateSlotSiteRenderPlan, TemplateSlotError> {
        let Some(wrapper_set) = self.store.wrapper_sets.get(wrapper_set_id.index()) else {
            return Err(CompilerError::compiler_error(
                "Runtime slot site planning found a slot wrapper-set ID that was not present in the TIR store.",
            )
            .into());
        };

        // Wrapper sets store `TemplateWrapperReference` values; extract the
        // store-local `TemplateIrId` for same-store lookups.
        let wrapper_refs = wrapper_set.wrappers.clone();
        for wrapper_ref in wrapper_refs.into_iter().rev() {
            add_ast_counter(AstCounter::TemplateWrapperApplications, 1);

            let wrapper_id = wrapper_ref.root.template_id;

            let Some(wrapper_root) = self
                .store
                .get_template(wrapper_id)
                .map(|template| template.root)
            else {
                return Err(CompilerError::compiler_error(format!(
                    "Runtime slot site planning found a slot wrapper template ref {} that was not present in the TIR store.",
                    wrapper_ref
                ))
                .into());
            };

            let Some(pieces) = self.try_build_child_wrapper_site_pieces_from_tir_id(
                wrapper_id,
                wrapper_root,
                &plan,
            )?
            else {
                return Err(CompilerError::compiler_error(
                    "Runtime slot site planning could not apply a TIR slot wrapper to a loose contribution.",
                )
                .into());
            };

            plan = TemplateSlotSiteRenderPlan { pieces };
        }

        Ok(plan)
    }

    /// Builds child-wrapper site render pieces from a known same-store wrapper
    /// TIR template.
    ///
    /// WHAT: consumes a store-local wrapper template ID and root directly,
    /// applying the same TIR render-piece construction used for all slot-site
    /// wrapper planning.
    /// WHY: `TirSlotPlaceholder` stores wrapper IDs rather than recursive
    /// `Template` values, so runtime wrapper planning must stay on the
    /// store-local TIR path.
    fn try_build_child_wrapper_site_pieces_from_tir_id(
        &mut self,
        template_id: TemplateIrId,
        wrapper_root: TemplateIrNodeId,
        inner_plan: &TemplateSlotSiteRenderPlan,
    ) -> Result<Option<Vec<TemplateSlotSiteRenderPiece>>, TemplateSlotError> {
        if !tir_subtree_has_unresolved_slots(self.store, wrapper_root) {
            let copied_root = copy_tir_subtree_with_active_slot_plan(
                wrapper_root,
                None,
                self.store,
                self.copy_state,
            )
            .map_err(TemplateSlotError::from)?;

            let mut pieces = vec![TemplateSlotSiteRenderPiece::Render(copied_root)];
            pieces.extend(inner_plan.pieces.iter().cloned());
            return Ok(Some(pieces));
        }

        let schema = collect_tir_slot_schema(self.store, template_id, self.string_table)?;
        let Some(target_key) = loose_contribution_target_key(&schema) else {
            // Named-only wrappers cannot absorb a loose contribution. Let the atom
            // fallback produce the same diagnostic it always has.
            return Ok(None);
        };

        let pieces = build_tir_wrapper_render_pieces(
            wrapper_root,
            inner_plan,
            target_key,
            self.store,
            self.copy_state,
        )
        .map_err(TemplateSlotError::from)?;

        Ok(Some(pieces))
    }
}

/// Chooses the slot key that a single loose contribution would fill.
///
/// WHAT: mirrors the atom-level router: loose chunks go to positional slots
///       first, then to the default slot. Named slots only receive explicit
///       `$insert(...)` contributions, so a wrapper with only named slots cannot
///       absorb a loose contribution through this path.
fn loose_contribution_target_key(schema: &TirSlotSchema) -> Option<SlotKey> {
    if let Some(index) = schema.ordered_positional_slots().first() {
        return Some(SlotKey::Positional(*index));
    }

    if schema.has_default_slot {
        return Some(SlotKey::Default);
    }

    None
}

/// Builds render pieces for a wrapper TIR tree by replacing direct slot
/// placeholders that receive the loose contribution with `inner_plan` pieces and
/// copying every other top-level node as a render piece.
///
/// WHAT: walks the wrapper's own content only; slots nested inside child
///       templates or control-flow bodies are treated as part of the subtree
///       that gets copied as a render piece, matching the atom fallback's
///       top-level behavior.
fn build_tir_wrapper_render_pieces(
    wrapper_root: TemplateIrNodeId,
    inner_plan: &TemplateSlotSiteRenderPlan,
    target_key: SlotKey,
    store: &mut TemplateIrStore,
    copy_state: &mut TirCopyState,
) -> Result<Vec<TemplateSlotSiteRenderPiece>, TemplateError> {
    let node = store.get_node(wrapper_root).cloned().ok_or_else(|| {
        CompilerError::compiler_error(
            "Child-wrapper TIR path could not read the wrapper root node.",
        )
    })?;

    match node.kind {
        TemplateIrNodeKind::Slot { placeholder } if placeholder.key == target_key => {
            Ok(inner_plan.pieces.clone())
        }

        TemplateIrNodeKind::Slot { .. } => Ok(vec![]),

        TemplateIrNodeKind::Sequence { children } => {
            let mut pieces = Vec::new();

            for child_id in children {
                if let Some(slot_key) = slot_key_for_node(store, child_id) {
                    if slot_key == target_key {
                        pieces.extend(inner_plan.pieces.iter().cloned());
                    }

                    continue;
                }

                let copied_child =
                    copy_tir_subtree_with_active_slot_plan(child_id, None, store, copy_state)?;
                pieces.push(TemplateSlotSiteRenderPiece::Render(copied_child));
            }

            Ok(pieces)
        }

        TemplateIrNodeKind::ChildTemplate { reference, .. } => {
            if let Some(template_id) = reference.template_id_in_store(store.store_id())
                && let Some(template) = store.get_template(template_id)
            {
                build_tir_wrapper_render_pieces(
                    template.root,
                    inner_plan,
                    target_key,
                    store,
                    copy_state,
                )
            } else {
                let copied_root =
                    copy_tir_subtree_with_active_slot_plan(wrapper_root, None, store, copy_state)?;
                Ok(vec![TemplateSlotSiteRenderPiece::Render(copied_root)])
            }
        }

        _ => {
            // Non-sequence roots with unresolved slots (for example a control-flow
            // wrapper) are copied whole without inserting the inner plan, just as
            // the atom fallback materializes the whole top-level content atom.
            let copied_root =
                copy_tir_subtree_with_active_slot_plan(wrapper_root, None, store, copy_state)?;
            Ok(vec![TemplateSlotSiteRenderPiece::Render(copied_root)])
        }
    }
}

fn slot_key_for_node(store: &TemplateIrStore, node_id: TemplateIrNodeId) -> Option<SlotKey> {
    let node = store.get_node(node_id)?;
    let TemplateIrNodeKind::Slot { placeholder } = &node.kind else {
        return None;
    };

    Some(placeholder.key.clone())
}
