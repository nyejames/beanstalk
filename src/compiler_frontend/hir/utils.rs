use crate::compiler_frontend::hir::hir_nodes::{BlockId, HirTerminator};

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
