//! Runtime wrapper slot-site planning.
//!
//! WHAT: Rewrites wrapper render plans so each concrete `$slot` placeholder
//! becomes a `RuntimeSlotSiteId`, then builds the per-site render plan that
//! replays matching contribution sources through placeholder-local wrappers.
//!
//! WHY: Runtime applications must evaluate each source once while repeated slot
//! placeholders can still carry different `$children(..)` and `$fresh` metadata.

use super::super::contribution_shape::classify_contribution_atom;
use super::types::{
    RuntimeSlotContributionSourceDraft, RuntimeSlotSiteId, RuntimeSlotSitePiece,
    RuntimeSlotSitePlan, RuntimeSlotSiteRenderPlan,
};
use crate::compiler_frontend::ast::expressions::expression::ExpressionKind;
use crate::compiler_frontend::ast::templates::template::{
    SlotPlaceholder, TemplateAtom, TemplateContent, TemplateType,
};
use crate::compiler_frontend::ast::templates::template_render_plan::{
    RenderPiece, TemplateRenderPlan,
};
use crate::compiler_frontend::ast::templates::template_slots::composition::{
    RoutedSlotContributions, route_slot_contributions,
};
use crate::compiler_frontend::ast::templates::template_slots::error::TemplateSlotError;
use crate::compiler_frontend::ast::templates::template_types::Template;
use crate::compiler_frontend::compiler_errors::SourceLocation;

pub(super) struct RuntimeWrapperSitePlan {
    pub(super) wrapper_plan: TemplateRenderPlan,
    pub(super) slot_sites: Vec<RuntimeSlotSitePlan>,
}

pub(super) fn build_runtime_wrapper_site_plan(
    wrapper_content: &TemplateContent,
    sources: &[RuntimeSlotContributionSourceDraft],
    fallback_location: &SourceLocation,
) -> Result<RuntimeWrapperSitePlan, TemplateSlotError> {
    RuntimeWrapperSitePlanBuilder {
        sources,
        fallback_location,
    }
    .build_wrapper_plan(wrapper_content)
}

struct RuntimeWrapperSitePlanBuilder<'a> {
    sources: &'a [RuntimeSlotContributionSourceDraft],
    fallback_location: &'a SourceLocation,
}

impl RuntimeWrapperSitePlanBuilder<'_> {
    fn build_wrapper_plan(
        self,
        wrapper_content: &TemplateContent,
    ) -> Result<RuntimeWrapperSitePlan, TemplateSlotError> {
        let mut slot_sites = Vec::new();
        let pieces = self.build_wrapper_pieces(&wrapper_content.atoms, &mut slot_sites)?;

        Ok(RuntimeWrapperSitePlan {
            wrapper_plan: TemplateRenderPlan { pieces },
            slot_sites,
        })
    }

    fn build_wrapper_pieces(
        &self,
        atoms: &[TemplateAtom],
        slot_sites: &mut Vec<RuntimeSlotSitePlan>,
    ) -> Result<Vec<RenderPiece>, TemplateSlotError> {
        let mut pieces = Vec::new();

        for atom in atoms {
            match atom {
                TemplateAtom::Slot(placeholder) => {
                    let id = RuntimeSlotSiteId(slot_sites.len());
                    let render_plan = self.build_site_render_plan(placeholder)?;
                    slot_sites.push(RuntimeSlotSitePlan {
                        id,
                        key: placeholder.key.clone(),
                        render_plan,
                        location: self.fallback_location.clone(),
                    });
                    pieces.push(RenderPiece::RuntimeSlotSite(id));
                }

                TemplateAtom::Content(segment) => {
                    let mut segment = segment.clone();
                    if let ExpressionKind::Template(template) = &segment.expression.kind
                        && template.has_unresolved_slots()
                    {
                        let mut nested_template = template.as_ref().clone_for_composition();
                        let nested_pieces =
                            self.build_wrapper_pieces(&nested_template.content.atoms, slot_sites)?;
                        nested_template.render_plan = Some(TemplateRenderPlan {
                            pieces: nested_pieces,
                        });
                        nested_template.kind = TemplateType::StringFunction;
                        segment.expression.kind =
                            ExpressionKind::Template(Box::new(nested_template));
                    }

                    let content = TemplateContent {
                        atoms: vec![TemplateAtom::Content(segment)],
                    };
                    pieces.extend(TemplateRenderPlan::from_content(&content).pieces);
                }
            }
        }

        Ok(pieces)
    }

    fn build_site_render_plan(
        &self,
        placeholder: &SlotPlaceholder,
    ) -> Result<RuntimeSlotSiteRenderPlan, TemplateSlotError> {
        let mut render_plan = RuntimeSlotSiteRenderPlan::default();

        for source in self
            .sources
            .iter()
            .filter(|source| source.source.target == placeholder.key)
        {
            let source_plan = RuntimeSlotSiteRenderPlan {
                pieces: vec![RuntimeSlotSitePiece::ContributionSource(source.source.id)],
            };
            let wrapped_plan = self.apply_site_wrappers(placeholder, source, source_plan)?;
            render_plan.pieces.extend(wrapped_plan.pieces);
        }

        Ok(render_plan)
    }

    fn apply_site_wrappers(
        &self,
        placeholder: &SlotPlaceholder,
        source: &RuntimeSlotContributionSourceDraft,
        source_plan: RuntimeSlotSiteRenderPlan,
    ) -> Result<RuntimeSlotSiteRenderPlan, TemplateSlotError> {
        let shape = classify_contribution_atom(&source.atom);

        // Source plans are distinct from site plans so repeated placeholders can
        // apply their own `$children(..)` metadata without re-evaluating the source.
        let (mut plan, wrapped_as_child) = if placeholder.child_wrappers.is_empty()
            || shape.skips_parent_child_wrappers
            || !shape.is_child_template_contribution
        {
            (source_plan, shape.is_child_template_contribution)
        } else {
            (
                self.wrap_site_plan_with_child_wrappers(
                    source_plan,
                    &source.atom,
                    &placeholder.child_wrappers,
                )?,
                true,
            )
        };

        if !placeholder.skip_parent_child_wrappers
            && !placeholder.applied_child_wrappers.is_empty()
            && wrapped_as_child
        {
            plan = self.wrap_site_plan_with_child_wrappers(
                plan,
                &source.atom,
                &placeholder.applied_child_wrappers,
            )?;
        }

        Ok(plan)
    }

    fn wrap_site_plan_with_child_wrappers(
        &self,
        mut plan: RuntimeSlotSiteRenderPlan,
        routing_atom: &TemplateAtom,
        child_wrappers: &[Template],
    ) -> Result<RuntimeSlotSiteRenderPlan, TemplateSlotError> {
        for wrapper in child_wrappers.iter().rev() {
            plan = self.wrap_site_plan_in_child_wrapper(plan, routing_atom, wrapper)?;
        }

        Ok(plan)
    }

    fn wrap_site_plan_in_child_wrapper(
        &self,
        inner_plan: RuntimeSlotSiteRenderPlan,
        routing_atom: &TemplateAtom,
        wrapper: &Template,
    ) -> Result<RuntimeSlotSiteRenderPlan, TemplateSlotError> {
        if !wrapper.has_unresolved_slots() {
            let mut pieces = TemplateRenderPlan::from_content(&wrapper.content)
                .pieces
                .into_iter()
                .map(|piece| RuntimeSlotSitePiece::Render(Box::new(piece)))
                .collect::<Vec<_>>();
            pieces.extend(inner_plan.pieces);
            return Ok(RuntimeSlotSiteRenderPlan { pieces });
        }

        let routed = route_slot_contributions(
            wrapper,
            TemplateContent {
                atoms: vec![routing_atom.clone()],
            },
            &wrapper.location,
        )?;
        let pieces =
            self.build_child_wrapper_site_pieces(&wrapper.content.atoms, &routed, &inner_plan)?;

        Ok(RuntimeSlotSiteRenderPlan { pieces })
    }

    fn build_child_wrapper_site_pieces(
        &self,
        atoms: &[TemplateAtom],
        routed: &RoutedSlotContributions,
        inner_plan: &RuntimeSlotSiteRenderPlan,
    ) -> Result<Vec<RuntimeSlotSitePiece>, TemplateSlotError> {
        let mut pieces = Vec::new();

        for atom in atoms {
            match atom {
                TemplateAtom::Slot(placeholder)
                    if !routed
                        .contributions
                        .atoms_for_slot(&placeholder.key)
                        .is_empty() =>
                {
                    pieces.extend(inner_plan.pieces.clone());
                }

                TemplateAtom::Slot(_) => {}

                TemplateAtom::Content(segment) => {
                    let content = TemplateContent {
                        atoms: vec![TemplateAtom::Content(segment.clone())],
                    };
                    pieces.extend(
                        TemplateRenderPlan::from_content(&content)
                            .pieces
                            .into_iter()
                            .map(|piece| RuntimeSlotSitePiece::Render(Box::new(piece))),
                    );
                }
            }
        }

        Ok(pieces)
    }
}
