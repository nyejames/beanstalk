//! AST runtime slot application planning.
//!
//! WHAT: This module resolves a valid slot application into either a fully
//! composed `TemplateContent` or an AST-prepared runtime plan with contribution
//! sources and concrete wrapper slot sites.
//!
//! WHY: Runtime slot applications share schema discovery and contribution
//! routing with compile-time composition, but HIR should only consume prepared
//! source/site plans. Keeping the planner split by responsibility makes that
//! AST/HIR boundary easier to audit.

mod remap;
mod sites;
mod sources;
mod types;

use super::composition::{compose_wrapper_atoms_recursive, route_slot_contributions};
use super::error::TemplateSlotError;
use crate::compiler_frontend::ast::templates::template::TemplateContent;
use crate::compiler_frontend::ast::templates::template_types::Template;
use crate::compiler_frontend::compiler_errors::SourceLocation;
use crate::compiler_frontend::symbols::string_interning::StringTable;

pub(crate) use types::{
    RuntimeSlotApplicationPlan, RuntimeSlotContributionSourceId, RuntimeSlotSiteId,
    RuntimeSlotSitePiece, RuntimeSlotSitePlan,
};
#[cfg(test)]
pub(crate) use types::{RuntimeSlotContributionSource, RuntimeSlotSiteRenderPlan};
pub(in crate::compiler_frontend::ast::templates) use types::{
    SlotResolutionMode, SlotResolutionOutcome,
};

#[cfg(test)]
pub(super) use sources::routed_slot_contributions_contain_runtime_content;

/// Resolves a slot application, returning either a composed result or a runtime plan.
///
/// WHAT:
/// - Reuses `route_slot_contributions` for schema discovery, insert extraction,
///   loose grouping, and target validation.
/// - For fully static applications, expands slot placeholders recursively.
/// - For runtime applications, builds a `RuntimeSlotApplicationPlan` instead.
///
/// WHY: One routing path keeps diagnostics and ordering consistent regardless of
/// whether the final outcome is composed at AST time or lowered at runtime.
pub(in crate::compiler_frontend::ast::templates) fn resolve_slot_application(
    wrapper: &Template,
    fill_content: TemplateContent,
    location: &SourceLocation,
    string_table: &StringTable,
    resolution_mode: SlotResolutionMode,
) -> Result<SlotResolutionOutcome, TemplateSlotError> {
    let routed = route_slot_contributions(wrapper, fill_content, location, string_table)?;

    if resolution_mode.allows_runtime_plans() && sources::should_lower_as_runtime(&routed) {
        let wrapper_content = sources::content_prepared_for_runtime_rendering(&wrapper.content);
        let sources = sources::build_runtime_contribution_sources(&routed, location, string_table);
        let wrapper = sites::build_runtime_wrapper_site_plan(
            &wrapper_content,
            &sources,
            location,
            string_table,
        )?;

        return Ok(SlotResolutionOutcome::Runtime(RuntimeSlotApplicationPlan {
            wrapper_plan: wrapper.wrapper_plan,
            contribution_sources: sources.into_iter().map(|draft| draft.source).collect(),
            slot_sites: wrapper.slot_sites,
            location: location.clone(),
        }));
    }

    let atoms = compose_wrapper_atoms_recursive(
        &wrapper.content.atoms,
        &routed.contributions,
        string_table,
        resolution_mode,
    )?;

    Ok(SlotResolutionOutcome::Composed(TemplateContent { atoms }))
}
