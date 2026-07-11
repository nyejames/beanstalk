//! Runtime template append context shared by HIR template lowering modules.
//!
//! WHAT: stores the active append target plus the runtime slot source/site state
//! needed while appending AST-owned runtime-template handoff nodes.
//! WHY: render appending, slot application lowering, control-flow lowering, and aggregate
//! wrapping all share this state, but `render_append.rs` should remain focused on appending
//! owned runtime-template nodes.

use crate::compiler_frontend::ast::templates::template_slots::RuntimeSlotContributionSourceId;
use crate::compiler_frontend::ast::templates::{OwnedRuntimeSlotSite, OwnedRuntimeTemplateNode};
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
    pub(super) wrapper_plan: &'a OwnedRuntimeTemplateNode,
    pub(super) target_accumulator: LocalId,
    pub(super) source_accumulators: &'a RuntimeSlotSourceAccumulatorContext,
    pub(super) slot_sites: &'a [OwnedRuntimeSlotSite],
    pub(super) contribution_emitted_flag: LocalId,
    pub(super) parent_emitted_flag: Option<LocalId>,
}

/// Policy for unresolved `OwnedRuntimeTemplateNode::Slot` placeholders.
///
/// WHAT: distinguishes the two runtime contexts in which HIR can see a slot
/// placeholder. Standalone runtime templates (e.g. helpers that are not being
/// used as slot wrappers) legitimately contain missing structural slots, which
/// render as empty strings. Active runtime slot-application wrappers, by
/// contrast, should have had every placeholder resolved to a concrete site by
/// AST slot routing; an unresolved placeholder there is an internal compiler
/// invariant breach.
/// WHY: keeps the HIR append path explicit about which no-output behavior is
/// valid and which is a transformation bug.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum RuntimeSlotPlaceholderPolicy {
    MissingSlotRendersEmpty,
    RejectUnresolvedSlot,
}

/// Append target plus optional runtime-slot state for owned-node lowering.
#[derive(Clone, Copy)]
pub(super) struct RuntimeTemplateAppendContext<'a> {
    pub(super) target_accumulator: LocalId,
    pub(super) emitted_output: Option<LocalId>,
    pub(super) source_accumulators: Option<&'a RuntimeSlotSourceAccumulatorContext>,
    pub(super) slot_sites: Option<&'a [OwnedRuntimeSlotSite]>,
    pub(super) loop_control_flush: Option<RuntimeSlotLoopControlFlush<'a>>,
    pub(super) slot_placeholder_policy: RuntimeSlotPlaceholderPolicy,
}

impl<'a> RuntimeTemplateAppendContext<'a> {
    pub(super) fn new(target_accumulator: LocalId) -> Self {
        Self {
            target_accumulator,
            emitted_output: None,
            source_accumulators: None,
            slot_sites: None,
            loop_control_flush: None,
            slot_placeholder_policy: RuntimeSlotPlaceholderPolicy::MissingSlotRendersEmpty,
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
        slot_sites: &'a [OwnedRuntimeSlotSite],
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

    /// Enables strict rejection of unresolved slot placeholders.
    ///
    /// WHAT: produces a context where an `OwnedRuntimeTemplateNode::Slot` is
    /// treated as a compiler transformation error instead of rendering empty.
    /// WHY: active runtime slot-application wrappers must resolve every slot
    /// placeholder to a concrete site; callers constructing that wrapper context
    /// opt into the stricter policy.
    pub(super) fn rejecting_unresolved_slots(mut self) -> Self {
        self.slot_placeholder_policy = RuntimeSlotPlaceholderPolicy::RejectUnresolvedSlot;
        self
    }

    pub(super) fn rejects_unresolved_slots(&self) -> bool {
        self.slot_placeholder_policy == RuntimeSlotPlaceholderPolicy::RejectUnresolvedSlot
    }

    pub(super) fn target_accumulator(&self) -> LocalId {
        self.target_accumulator
    }

    pub(super) fn emitted_output(&self) -> Option<LocalId> {
        self.emitted_output
    }
}
