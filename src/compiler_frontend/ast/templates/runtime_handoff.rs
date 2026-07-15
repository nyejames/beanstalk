//! AST-owned runtime-template handoff exposed to HIR.
//!
//! HIR consumes finalized owned runtime-template handoff data.
//! HIR must not depend on TIR IDs, TIR stores, formatter views, slot routing internals, or
//! directive parsing.
//!
//! WHAT: owns the data shapes and structural walkers that form the AST/HIR boundary for
//! runtime templates and runtime slot applications.
//! WHY: the data shape is the AST/HIR boundary contract. Defining it outside the TIR module
//! keeps HIR independent of TIR-internal stores, views, overlays, and registry values while
//! keeping the handoff walkers co-located with the data they traverse.

use crate::compiler_frontend::ast::expressions::expression::Expression;
use crate::compiler_frontend::ast::templates::template::ReactiveSubscription;
use crate::compiler_frontend::ast::templates::template_control_flow::{
    TemplateBranchSelector, TemplateLoopControlKind, TemplateLoopHeader,
};
use crate::compiler_frontend::ast::templates::template_slots::{
    RuntimeSlotContributionSourceId, RuntimeSlotSiteId,
};
use crate::compiler_frontend::symbols::string_interning::StringId;
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;

/// Owned runtime slot-application plan prepared for HIR lowering.
///
/// WHAT: mirrors the routed source/site shape of a runtime slot plan, replacing
/// all structural template references with owned runtime-template nodes.
/// WHY: this handoff shape keeps HIR independent of the AST-side slot-plan
/// indexing model while preserving slot source order, site order, and
/// repeated-source replay.
#[derive(Clone, Debug)]
pub struct OwnedRuntimeSlotApplicationHandoff {
    pub(crate) wrapper: OwnedRuntimeTemplateNode,
    pub(crate) contribution_sources: Vec<OwnedRuntimeSlotContributionSource>,
    pub(crate) slot_sites: Vec<OwnedRuntimeSlotSite>,
    pub(crate) location: SourceLocation,
}

/// Runtime template value materialized for a child-template boundary.
#[derive(Clone, Debug)]
pub struct OwnedRuntimeTemplateHandoff {
    pub(crate) body: OwnedRuntimeTemplateBody,
    pub(crate) location: SourceLocation,
}

/// Runtime template body kind.
///
/// WHAT: distinguishes ordinary render trees from nested runtime slot
/// applications.
/// WHY: current HIR lowering gives runtime slot applications precedence over
/// linear/control-flow rendering. Keeping that distinction in the handoff lets
/// the later lowering slice preserve the same dispatch rule without looking
/// back at structural template data.
#[derive(Clone, Debug)]
pub(crate) enum OwnedRuntimeTemplateBody {
    Render(OwnedRuntimeTemplateNode),
    RuntimeSlotApplication(Box<OwnedRuntimeSlotApplicationHandoff>),
}

/// Owned runtime-template node prepared for the AST/HIR boundary.
///
/// WHAT: a self-contained, recursive tree representing one runtime template
/// fragment. No field carries a TIR store ID, node ID, view, overlay, or
/// registry value.
/// WHY: HIR lowering consumes this shape directly; keeping it free of
/// AST-stage identifiers lets the AST finalize and drop its TIR data before
/// HIR runs.
#[derive(Clone, Debug)]
pub(crate) enum OwnedRuntimeTemplateNode {
    Sequence {
        children: Vec<OwnedRuntimeTemplateNode>,
    },

    Text {
        text: StringId,
        byte_len: u32,
        reactive_subscription: Option<ReactiveSubscription>,
        location: SourceLocation,
    },

    DynamicExpression {
        expression: Box<Expression>,
        reactive_subscription: Option<ReactiveSubscription>,
    },

    ChildTemplate {
        template: Box<OwnedRuntimeTemplateHandoff>,
    },

    /// Output-conditioned wrapper application around one child occurrence.
    ///
    /// WHAT: renders `child` into a local accumulator, then renders `wrapper`
    /// only when the child structurally emitted output. The wrapper uses
    /// `AggregateOutput` as the child-output splice point.
    /// WHY: this is the neutral AST/HIR boundary shape for
    /// `TirWrapperApplicationMode::IfChildEmits`. HIR gets owned nodes only,
    /// not TIR overlay IDs or AST template handles.
    ConditionalWrapper {
        child: Box<OwnedRuntimeTemplateNode>,
        wrapper: Box<OwnedRuntimeTemplateNode>,
        location: SourceLocation,
    },

    BranchChain {
        branches: Vec<OwnedRuntimeTemplateBranch>,
        fallback: Option<Box<OwnedRuntimeTemplateNode>>,
        location: SourceLocation,
    },

    Loop {
        header: TemplateLoopHeader,
        body: Box<OwnedRuntimeTemplateNode>,
        aggregate_wrapper: Option<Box<OwnedRuntimeTemplateNode>>,
        location: SourceLocation,
    },

    AggregateOutput,

    LoopControl {
        kind: TemplateLoopControlKind,
        location: SourceLocation,
    },

    RuntimeSlotSite {
        site: RuntimeSlotSiteId,
    },

    /// Structural slot placeholder that survived as a runtime value.
    ///
    /// WHAT: mirrors a structural slot placeholder for wrapper-shaped templates
    /// that are used as ordinary runtime values. HIR rendering skips the
    /// placeholder and produces no output, matching the owned-handoff
    /// slot pass-through behavior.
    /// WHY: unresolved slot placeholders are not renderable chunks, but they
    /// are a valid structural no-output shape once wrapper composition has
    /// finished and the wrapper is treated as a value rather than a helper.
    Slot {
        location: SourceLocation,
    },
}

/// One owned conditional runtime-template branch.
#[derive(Clone, Debug)]
pub(crate) struct OwnedRuntimeTemplateBranch {
    pub(crate) selector: TemplateBranchSelector,
    pub(crate) body: OwnedRuntimeTemplateNode,
    pub(crate) location: SourceLocation,
}

/// Owned source-accumulator plan for one runtime slot contribution.
#[derive(Clone, Debug)]
pub(crate) struct OwnedRuntimeSlotContributionSource {
    pub(crate) source: RuntimeSlotContributionSourceId,
    pub(crate) render_root: OwnedRuntimeTemplateNode,
    pub(crate) renders_wrapper_unconditionally: bool,
    pub(crate) location: SourceLocation,
}

/// Owned runtime slot-site plan for one wrapper placeholder occurrence.
#[derive(Clone, Debug)]
pub(crate) struct OwnedRuntimeSlotSite {
    pub(crate) site: RuntimeSlotSiteId,
    pub(crate) render_plan: OwnedRuntimeSlotSiteRenderPlan,
    pub(crate) location: SourceLocation,
}

/// Owned render plan for a concrete runtime slot site.
#[derive(Clone, Debug, Default)]
pub(crate) struct OwnedRuntimeSlotSiteRenderPlan {
    pub(crate) pieces: Vec<OwnedRuntimeSlotSiteRenderPiece>,
}

/// One owned slot-site render piece.
#[derive(Clone, Debug)]
pub(crate) enum OwnedRuntimeSlotSiteRenderPiece {
    Render(OwnedRuntimeTemplateNode),
    ContributionSource(RuntimeSlotContributionSourceId),
}

/// Walks every nested `OwnedRuntimeTemplateNode` in `handoff` and calls `callback` for each.
///
/// WHAT: centralizes the structural recursion over the AST/HIR runtime-template handoff so
/// annotation, metadata merge, and HIR normalization do not each duplicate the walk.
/// WHY: the handoff shape is the neutral AST/HIR boundary; keeping its walker co-located with
/// the data prevents three separate local copies from drifting out of sync.
///
/// `callback` is invoked on a node before the walk recurses into its children, preserving the
/// document order of the previous local walkers. The callback may short-circuit
/// the walk by returning `Err`.
pub(crate) fn walk_owned_runtime_template_handoff<E>(
    handoff: &OwnedRuntimeTemplateHandoff,
    callback: &mut impl FnMut(&OwnedRuntimeTemplateNode) -> Result<(), E>,
) -> Result<(), E> {
    match &handoff.body {
        OwnedRuntimeTemplateBody::Render(node) => {
            walk_owned_runtime_template_node(node, callback)?;
        }

        OwnedRuntimeTemplateBody::RuntimeSlotApplication(handoff) => {
            walk_owned_runtime_slot_application_handoff(handoff, callback)?;
        }
    }

    Ok(())
}

/// Walks every nested `OwnedRuntimeTemplateNode` in `handoff` and calls `callback` for each.
///
/// See [`walk_owned_runtime_template_handoff`] for traversal order guarantees.
pub(crate) fn walk_owned_runtime_slot_application_handoff<E>(
    handoff: &OwnedRuntimeSlotApplicationHandoff,
    callback: &mut impl FnMut(&OwnedRuntimeTemplateNode) -> Result<(), E>,
) -> Result<(), E> {
    walk_owned_runtime_template_node(&handoff.wrapper, callback)?;

    for source in &handoff.contribution_sources {
        walk_owned_runtime_template_node(&source.render_root, callback)?;
    }

    for site in &handoff.slot_sites {
        walk_owned_runtime_slot_site_render_plan(&site.render_plan, callback)?;
    }

    Ok(())
}

fn walk_owned_runtime_slot_site_render_plan<E>(
    plan: &OwnedRuntimeSlotSiteRenderPlan,
    callback: &mut impl FnMut(&OwnedRuntimeTemplateNode) -> Result<(), E>,
) -> Result<(), E> {
    for piece in &plan.pieces {
        if let OwnedRuntimeSlotSiteRenderPiece::Render(node) = piece {
            walk_owned_runtime_template_node(node, callback)?;
        }
    }

    Ok(())
}

/// Walks every nested `OwnedRuntimeTemplateNode` in `node` and calls `callback` for each.
///
/// See [`walk_owned_runtime_template_handoff`] for traversal order guarantees.
pub(crate) fn walk_owned_runtime_template_node<E>(
    node: &OwnedRuntimeTemplateNode,
    callback: &mut impl FnMut(&OwnedRuntimeTemplateNode) -> Result<(), E>,
) -> Result<(), E> {
    callback(node)?;

    match node {
        OwnedRuntimeTemplateNode::Sequence { children, .. } => {
            for child in children {
                walk_owned_runtime_template_node(child, callback)?;
            }
        }

        OwnedRuntimeTemplateNode::ChildTemplate { template, .. } => {
            walk_owned_runtime_template_handoff(template, callback)?;
        }

        OwnedRuntimeTemplateNode::ConditionalWrapper { child, wrapper, .. } => {
            walk_owned_runtime_template_node(child, callback)?;
            walk_owned_runtime_template_node(wrapper, callback)?;
        }

        OwnedRuntimeTemplateNode::BranchChain {
            branches, fallback, ..
        } => {
            for branch in branches {
                walk_owned_runtime_template_node(&branch.body, callback)?;
            }

            if let Some(fallback) = fallback {
                walk_owned_runtime_template_node(fallback, callback)?;
            }
        }

        OwnedRuntimeTemplateNode::Loop {
            body,
            aggregate_wrapper,
            ..
        } => {
            walk_owned_runtime_template_node(body, callback)?;

            if let Some(aggregate_wrapper) = aggregate_wrapper {
                walk_owned_runtime_template_node(aggregate_wrapper, callback)?;
            }
        }

        OwnedRuntimeTemplateNode::Text { .. }
        | OwnedRuntimeTemplateNode::DynamicExpression { .. }
        | OwnedRuntimeTemplateNode::AggregateOutput
        | OwnedRuntimeTemplateNode::LoopControl { .. }
        | OwnedRuntimeTemplateNode::RuntimeSlotSite { .. }
        | OwnedRuntimeTemplateNode::Slot { .. } => {}
    }

    Ok(())
}

/// Events emitted by the mutable owned-runtime-template walker.
pub(crate) enum OwnedRuntimeTemplateWalkMutEvent<'a> {
    /// A runtime template node before its children are walked.
    Node(&'a mut OwnedRuntimeTemplateNode),
    /// A runtime template handoff after its body has been walked.
    ///
    /// WHAT: previously emitted after walking the handoff body so callers could
    ///       visit style child templates that were stored on `Style`. `Style` no
    ///       longer carries recursive wrapper templates, so this event now only
    ///       signals the boundary after the handoff body has been processed.
    HandoffAfterBody(&'a mut OwnedRuntimeTemplateHandoff),
}

/// Mutable variant of [`walk_owned_runtime_template_handoff`].
///
/// WHAT: same traversal order as the previous local mutable walkers, but allows
/// the callback to mutate each visited node or post-body handoff. The callback
/// may short-circuit the walk by returning `Err`.
/// WHY: HIR normalization and the annotation pass need to mutate expressions inside the
/// owned handoff; sharing one mutable walker avoids duplicating the recursion alongside
/// the immutable copy.
pub(crate) fn walk_owned_runtime_template_handoff_mut<E>(
    handoff: &mut OwnedRuntimeTemplateHandoff,
    callback: &mut impl FnMut(OwnedRuntimeTemplateWalkMutEvent<'_>) -> Result<(), E>,
) -> Result<(), E> {
    match &mut handoff.body {
        OwnedRuntimeTemplateBody::Render(node) => {
            walk_owned_runtime_template_node_mut(node, callback)?;
        }

        OwnedRuntimeTemplateBody::RuntimeSlotApplication(handoff) => {
            walk_owned_runtime_slot_application_handoff_mut(handoff, callback)?;
        }
    }

    callback(OwnedRuntimeTemplateWalkMutEvent::HandoffAfterBody(handoff))?;

    Ok(())
}

/// Mutable variant of [`walk_owned_runtime_slot_application_handoff`].
pub(crate) fn walk_owned_runtime_slot_application_handoff_mut<E>(
    handoff: &mut OwnedRuntimeSlotApplicationHandoff,
    callback: &mut impl FnMut(OwnedRuntimeTemplateWalkMutEvent<'_>) -> Result<(), E>,
) -> Result<(), E> {
    walk_owned_runtime_template_node_mut(&mut handoff.wrapper, callback)?;

    for source in &mut handoff.contribution_sources {
        walk_owned_runtime_template_node_mut(&mut source.render_root, callback)?;
    }

    for site in &mut handoff.slot_sites {
        walk_owned_runtime_slot_site_render_plan_mut(&mut site.render_plan, callback)?;
    }

    Ok(())
}

fn walk_owned_runtime_slot_site_render_plan_mut<E>(
    plan: &mut OwnedRuntimeSlotSiteRenderPlan,
    callback: &mut impl FnMut(OwnedRuntimeTemplateWalkMutEvent<'_>) -> Result<(), E>,
) -> Result<(), E> {
    for piece in &mut plan.pieces {
        if let OwnedRuntimeSlotSiteRenderPiece::Render(node) = piece {
            walk_owned_runtime_template_node_mut(node, callback)?;
        }
    }

    Ok(())
}

/// Mutable variant of [`walk_owned_runtime_template_node`].
pub(crate) fn walk_owned_runtime_template_node_mut<E>(
    node: &mut OwnedRuntimeTemplateNode,
    callback: &mut impl FnMut(OwnedRuntimeTemplateWalkMutEvent<'_>) -> Result<(), E>,
) -> Result<(), E> {
    callback(OwnedRuntimeTemplateWalkMutEvent::Node(node))?;

    match node {
        OwnedRuntimeTemplateNode::Sequence { children, .. } => {
            for child in children {
                walk_owned_runtime_template_node_mut(child, callback)?;
            }
        }

        OwnedRuntimeTemplateNode::ChildTemplate { template, .. } => {
            walk_owned_runtime_template_handoff_mut(template, callback)?;
        }

        OwnedRuntimeTemplateNode::ConditionalWrapper { child, wrapper, .. } => {
            walk_owned_runtime_template_node_mut(child, callback)?;
            walk_owned_runtime_template_node_mut(wrapper, callback)?;
        }

        OwnedRuntimeTemplateNode::BranchChain {
            branches, fallback, ..
        } => {
            for branch in branches {
                walk_owned_runtime_template_node_mut(&mut branch.body, callback)?;
            }

            if let Some(fallback) = fallback {
                walk_owned_runtime_template_node_mut(fallback, callback)?;
            }
        }

        OwnedRuntimeTemplateNode::Loop {
            body,
            aggregate_wrapper,
            ..
        } => {
            walk_owned_runtime_template_node_mut(body, callback)?;

            if let Some(aggregate_wrapper) = aggregate_wrapper {
                walk_owned_runtime_template_node_mut(aggregate_wrapper, callback)?;
            }
        }

        OwnedRuntimeTemplateNode::Text { .. }
        | OwnedRuntimeTemplateNode::DynamicExpression { .. }
        | OwnedRuntimeTemplateNode::AggregateOutput
        | OwnedRuntimeTemplateNode::LoopControl { .. }
        | OwnedRuntimeTemplateNode::RuntimeSlotSite { .. }
        | OwnedRuntimeTemplateNode::Slot { .. } => {}
    }

    Ok(())
}
