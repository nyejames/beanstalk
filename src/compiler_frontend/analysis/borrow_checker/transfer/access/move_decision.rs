use crate::compiler_frontend::analysis::borrow_checker::state::{
    FunctionLayout, FutureUseKind, RootSet,
};
use crate::compiler_frontend::hir::hir_nodes::BlockId;

// WHAT: Encodes whether a mutable-capable access should remain a borrow or consume ownership.
// WHY: Transfer paths use this single decision to keep assignment/call move behavior consistent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum MoveDecision {
    Borrow,
    Move,
    Inconsistent(usize),
}

// WHAT: Classifies move eligibility from future-use facts at one program point.
// WHY: The borrow checker only allows moves when roots have no required future use.
pub(super) fn classify_move_decision(
    layout: &FunctionLayout,
    block_id: BlockId,
    roots: &RootSet,
    current_order: i32,
) -> MoveDecision {
    let mut saw_must = false;
    let mut saw_none = false;

    for root_index in roots.iter_ones() {
        match layout.future_use_kind(block_id, root_index, current_order) {
            FutureUseKind::Must => saw_must = true,
            FutureUseKind::None => saw_none = true,
            FutureUseKind::May => return MoveDecision::Inconsistent(root_index),
        }
    }

    if saw_must {
        MoveDecision::Borrow
    } else if saw_none {
        MoveDecision::Move
    } else {
        MoveDecision::Borrow
    }
}
