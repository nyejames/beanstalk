//! String-id remapping for runtime slot plans.
//!
//! WHAT: Remaps interned names and source locations carried by AST-prepared
//! runtime slot plans after string-table merge boundaries.
//!
//! WHY: Runtime slot plans cross the AST/HIR boundary as normal AST payloads, so
//! they must follow the same deterministic string-id remap lifecycle as the rest
//! of the frontend.

use super::types::{RuntimeSlotApplicationPlan, RuntimeSlotSitePiece, RuntimeSlotSiteRenderPlan};
use crate::compiler_frontend::symbols::string_interning::StringIdRemap;

impl RuntimeSlotApplicationPlan {
    pub fn remap_string_ids(&mut self, remap: &StringIdRemap) {
        self.wrapper_plan.remap_string_ids(remap);

        for source in &mut self.contribution_sources {
            source.target.remap_string_ids(remap);
            source.location.remap_string_ids(remap);
            source.render_plan.remap_string_ids(remap);
        }

        for site in &mut self.slot_sites {
            site.key.remap_string_ids(remap);
            site.location.remap_string_ids(remap);
            site.render_plan.remap_string_ids(remap);
        }

        self.location.remap_string_ids(remap);
    }
}

impl RuntimeSlotSiteRenderPlan {
    pub fn remap_string_ids(&mut self, remap: &StringIdRemap) {
        for piece in &mut self.pieces {
            piece.remap_string_ids(remap);
        }
    }
}

impl RuntimeSlotSitePiece {
    pub fn remap_string_ids(&mut self, remap: &StringIdRemap) {
        match self {
            RuntimeSlotSitePiece::Render(piece) => piece.remap_string_ids(remap),
            RuntimeSlotSitePiece::ContributionSource(_) => {}
        }
    }
}
