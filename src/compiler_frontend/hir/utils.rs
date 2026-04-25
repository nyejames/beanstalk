use crate::compiler_frontend::hir::ids::BlockId;
use crate::compiler_frontend::hir::terminators::HirTerminator;
use rustc_hash::FxHashSet;
use std::collections::VecDeque;

/// Extract the successor block IDs from a HIR terminator.
pub fn terminator_targets(terminator: &HirTerminator) -> Vec<BlockId> {
    match terminator {
        HirTerminator::Jump { target, .. } => vec![*target],
        HirTerminator::If {
            then_block,
            else_block,
            ..
        } => vec![*then_block, *else_block],
        HirTerminator::Match { arms, .. } => arms.iter().map(|arm| arm.body).collect(),
        HirTerminator::Break { target } | HirTerminator::Continue { target } => vec![*target],
        HirTerminator::Return(_) | HirTerminator::Panic { .. } => Vec::new(),
    }
}

/// Breadth-first traversal of reachable blocks starting from `entry`.
///
/// WHAT: collects every block ID reachable via terminator successors.
/// WHY: backends and analyses need a canonical reachability walk without reconstructing it
///      inline; this keeps successor logic in one place.
///
/// Callers supply a closure that resolves a block ID into its successor list. The closure can
/// fail (for example if the block is missing), so the error type is generic.
pub fn collect_reachable_blocks<E>(
    entry: BlockId,
    mut get_successors: impl FnMut(BlockId) -> Result<Vec<BlockId>, E>,
) -> Result<Vec<BlockId>, E> {
    let mut visited = FxHashSet::default();
    let mut queue = VecDeque::new();
    let mut order = Vec::new();

    queue.push_back(entry);

    while let Some(block_id) = queue.pop_front() {
        if !visited.insert(block_id) {
            continue;
        }
        order.push(block_id);
        for successor in get_successors(block_id)? {
            queue.push_back(successor);
        }
    }

    Ok(order)
}
