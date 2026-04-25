//! Exec IR instruction definitions.
//!
//! WHAT: defines the instructions and terminators that make up Exec IR blocks.

use super::types::{ExecBinaryOperator, ExecBlockId, ExecConstId, ExecLocalId, ExecUnaryOperator};

#[derive(Debug, Clone)]
pub(crate) enum ExecInstruction {
    LoadConst {
        target: ExecLocalId,
        const_id: ExecConstId,
    },
    ReadLocal {
        target: ExecLocalId,
        source: ExecLocalId,
    },
    CopyLocal {
        target: ExecLocalId,
        source: ExecLocalId,
    },
    BinaryOp {
        left: ExecLocalId,
        operator: ExecBinaryOperator,
        right: ExecLocalId,
        destination: ExecLocalId,
    },
    UnaryOp {
        operand: ExecLocalId,
        operator: ExecUnaryOperator,
        destination: ExecLocalId,
    },
}

#[derive(Debug, Clone)]
pub(crate) enum ExecTerminator {
    Return {
        value: Option<ExecLocalId>,
    },
    Jump {
        target: ExecBlockId,
    },
    BranchBool {
        condition: ExecLocalId,
        then_block: ExecBlockId,
        else_block: ExecBlockId,
    },
    PendingLowering {
        description: String,
    },
    UnreachableTrap,
}
