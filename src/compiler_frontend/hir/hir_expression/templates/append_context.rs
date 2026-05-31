//! Runtime template append context shared by HIR template lowering modules.
//!
//! WHAT: stores the active append target plus the runtime slot source/site state
//! needed while appending AST-prepared render plans.
//! WHY: render appending, slot application lowering, control-flow lowering, and aggregate
//! wrapping all share this state, but `render_append.rs` should remain focused on appending
//! render-plan pieces.

use crate::compiler_frontend::ast::templates::template_render_plan::TemplateRenderPlan;
use crate::compiler_frontend::ast::templates::template_slots::{
    RuntimeSlotContributionSourceId, RuntimeSlotSitePlan,
};
use crate::compiler_frontend::hir::ids::LocalId;

/// Source locals available while HIR lowers a runtime slot application wrapper.
///
/// WHAT: stores already-initialized source accumulators by AST source ID.
/// WHY: repeated slot sites must load source output without re-lowering the
/// authored contribution expressions.
pub(super) struct RuntimeSlotSourceAccumulatorContext {
    locals_by_source: Vec<LocalId>,
}

impl RuntimeSlotSourceAccumulatorContext {
    pub(super) fn new() -> Self {
        Self {
            locals_by_source: Vec::new(),
        }
    }

    pub(super) fn insert(&mut self, id: RuntimeSlotContributionSourceId, local: LocalId) {
        if self.locals_by_source.len() <= id.0 {
            self.locals_by_source.resize(id.0 + 1, local);
        }

        self.locals_by_source[id.0] = local;
    }

    pub(super) fn local_for(&self, id: RuntimeSlotContributionSourceId) -> Option<LocalId> {
        self.locals_by_source.get(id.0).copied()
    }
}

/// Wrapper replay needed before a runtime slot contribution emits loop control.
///
/// WHAT: carries the outer slot-application wrapper state while a contribution
/// is being accumulated into slot-local strings.
/// WHY: if a contribution outputs text and then hits `[break]` / `[continue]`,
/// the wrapper must be appended on that terminating CFG path before control
/// jumps to the surrounding template loop.
#[derive(Clone, Copy)]
pub(super) struct RuntimeSlotLoopControlFlush<'a> {
    pub(super) wrapper_plan: &'a TemplateRenderPlan,
    pub(super) target_accumulator: LocalId,
    pub(super) source_accumulators: &'a RuntimeSlotSourceAccumulatorContext,
    pub(super) slot_sites: &'a [RuntimeSlotSitePlan],
    pub(super) contribution_emitted_flag: LocalId,
    pub(super) parent_emitted_flag: Option<LocalId>,
}

/// Append target plus optional runtime-slot state for render-plan lowering.
#[derive(Clone, Copy)]
pub(super) struct RuntimeTemplateAppendContext<'a> {
    pub(super) target_accumulator: LocalId,
    pub(super) emitted_output: Option<LocalId>,
    pub(super) source_accumulators: Option<&'a RuntimeSlotSourceAccumulatorContext>,
    pub(super) slot_sites: Option<&'a [RuntimeSlotSitePlan]>,
    pub(super) loop_control_flush: Option<RuntimeSlotLoopControlFlush<'a>>,
}

impl<'a> RuntimeTemplateAppendContext<'a> {
    pub(super) fn new(target_accumulator: LocalId) -> Self {
        Self {
            target_accumulator,
            emitted_output: None,
            source_accumulators: None,
            slot_sites: None,
            loop_control_flush: None,
        }
    }

    pub(super) fn with_emitted_output(mut self, flag: Option<LocalId>) -> Self {
        self.emitted_output = flag;
        self
    }

    pub(super) fn with_target_accumulator(mut self, target_accumulator: LocalId) -> Self {
        self.target_accumulator = target_accumulator;
        self
    }

    pub(super) fn with_runtime_slot_sites(
        mut self,
        source_accumulators: &'a RuntimeSlotSourceAccumulatorContext,
        slot_sites: &'a [RuntimeSlotSitePlan],
    ) -> Self {
        self.source_accumulators = Some(source_accumulators);
        self.slot_sites = Some(slot_sites);
        self
    }

    pub(super) fn with_loop_control_flush(
        mut self,
        flush: RuntimeSlotLoopControlFlush<'a>,
    ) -> Self {
        self.loop_control_flush = Some(flush);
        self
    }

    pub(super) fn target_accumulator(&self) -> LocalId {
        self.target_accumulator
    }

    pub(super) fn emitted_output(&self) -> Option<LocalId> {
        self.emitted_output
    }
}
