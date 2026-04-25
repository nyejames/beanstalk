//! HIR memory places.
//!
//! WHAT: canonical memory projections such as locals, fields, and indexed elements.
//! WHY: assignments, loads, copies, and borrow checking need one shared place representation.

use crate::compiler_frontend::hir::expressions::HirExpression;
use crate::compiler_frontend::hir::ids::{FieldId, LocalId};

#[derive(Debug, Clone)]
pub enum HirPlace {
    Local(LocalId),

    Field {
        base: Box<HirPlace>,
        field: FieldId,
    },

    Index {
        base: Box<HirPlace>,
        index: Box<HirExpression>,
    },
}
