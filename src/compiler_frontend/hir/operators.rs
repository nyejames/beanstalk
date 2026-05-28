//! HIR operators.
//!
//! WHAT: normalized binary and unary operator enums used by HIR expressions.
//! WHY: backends should consume semantic operators rather than frontend token kinds.

#[derive(Debug, Clone, Copy)]
pub enum HirBinOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
    IntDiv,
    Exponent,
}

#[derive(Debug, Clone, Copy)]
pub enum HirUnaryOp {
    Neg,
    Not,
}
