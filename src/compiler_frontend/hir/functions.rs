//! HIR function declarations.
//!
//! WHAT: function-level HIR metadata, including entry block, parameters, return type, and semantic
//! origin classification.
//! WHY: backends need to distinguish regular functions from the implicit entry `start` function.

use crate::compiler_frontend::hir::hir_datatypes::TypeId;
use crate::compiler_frontend::hir::ids::{BlockId, FunctionId, LocalId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HirFunctionOrigin {
    /// Regular user-declared function.
    Normal,
    /// Implicit start function for the module entry file.
    EntryStart,
}

#[derive(Debug, Clone)]
pub struct HirFunction {
    pub id: FunctionId,
    pub entry: BlockId,
    pub params: Vec<LocalId>,
    pub return_type: TypeId,
    pub return_aliases: Vec<Option<Vec<usize>>>,
}
