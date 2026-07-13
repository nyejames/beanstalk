//! Control-flow root installation.
//!
//! WHAT: installs TIR-derived control-flow body roots after render-unit
//!       preparation.
//! WHY: production readers need a single owner for same-store control-flow
//!      body replacement.

use crate::compiler_frontend::ast::templates::tir::{TemplateIrNodeId, TemplateIrStore};
use crate::compiler_frontend::compiler_errors::CompilerError;

// -------------------------
//  Control-flow body target
// -------------------------

/// Identifies which body inside a control-flow TIR node should receive a
/// finalized simple TIR root.
///
/// WHAT: branch chains have multiple branch bodies plus an optional fallback;
///       loops have one body. This enum lets the store helper locate the right
///       body node ID without exposing vector indexes directly.
/// WHY: keeps `TemplateIrStore` replacement logic independent of AST branch
///      types while still describing the replacement target precisely.
#[derive(Clone, Copy, Debug)]
pub(crate) enum ControlFlowBodyKind {
    /// A branch in a `BranchChain`, identified by its position in the branch vector.
    Branch { index: usize },

    /// The optional fallback body of a `BranchChain`.
    Fallback,

    /// The per-iteration body of a `Loop`.
    LoopBody,
}

/// Replaces one control-flow body node with an already-built TIR root.
///
/// WHAT: points the requested body slot on `control_flow_node_id` at
///       `new_body_root`.
/// WHY: TIR-derived branch/fallback and loop body paths need the same
///      replacement step. Sharing it keeps mutation in the TIR store owner so
///      callers only differ in how they build the root. Returns `CompilerError`
///      when the parser TIR control-flow node or body slot is missing.
pub(crate) fn replace_control_flow_body_tir_root(
    store: &mut TemplateIrStore,
    control_flow_node_id: TemplateIrNodeId,
    body_kind: ControlFlowBodyKind,
    new_body_root: TemplateIrNodeId,
) -> Result<(), CompilerError> {
    if !store.replace_control_flow_body_node_by_id(control_flow_node_id, body_kind, new_body_root) {
        return Err(CompilerError::compiler_error(
            "Control-flow body replacement failed to install the prepared root onto the parser TIR node.",
        ));
    }

    Ok(())
}

/// Replaces the aggregate-wrapper subtree on the owning template's same-store
/// `Loop` control-flow node.
///
/// WHAT: installs `new_aggregate_wrapper_root` on the supplied TIR `Loop` node.
/// WHY: loop aggregate wrappers are now composed as normal TIR subtrees during
///      render-unit preparation. Keeping the mutation here makes its invariant
///      handling consistent with body-root replacement.
pub(crate) fn replace_loop_aggregate_wrapper_tir_root(
    store: &mut TemplateIrStore,
    control_flow_node_id: TemplateIrNodeId,
    new_aggregate_wrapper_root: TemplateIrNodeId,
) -> Result<(), CompilerError> {
    if !store
        .replace_loop_aggregate_wrapper_node_by_id(control_flow_node_id, new_aggregate_wrapper_root)
    {
        return Err(CompilerError::compiler_error(
            "Loop aggregate-wrapper installation failed to replace the wrapper root on the parser TIR loop node.",
        ));
    }

    Ok(())
}
