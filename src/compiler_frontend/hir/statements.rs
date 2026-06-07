//! HIR statements.
//!
//! WHAT: effectful operations inside HIR blocks.
//! WHY: statements are where assignment, calls, side-effect expressions, and runtime fragment pushes
//! become explicit before borrow validation and backend lowering.

use crate::compiler_frontend::external_packages::CallTarget;
use crate::compiler_frontend::hir::expressions::{HirExpression, HirMapOp};
use crate::compiler_frontend::hir::ids::{HirNodeId, LocalId};
use crate::compiler_frontend::hir::places::HirPlace;
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;
use crate::compiler_frontend::traits::ids::{TraitId, TraitRequirementId};

#[derive(Debug, Clone)]
pub struct HirStatement {
    pub id: HirNodeId,
    pub kind: HirStatementKind,
    pub location: SourceLocation,
}

#[derive(Debug, Clone)]
pub enum HirStatementKind {
    Assign {
        target: HirPlace,
        value: HirExpression,
    },

    /// Call a function and optionally capture the result.
    ///
    /// WHAT: invokes `target` with evaluated `args` and binds the return value to `result`
    ///       when present.
    /// WHY: nested calls are flattened into statement preludes during expression lowering;
    ///      a top-level call in statement position is represented directly as a `Call`.
    Call {
        target: CallTarget,
        args: Vec<HirExpression>,
        result: Option<LocalId>,
    },

    /// Dispatch a method through a dynamic trait wrapper.
    ///
    /// WHAT: stores the trait/requirement identity chosen by AST plus the lowered receiver and
    /// argument access facts needed by borrow validation.
    /// WHY: JS lowers runtime method tables from these explicit facts; backends must not
    /// rediscover trait evidence or concrete implementation methods.
    CallDynamicTraitMethod {
        receiver: HirExpression,
        receiver_effect: HirDynamicTraitCallArgumentEffect,
        #[allow(dead_code)] // Reserved for backend validation and future table selection.
        trait_id: TraitId,
        requirement_id: TraitRequirementId,
        args: Vec<HirDynamicTraitCallArgument>,
        result: Option<LocalId>,
    },

    /// Expression evaluated only for side effects.
    Expr(HirExpression),

    /// Accumulate one runtime string value into the entry start() fragment vec.
    ///
    /// WHAT: explicit HIR primitive that lowers from `NodeKind::PushStartRuntimeFragment`.
    /// WHY: backends handle fragment accumulation without needing to inspect the entry start
    /// function body for heuristic push patterns.
    PushRuntimeFragment {
        /// The local holding the Vec<String> accumulator inside entry start().
        vec_local: LocalId,
        /// Expression that produces the string value to push.
        value: HirExpression,
    },

    /// Explicit deterministic drop.
    #[allow(dead_code)] // Planned: explicit drop statements after ownership lowering matures.
    Drop(LocalId),

    // -------------------------
    //  Map Builtins
    // -------------------------
    /// Perform a compiler-owned map builtin operation.
    ///
    /// WHAT: lowers `get`, `contains`, `set`, `remove`, `clear`, and `length` into an explicit
    ///       HIR statement so backends do not need to rediscover map builtin semantics.
    /// WHY: map operations are language builtins, not external package calls. Keeping them
    ///      as dedicated statements preserves receiver mutability, argument order, and
    ///      result local shape for borrow validation and backend lowering.
    MapOp {
        /// The specific builtin operation (get, contains, set, remove, clear, length).
        op: HirMapOp,
        /// The map value being operated on.
        receiver: HirExpression,
        /// Operation-specific arguments such as lookup keys or inserted values.
        args: Vec<HirExpression>,
        /// Local that receives the operation result, if any.
        result: Option<LocalId>,
    },
}

#[derive(Debug, Clone)]
pub struct HirDynamicTraitCallArgument {
    pub value: HirExpression,
    pub effect: HirDynamicTraitCallArgumentEffect,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HirDynamicTraitCallArgumentEffect {
    SharedBorrow,
    MayConsume,
}
