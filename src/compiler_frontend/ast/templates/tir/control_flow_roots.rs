//! Control-flow root installation and resolution.
//!
//! WHAT: installs and resolves TIR-derived control-flow body roots after
//! render-unit preparation.
//! WHY: production readers need a single owner for same-store control-flow
//! body replacement and finalized body-root resolution.

use crate::compiler_frontend::ast::templates::template_types::Template;
use crate::compiler_frontend::ast::templates::tir::{
    TemplateIrNodeId, TemplateIrStore, TemplateParserIrBuilderState, TemplateTirBodyReference,
    TemplateTirPhase,
};
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;
use std::sync::Arc;

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
/// WHAT: locates the owning control-flow node through the in-progress parser
///       builder, then points the requested body slot at `new_body_root`.
/// WHY: TIR-derived branch/fallback and loop body paths need the same
///      control-flow-node lookup and replacement step. Sharing it keeps the
///      store-owner proof and replacement logic in one place so callers only
///      differ in how they build the root. Returns `CompilerError` when the
///      parser TIR control-flow node or body slot is missing.
pub(crate) fn replace_control_flow_body_tir_root(
    builder: &TemplateParserIrBuilderState,
    store: &mut TemplateIrStore,
    body_kind: ControlFlowBodyKind,
    new_body_root: TemplateIrNodeId,
    phase: TemplateTirPhase,
    location: SourceLocation,
) -> Result<TemplateTirBodyReference, CompilerError> {
    let control_flow_node_id = builder.control_flow_node_id(store).ok_or_else(|| {
        CompilerError::compiler_error(
            "Control-flow body replacement could not find the owning parser TIR control-flow node.",
        )
    })?;

    if !store.replace_control_flow_body_node_by_id(control_flow_node_id, body_kind, new_body_root) {
        return Err(CompilerError::compiler_error(
            "Control-flow body replacement failed to install the prepared root onto the parser TIR node.",
        ));
    }

    Ok(TemplateTirBodyReference::new(
        store.owner(),
        store.store_id(),
        new_body_root,
        phase,
        location,
    ))
}

/// Replaces the aggregate-wrapper subtree on the owning template's same-store
/// `Loop` control-flow node.
///
/// WHAT: finds the loop node through the same owner/template-or-builder lookup
///       used by `replace_control_flow_body_tir_root`, then installs
///       `new_aggregate_wrapper_root` as the `aggregate_wrapper` field.
/// WHY: loop aggregate wrappers are now composed as normal TIR subtrees during
///      render-unit preparation. Sharing the lookup keeps the installation path
///      consistent with body-root replacement.
pub(crate) fn replace_loop_aggregate_wrapper_tir_root(
    builder: &TemplateParserIrBuilderState,
    store: &mut TemplateIrStore,
    new_aggregate_wrapper_root: TemplateIrNodeId,
) -> Result<(), CompilerError> {
    let control_flow_node_id = builder.control_flow_node_id(store).ok_or_else(|| {
        CompilerError::compiler_error(
            "Loop aggregate-wrapper installation could not find the owning parser TIR loop node.",
        )
    })?;

    if !store
        .replace_loop_aggregate_wrapper_node_by_id(control_flow_node_id, new_aggregate_wrapper_root)
    {
        return Err(CompilerError::compiler_error(
            "Loop aggregate-wrapper installation failed to replace the wrapper root on the parser TIR loop node.",
        ));
    }

    Ok(())
}

/// Resolves a finalized control-flow body root from the module TIR store.
///
/// WHAT: finds the owning control-flow node through the template's same-store
/// finalized reference, then returns the requested body root as a
/// store-proven reference.
/// WHY: production readers use this TIR-owned root after render-unit
/// preparation installs finalized bodies into the store. A missing same-store
/// root means the template is not available through the TIR authority.
pub(crate) fn finalized_control_flow_body_tir_reference(
    template: &Template,
    store: &TemplateIrStore,
    body_kind: ControlFlowBodyKind,
) -> Option<TemplateTirBodyReference> {
    let store_owner = store.owner();
    let control_flow_node_id = template
        .tir_reference
        .as_ref()
        .filter(|reference| Arc::ptr_eq(&reference.store_owner, &store_owner))
        .and_then(|reference| {
            store.control_flow_node_id_for_template(reference.root.template_id)
        })?;

    let root = store.control_flow_body_node_id_by_id(control_flow_node_id, body_kind)?;

    Some(TemplateTirBodyReference::new(
        store_owner.clone(),
        store.store_id(),
        root,
        template
            .tir_reference
            .as_ref()
            .map(|reference| reference.phase)
            .unwrap_or(TemplateTirPhase::Composed),
        template.location.to_owned(),
    ))
}
