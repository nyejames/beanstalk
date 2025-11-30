//! HIR Place model (scaffold)
//!
//! Represents memory locations for borrow checking and move analysis.

use crate::compiler::hir::nodes::HirExpr;
use crate::compiler::string_interning::InternedString;

#[derive(Debug, Clone)]
pub enum Place {
    Local(InternedString),
    Field {
        base: Box<Place>,
        field: InternedString,
    },
    Index {
        base: Box<Place>,
        index: Box<HirExpr>,
    },
    Global(InternedString),
}
