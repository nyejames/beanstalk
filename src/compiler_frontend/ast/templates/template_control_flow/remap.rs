//! String-table remapping for structured template control flow.
//!
//! Project and module compilation merge worker-local string tables at several
//! boundaries. These impls keep every interned path/name inside template
//! control-flow payloads aligned with the merged table.

use crate::compiler_frontend::symbols::string_interning::StringIdRemap;

use super::types::{
    TemplateBranchChain, TemplateBranchSelector, TemplateControlFlow, TemplateLoopAggregatePiece,
    TemplateLoopAggregateRenderPlan, TemplateLoopControlFlow, TemplateLoopHeader,
};

impl TemplateControlFlow {
    pub(crate) fn remap_string_ids(&mut self, remap: &StringIdRemap) {
        match self {
            Self::BranchChain(branch_chain) => branch_chain.remap_string_ids(remap),
            Self::Loop(template_loop) => template_loop.remap_string_ids(remap),
            Self::LoopControl(signal) => signal.location.remap_string_ids(remap),
        }
    }
}

impl TemplateBranchChain {
    fn remap_string_ids(&mut self, remap: &StringIdRemap) {
        for branch in &mut self.branches {
            branch.selector.remap_string_ids(remap);
            branch.content.remap_string_ids(remap);
            if let Some(render_plan) = &mut branch.render_plan {
                render_plan.remap_string_ids(remap);
            }
            branch.location.remap_string_ids(remap);
        }

        if let Some(fallback) = &mut self.fallback {
            fallback.content.remap_string_ids(remap);
            if let Some(render_plan) = &mut fallback.render_plan {
                render_plan.remap_string_ids(remap);
            }
            fallback.location.remap_string_ids(remap);
        }

        self.location.remap_string_ids(remap);
    }
}

impl TemplateBranchSelector {
    pub(crate) fn remap_string_ids(&mut self, remap: &StringIdRemap) {
        match self {
            Self::Bool(condition) => {
                condition.remap_string_ids(remap);
            }

            Self::OptionPresentCapture { scrutinee, pattern } => {
                scrutinee.remap_string_ids(remap);
                pattern.remap_string_ids(remap);
            }
        }
    }
}

impl TemplateLoopControlFlow {
    fn remap_string_ids(&mut self, remap: &StringIdRemap) {
        self.header.remap_string_ids(remap);
        self.body_content.remap_string_ids(remap);
        if let Some(render_plan) = &mut self.body_render_plan {
            render_plan.remap_string_ids(remap);
        }
        if let Some(render_plan) = &mut self.aggregate_render_plan {
            render_plan.remap_string_ids(remap);
        }
        self.location.remap_string_ids(remap);
    }
}

impl TemplateLoopAggregateRenderPlan {
    pub(crate) fn remap_string_ids(&mut self, remap: &StringIdRemap) {
        for piece in &mut self.pieces {
            piece.remap_string_ids(remap);
        }
    }
}

impl TemplateLoopAggregatePiece {
    fn remap_string_ids(&mut self, remap: &StringIdRemap) {
        match self {
            Self::Render(piece) => piece.remap_string_ids(remap),
            Self::Aggregate => {}
        }
    }
}

impl TemplateLoopHeader {
    pub(crate) fn remap_string_ids(&mut self, remap: &StringIdRemap) {
        match self {
            Self::Conditional { condition } => {
                condition.remap_string_ids(remap);
            }

            Self::Range { bindings, range } => {
                bindings.remap_string_ids(remap);
                range.remap_string_ids(remap);
            }

            Self::Collection { bindings, iterable } => {
                bindings.remap_string_ids(remap);
                iterable.remap_string_ids(remap);
            }
        }
    }
}
