//! TIR-native formatter view.
//!
//! WHAT: feeds existing formatter algorithms from `TirView` and effective
//! `TemplateIrNodeKind` snapshots. Formatted output is mapped directly back to
//! append-only TIR nodes after view extraction finishes.
//!
//! WHY: removes the formatter-dependent representation ping-pong for compile-time
//! template folding while preserving formatter behavior. Existing formatter
//! algorithms (`$md`, `$raw`, etc.) stay unchanged; this module is only
//! the adapter that presents TIR data as `FormatterInput` and rebuilds TIR from
//! `FormatterOutput`.
//!
//! ## Production authority
//!
//! Linear templates format directly from TIR. Control-flow body roots are
//! also formatted natively in TIR through this adapter. No intermediate
//! content-to-TIR conversion step remains in the production render-unit path.

use crate::compiler_frontend::ast::templates::formatter_contract::{
    FormatterAnchorId, FormatterInput, FormatterInputPiece, FormatterOpaqueKind,
    FormatterOpaquePiece, FormatterOutputPiece, FormatterTextPiece, output_to_input,
};
use crate::compiler_frontend::ast::templates::styles::whitespace::{
    TemplateBodyRunPosition, TemplateWhitespacePassProfile, apply_whitespace_passes_to_input,
};
use crate::compiler_frontend::ast::templates::template::{
    BodyWhitespacePolicy, ReactiveSubscription, Style, TemplateSegmentOrigin,
};
use crate::compiler_frontend::ast::templates::tir::ids::{TemplateIrId, TemplateIrNodeId};
use crate::compiler_frontend::ast::templates::tir::node::{TemplateIrNode, TemplateIrNodeKind};
use crate::compiler_frontend::ast::templates::tir::overlays::TemplateOverlaySetId;
use crate::compiler_frontend::ast::templates::tir::refs::{
    TemplateNodeRef, TemplateRef, TemplateStoreId, TemplateTirChildReference,
};
use crate::compiler_frontend::ast::templates::tir::view::{TemplateTirPhase, TirView};
use crate::compiler_frontend::compiler_errors::{CompilerError, CompilerMessages};
use crate::compiler_frontend::compiler_messages::CompilerDiagnostic;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;
use std::collections::HashSet;

// -------------------------
//  Public result type
// -------------------------

/// Result of applying a style formatter to a TIR subtree.
///
/// WHAT: carries the new root node ID for the formatted tree and any formatter
/// warnings.
/// WHY: callers replace the original template root with `root` and forward
/// warnings to the appropriate diagnostic context.
pub(crate) struct TirFormatterResult {
    pub root: TemplateIrNodeId,
    pub warnings: Vec<CompilerDiagnostic>,
}

// -------------------------
//  Public entry point
// -------------------------

/// Formats a TIR template body tree using its style formatter.
///
/// WHAT: walks the root node of a template already stored in `store`,
///       identifies contiguous body-eligible runs, and runs the shared
///       whitespace/formatter pipeline on each run. Opaque anchors (child
///       templates, dynamic expressions) are preserved; head-origin content and
///       structural nodes break runs and pass through unchanged.
/// WHY: this keeps formatter behavior on the authoritative TIR representation.
pub(crate) fn format_tir_template(
    view: &TirView<'_>,
    style: &Style,
    string_table: &mut StringTable,
) -> Result<TirFormatterResult, CompilerMessages> {
    let root_ref = view.root_ref();
    let root_node_id = {
        let template = view
            .root_template()
            .map_err(|error| compiler_error_messages(error, string_table))?;
        template.root
    };

    let formatter = style.formatter.as_ref();

    let implicit_default_whitespace_pass = (style.body_whitespace_policy
        == BodyWhitespacePolicy::DefaultTemplateBehavior
        && formatter.is_none())
    .then_some(TemplateWhitespacePassProfile::default_template_body());

    if implicit_default_whitespace_pass.is_none() && formatter.is_none() {
        return Ok(TirFormatterResult {
            root: root_node_id,
            warnings: Vec::new(),
        });
    }

    let pre_format_passes = formatter
        .map(|f| f.pre_format_whitespace_passes.as_slice())
        .unwrap_or_else(|| {
            if let Some(pass) = &implicit_default_whitespace_pass {
                std::slice::from_ref(pass)
            } else {
                &[]
            }
        });

    let post_format_passes = formatter
        .map(|f| f.post_format_whitespace_passes.as_slice())
        .unwrap_or(&[]);

    let root_node_ref = TemplateNodeRef::new(root_ref.store_id, root_node_id);
    let result = format_tir_node(
        view,
        root_node_ref,
        pre_format_passes,
        post_format_passes,
        formatter,
        string_table,
    )?;

    // Child templates are opaque to the parent formatter, but they still need
    // their own formatter applied before folding. Recursively format every
    // reachable child template so the fold path sees formatted bodies.
    let mut visited = HashSet::new();
    let formatted_root_ref = TemplateNodeRef::new(root_ref.store_id, result.root);
    format_child_templates_in_subtree(view, formatted_root_ref, &mut visited, string_table)?;

    Ok(result)
}

/// Cheap structural facts extracted from a TIR node for child-template
/// formatting traversal.
///
/// WHAT: carries only the IDs and references needed to continue recursion or
///       format a referenced child template, without cloning the entire
///       `TemplateIrNode`.
/// WHY: the `TirView` `RefCell` borrow must end before recursive calls that may
///      mutate the store. Extracting cheap facts while the node is borrowed and
///      acting after the borrow ends avoids whole-node clones.
enum FormatterChildFact {
    ChildTemplate {
        reference: TemplateTirChildReference,
    },
    Sequence(Vec<TemplateIrNodeId>),
    BranchChain {
        branch_bodies: Vec<TemplateIrNodeId>,
        fallback: Option<TemplateIrNodeId>,
    },
    Loop {
        body: TemplateIrNodeId,
        aggregate_wrapper: Option<TemplateIrNodeId>,
    },
    InsertContribution {
        template: TemplateIrId,
    },
    Other,
}

/// Extracts the cheap structural facts needed for child-template formatting
/// traversal from a node kind, without cloning the entire node.
fn extract_formatter_child_fact(kind: &TemplateIrNodeKind) -> FormatterChildFact {
    match kind {
        TemplateIrNodeKind::ChildTemplate { reference, .. } => FormatterChildFact::ChildTemplate {
            reference: *reference,
        },
        TemplateIrNodeKind::Sequence { children } => FormatterChildFact::Sequence(children.clone()),
        TemplateIrNodeKind::BranchChain { branches, fallback } => FormatterChildFact::BranchChain {
            branch_bodies: branches.iter().map(|branch| branch.body).collect(),
            fallback: *fallback,
        },
        TemplateIrNodeKind::Loop {
            body,
            aggregate_wrapper,
            ..
        } => FormatterChildFact::Loop {
            body: *body,
            aggregate_wrapper: *aggregate_wrapper,
        },
        TemplateIrNodeKind::InsertContribution { template } => {
            FormatterChildFact::InsertContribution {
                template: *template,
            }
        }
        _ => FormatterChildFact::Other,
    }
}

/// Recursively formats child templates reachable from a TIR subtree.
///
/// WHAT: walks the formatted tree under `node_id` and calls `format_tir_template`
///       on every `ChildTemplate` reference that has not already been visited.
/// WHY: parent formatters treat children as opaque anchors, so the parent's own
///      formatting pass does not format nested children. This pass ensures each
///      child template is formatted independently before folding.
fn format_child_templates_in_subtree(
    view: &TirView<'_>,
    node_ref: TemplateNodeRef,
    visited: &mut HashSet<TemplateRef>,
    string_table: &mut StringTable,
) -> Result<(), CompilerMessages> {
    let fact = {
        let node = view
            .effective_node(node_ref)
            .map_err(|error| compiler_error_messages(error, string_table))?;
        extract_formatter_child_fact(&node.kind)
    };

    match fact {
        FormatterChildFact::ChildTemplate { reference } if visited.insert(reference.root) => {
            format_referenced_child_template(
                view,
                reference.root,
                reference.phase,
                reference.overlay_set_id,
                string_table,
            )?;
        }

        FormatterChildFact::Sequence(children) => {
            for child_id in children {
                let child_ref = TemplateNodeRef::new(node_ref.store_id, child_id);
                format_child_templates_in_subtree(view, child_ref, visited, string_table)?;
            }
        }

        FormatterChildFact::BranchChain {
            branch_bodies,
            fallback,
        } => {
            for body_id in branch_bodies {
                let branch_ref = TemplateNodeRef::new(node_ref.store_id, body_id);
                format_child_templates_in_subtree(view, branch_ref, visited, string_table)?;
            }

            if let Some(fallback_id) = fallback {
                let fallback_ref = TemplateNodeRef::new(node_ref.store_id, fallback_id);
                format_child_templates_in_subtree(view, fallback_ref, visited, string_table)?;
            }
        }

        FormatterChildFact::Loop {
            body,
            aggregate_wrapper,
        } => {
            let body_ref = TemplateNodeRef::new(node_ref.store_id, body);
            format_child_templates_in_subtree(view, body_ref, visited, string_table)?;

            if let Some(aggregate_id) = aggregate_wrapper {
                let aggregate_ref = TemplateNodeRef::new(node_ref.store_id, aggregate_id);
                format_child_templates_in_subtree(view, aggregate_ref, visited, string_table)?;
            }
        }

        FormatterChildFact::InsertContribution { template }
            if visited.insert(TemplateRef::new(node_ref.store_id, template)) =>
        {
            let child_ref = TemplateRef::new(node_ref.store_id, template);
            // InsertContribution nodes reference SlotInsert templates that are
            // always at Formatted phase: create_template_node formats every
            // same-store linear template before the parser records the insert
            // contribution. Using Formatted prevents re-formatting an already
            // formatted root.
            format_referenced_child_template(
                view,
                child_ref,
                TemplateTirPhase::Formatted,
                view.overlay_set_id(),
                string_table,
            )?;
        }

        _ => {}
    }

    Ok(())
}

/// Formats a single child/insert template referenced by ID and updates its
/// root in the store.
///
/// WHAT: looks up the referenced template, formats it with its own style, and
///       writes the formatted root back into the store.
/// WHY: both `ChildTemplate` and `InsertContribution` nodes reference nested
///      templates that need independent formatting before folding.
fn format_referenced_child_template(
    view: &TirView<'_>,
    template_ref: TemplateRef,
    phase: TemplateTirPhase,
    overlay_set_id: TemplateOverlaySetId,
    string_table: &mut StringTable,
) -> Result<(), CompilerMessages> {
    let child_view = view
        .child_view(template_ref, phase, overlay_set_id)
        .map_err(|error| compiler_error_messages(error, string_table))?;

    let style = {
        let template = child_view
            .root_template()
            .map_err(|error| compiler_error_messages(error, string_table))?;
        template.style.clone()
    };

    // A child template whose reference phase has already reached Formatted
    // carries a formatted root and must not be re-formatted. Re-formatting
    // would double-escape output such as markdown paragraphs.
    let already_formatted =
        style.formatter.is_some() && phase.is_at_least(TemplateTirPhase::Formatted);

    if already_formatted {
        return Ok(());
    }

    let result = format_tir_template(&child_view, &style, string_table)?;

    let mut store = view
        .registry_ref()
        .store_mut(template_ref.store_id)
        .map_err(|error| compiler_error_messages(error, string_table))?;
    let Some(template) = store.templates.get_mut(template_ref.template_id.index()) else {
        return Err(compiler_error_messages(
            CompilerError::compiler_error(format!(
                "TIR formatter view lost referenced child template {} during writeback.",
                template_ref
            )),
            string_table,
        ));
    };
    template.root = result.root;

    Ok(())
}

// -------------------------
//  Recursive node formatting
// -------------------------

/// Cheap structural facts extracted from a TIR node for formatter dispatch.
///
/// WHAT: carries only the children IDs and source location needed to format a
///       single node, without cloning the entire `TemplateIrNode`.
/// WHY: the `TirView` `RefCell` borrow must end before formatting calls that
///      may mutate the store. Extracting cheap facts while the node is borrowed
///      and acting after the borrow ends avoids whole-node clones.
enum FormatterNodeFact {
    Sequence {
        children: Vec<TemplateIrNodeId>,
        location: SourceLocation,
    },
    BodyEligible {
        location: SourceLocation,
    },
    Passthrough,
}

/// Extracts the cheap structural facts needed for formatter dispatch from a
/// node, without cloning the entire node.
fn extract_formatter_node_fact(node: &TemplateIrNode) -> FormatterNodeFact {
    match &node.kind {
        TemplateIrNodeKind::Sequence { children } => FormatterNodeFact::Sequence {
            children: children.clone(),
            location: node.location.clone(),
        },
        _ if is_body_eligible_kind(&node.kind) => FormatterNodeFact::BodyEligible {
            location: node.location.clone(),
        },
        _ => FormatterNodeFact::Passthrough,
    }
}

/// Formats a single TIR node and returns the formatted root for that subtree.
///
/// WHAT: sequences are scanned for body runs; single body-eligible nodes are
/// wrapped in a synthetic run; all other node kinds pass through unchanged.
/// WHY: formatter bodies are flat runs of text and opaque anchors. Recursing
/// into child templates would violate opacity, and control-flow nodes are not
/// expected in a simple formatter body.
fn format_tir_node(
    view: &TirView<'_>,
    node_ref: TemplateNodeRef,
    pre_format_passes: &[TemplateWhitespacePassProfile],
    post_format_passes: &[TemplateWhitespacePassProfile],
    formatter: Option<&crate::compiler_frontend::ast::templates::template::Formatter>,
    string_table: &mut StringTable,
) -> Result<TirFormatterResult, CompilerMessages> {
    let fact = {
        let node = view
            .effective_node(node_ref)
            .map_err(|error| compiler_error_messages(error, string_table))?;
        extract_formatter_node_fact(&node)
    };

    match fact {
        FormatterNodeFact::Sequence { children, location } => {
            let head_node_count = view
                .root_template()
                .ok()
                .filter(|template| template.root == node_ref.node_id)
                .map_or(0, |template| template.summary.head_node_count as usize);

            format_tir_sequence(
                view,
                node_ref,
                &children,
                location,
                head_node_count,
                pre_format_passes,
                post_format_passes,
                formatter,
                string_table,
            )
        }

        FormatterNodeFact::BodyEligible { location } => {
            // A single body-eligible node is treated as a run of one. It is not
            // wrapped in a sequence unless the formatter expands it.
            let representative_location =
                representative_location_for_single_node(view, node_ref, string_table)?;
            let (replacement_nodes, warnings, content_changed) = process_formatter_run(
                view,
                node_ref.store_id,
                std::slice::from_ref(&node_ref.node_id),
                TemplateBodyRunPosition::Only,
                &representative_location,
                pre_format_passes,
                post_format_passes,
                formatter,
                string_table,
            )?;

            let root = if replacement_nodes.len() == 1 && !content_changed {
                replacement_nodes[0]
            } else {
                push_formatter_node(
                    view,
                    node_ref.store_id,
                    TemplateIrNode::new(
                        TemplateIrNodeKind::Sequence {
                            children: replacement_nodes,
                        },
                        location,
                    ),
                    None,
                    string_table,
                )?
            };

            Ok(TirFormatterResult { root, warnings })
        }

        // Structural nodes that are not body-eligible pass through unchanged.
        FormatterNodeFact::Passthrough => Ok(TirFormatterResult {
            root: node_ref.node_id,
            warnings: Vec::new(),
        }),
    }
}

/// Cheap eligibility facts for a child node during sequence formatting.
///
/// WHAT: carries only the two boolean facts needed for run-membership decisions,
///       extracted while the node is borrowed so the `TemplateIrNodeKind` clone
///       is avoided.
struct ChildRunEligibility {
    is_child_template: bool,
    is_body_eligible: bool,
}

/// Formats a sequence node by scanning its children for contiguous body runs.
///
/// WHAT: children that are body-eligible form formatter runs; everything else
/// terminates the current run. Each run is processed independently, and the
/// resulting nodes are spliced back in order.
/// WHY: formatter runs operate on TIR node IDs while keeping structural nodes
/// outside the formatter-visible surface.
#[allow(clippy::too_many_arguments)]
fn format_tir_sequence(
    view: &TirView<'_>,
    original_node_ref: TemplateNodeRef,
    children: &[TemplateIrNodeId],
    location: SourceLocation,
    head_node_count: usize,
    pre_format_passes: &[TemplateWhitespacePassProfile],
    post_format_passes: &[TemplateWhitespacePassProfile],
    formatter: Option<&crate::compiler_frontend::ast::templates::template::Formatter>,
    string_table: &mut StringTable,
) -> Result<TirFormatterResult, CompilerMessages> {
    let mut new_children: Vec<TemplateIrNodeId> = Vec::with_capacity(children.len());
    let mut current_run: Vec<TemplateIrNodeId> = Vec::new();
    let mut all_warnings: Vec<CompilerDiagnostic> = Vec::new();
    let mut content_changed = false;
    let mut is_first_run = true;

    for (child_index, &child_id) in children.iter().enumerate() {
        let child_ref = TemplateNodeRef::new(original_node_ref.store_id, child_id);
        let child_eligibility = {
            let child = view
                .effective_node(child_ref)
                .map_err(|error| compiler_error_messages(error, string_table))?;
            ChildRunEligibility {
                is_child_template: matches!(child.kind, TemplateIrNodeKind::ChildTemplate { .. }),
                is_body_eligible: is_body_eligible_kind(&child.kind),
            }
        };

        let is_head_child_template =
            child_index < head_node_count && child_eligibility.is_child_template;
        let is_eligible = child_eligibility.is_body_eligible && !is_head_child_template;

        if is_eligible {
            current_run.push(child_id);
            continue;
        }

        if !current_run.is_empty() {
            let run_position = run_position_for_run(is_first_run, false);
            let representative_location = representative_location_for_run(
                view,
                original_node_ref.store_id,
                &current_run,
                string_table,
            )?;

            let (replacement, warnings, run_changed) = process_formatter_run(
                view,
                original_node_ref.store_id,
                &current_run,
                run_position,
                &representative_location,
                pre_format_passes,
                post_format_passes,
                formatter,
                string_table,
            )?;

            new_children.extend(replacement);
            all_warnings.extend(warnings);
            content_changed |= run_changed;
            current_run.clear();
            is_first_run = false;
        }

        new_children.push(child_id);
    }

    if !current_run.is_empty() {
        let run_position = run_position_for_run(is_first_run, true);
        let representative_location = representative_location_for_run(
            view,
            original_node_ref.store_id,
            &current_run,
            string_table,
        )?;

        let (replacement, warnings, run_changed) = process_formatter_run(
            view,
            original_node_ref.store_id,
            &current_run,
            run_position,
            &representative_location,
            pre_format_passes,
            post_format_passes,
            formatter,
            string_table,
        )?;

        new_children.extend(replacement);
        all_warnings.extend(warnings);
        content_changed |= run_changed;
    }

    let root = if !content_changed && new_children.len() == children.len() {
        // Fast path: nothing changed, so the original node is still valid.
        original_node_ref.node_id
    } else {
        push_formatter_node(
            view,
            original_node_ref.store_id,
            TemplateIrNode::new(
                TemplateIrNodeKind::Sequence {
                    children: new_children,
                },
                location,
            ),
            None,
            string_table,
        )?
    };

    Ok(TirFormatterResult {
        root,
        warnings: all_warnings,
    })
}

// -------------------------
//  Run membership
// -------------------------

/// Returns true when a node kind can participate in a contiguous formatter run.
///
/// WHAT: body text, body dynamic expressions, and opaque child templates are
/// formatter-visible. Head-origin text/expressions and structural nodes break
/// runs.
/// WHY: head nodes and structural control flow aren't body-formatting input.
fn is_body_eligible_kind(kind: &TemplateIrNodeKind) -> bool {
    match kind {
        TemplateIrNodeKind::Text { origin, .. } => *origin == TemplateSegmentOrigin::Body,

        TemplateIrNodeKind::DynamicExpression { origin, .. } => {
            *origin == TemplateSegmentOrigin::Body
        }

        TemplateIrNodeKind::ChildTemplate { .. } => true,

        _ => false,
    }
}

/// Returns true when a `ChildTemplate` TIR node references a head-expression
/// insert child.
///
/// WHAT: a head-expression insert child has a TIR root that consists only of
/// head-origin `Text` nodes. Such children are opaque expression anchors to the
/// parent formatter, not sealed child-template boundaries, so they must be
/// classified as `FormatterOpaqueKind::DynamicExpression`.
/// WHY: markdown inline-code pairing must work across inserted scalar strings
/// without opening body-bearing child templates to the parent formatter.
fn child_template_is_head_expression_insert_in_tir(
    view: &TirView<'_>,
    reference: &TemplateTirChildReference,
) -> Result<bool, CompilerError> {
    // Cross-store child templates cannot be inspected from this view; stay
    // conservative and treat them as opaque child-template boundaries.
    if reference.root.store_id != view.root_ref().store_id {
        return Ok(false);
    }

    let child_view = view.child_view(reference.root, reference.phase, reference.overlay_set_id)?;
    let child_template = child_view.root_template()?;
    let root_node_ref = TemplateNodeRef::new(child_view.root_ref().store_id, child_template.root);
    let root_node = child_view.effective_node(root_node_ref)?;

    let candidate_ids = match &root_node.kind {
        TemplateIrNodeKind::Sequence { children } => children.as_slice(),
        _ => std::slice::from_ref(&root_node_ref.node_id),
    };

    if candidate_ids.is_empty() {
        return Ok(false);
    }

    for node_id in candidate_ids {
        let node_ref = TemplateNodeRef::new(root_node_ref.store_id, *node_id);
        let node = child_view.effective_node(node_ref)?;

        match &node.kind {
            TemplateIrNodeKind::Text { origin, .. } if *origin == TemplateSegmentOrigin::Head => {}
            _ => return Ok(false),
        }
    }

    Ok(true)
}

/// Classifies a body-eligible node kind into the opaque anchor kind used by the
/// formatter pipeline.
///
/// WHAT: child-template nodes become `ChildTemplate` anchors unless they are
/// head-expression inserts, which become `DynamicExpression` anchors; body
/// dynamic expressions become `DynamicExpression` anchors.
/// WHY: the `$md` inline-code pass distinguishes these two kinds without
/// inspecting its content, and head-expression inserts must pair like direct
/// dynamic-expression anchors. Accepting the kind directly avoids a repeated
/// `effective_node` read when the caller already holds the node borrow.
fn opaque_kind_for_kind(
    view: &TirView<'_>,
    kind: &TemplateIrNodeKind,
) -> Result<FormatterOpaqueKind, CompilerError> {
    match kind {
        TemplateIrNodeKind::ChildTemplate { reference, .. } => {
            if child_template_is_head_expression_insert_in_tir(view, reference)? {
                Ok(FormatterOpaqueKind::DynamicExpression)
            } else {
                Ok(FormatterOpaqueKind::ChildTemplate)
            }
        }
        TemplateIrNodeKind::DynamicExpression { .. } => Ok(FormatterOpaqueKind::DynamicExpression),

        _ => Err(CompilerError::compiler_error(format!(
            "TIR formatter view attempted to anchor unsupported node kind: {:?}",
            kind
        ))),
    }
}

// -------------------------
//  Run processing
// -------------------------

/// Processes one contiguous formatter run through whitespace passes and the
/// style formatter.
///
/// WHAT: builds `FormatterInput` from the run, runs pre-format whitespace
/// passes, the formatter, and post-format whitespace passes, then maps the
/// output back to TIR node IDs using a local anchor side-table.
/// WHY: this is the core adapter step that lets existing formatters consume TIR
/// data and produce TIR data.
#[allow(clippy::too_many_arguments)]
fn process_formatter_run(
    view: &TirView<'_>,
    store_id: TemplateStoreId,
    run: &[TemplateIrNodeId],
    run_position: TemplateBodyRunPosition,
    representative_location: &SourceLocation,
    pre_format_passes: &[TemplateWhitespacePassProfile],
    post_format_passes: &[TemplateWhitespacePassProfile],
    formatter: Option<&crate::compiler_frontend::ast::templates::template::Formatter>,
    string_table: &mut StringTable,
) -> Result<(Vec<TemplateIrNodeId>, Vec<CompilerDiagnostic>, bool), CompilerMessages> {
    if run.is_empty() {
        return Ok((Vec::new(), Vec::new(), false));
    }

    let mut input_pieces: Vec<FormatterInputPiece> = Vec::with_capacity(run.len());
    let mut anchor_side_table: Vec<TemplateIrNodeId> = Vec::with_capacity(run.len());
    let mut run_reactive_subscription: Option<ReactiveSubscription> = None;

    for &node_id in run {
        let node_ref = TemplateNodeRef::new(store_id, node_id);
        let node = view
            .effective_node(node_ref)
            .map_err(|error| compiler_error_messages(error, string_table))?;

        match &node.kind {
            TemplateIrNodeKind::Text { text, .. } => {
                if run_reactive_subscription.is_none()
                    && let Some(store) = view.registry_ref().store(store_id)
                {
                    run_reactive_subscription = store.node_reactive_subscription(node_id).cloned();
                }

                input_pieces.push(FormatterInputPiece::Text(FormatterTextPiece {
                    text: *text,
                    location: node.location.clone(),
                }));
            }

            _ => {
                let anchor_id = FormatterAnchorId(anchor_side_table.len());
                anchor_side_table.push(node_id);

                input_pieces.push(FormatterInputPiece::Opaque(FormatterOpaquePiece {
                    id: anchor_id,
                    kind: opaque_kind_for_kind(view, &node.kind)
                        .map_err(|error| CompilerMessages::from_error_ref(error, string_table))?,
                }));
            }
        }
    }

    let input = FormatterInput {
        pieces: input_pieces,
    };

    // 1. Pre-format whitespace passes.
    let mut output =
        apply_whitespace_passes_to_input(input, pre_format_passes, run_position, string_table);

    // 2. Style formatter.
    let mut formatter_warnings = Vec::new();

    if let Some(fmt) = formatter {
        let next_input = output_to_input(output, representative_location, string_table);
        let formatter_result = fmt.formatter.format(next_input, string_table)?;

        formatter_warnings.extend(formatter_result.warnings);
        output = formatter_result.output;
    }

    // 3. Post-format whitespace passes.
    if !post_format_passes.is_empty() {
        let post_input = output_to_input(output, representative_location, string_table);

        output = apply_whitespace_passes_to_input(
            post_input,
            post_format_passes,
            run_position,
            string_table,
        );
    }

    // 4. Map formatter output back to TIR nodes.
    let (replacement_nodes, content_changed) = output_to_tir_nodes(
        view,
        store_id,
        output,
        representative_location,
        &anchor_side_table,
        run_reactive_subscription,
        string_table,
    )?;

    // A run is considered changed if its output node IDs differ from the input.
    let run_changed = content_changed
        || replacement_nodes.len() != run.len()
        || !replacement_nodes
            .iter()
            .zip(run.iter())
            .all(|(a, b)| a == b);

    Ok((replacement_nodes, formatter_warnings, run_changed))
}

/// Maps formatter output pieces back to TIR node IDs.
///
/// WHAT: text output becomes a new body `Text` node; opaque anchors are looked
/// up in the local side-table and the original TIR node is reused.
/// WHY: preserving original nodes for anchors keeps child-template opacity and
/// dynamic-expression metadata intact.
fn output_to_tir_nodes(
    view: &TirView<'_>,
    store_id: TemplateStoreId,
    output: crate::compiler_frontend::ast::templates::formatter_contract::FormatterOutput,
    representative_location: &SourceLocation,
    anchor_side_table: &[TemplateIrNodeId],
    run_reactive_subscription: Option<ReactiveSubscription>,
    string_table: &mut StringTable,
) -> Result<(Vec<TemplateIrNodeId>, bool), CompilerMessages> {
    let mut nodes = Vec::with_capacity(output.pieces.len());
    let mut content_changed = false;

    for piece in output.pieces {
        match piece {
            FormatterOutputPiece::Text(text) => {
                let text_id = string_table.intern(&text);
                let byte_len = text.len();

                nodes.push(push_formatter_node(
                    view,
                    store_id,
                    TemplateIrNode::new(
                        TemplateIrNodeKind::Text {
                            text: text_id,
                            byte_len: byte_len as u32,
                            origin: TemplateSegmentOrigin::Body,
                        },
                        representative_location.clone(),
                    ),
                    run_reactive_subscription.clone(),
                    string_table,
                )?);

                content_changed = true;
            }

            FormatterOutputPiece::Opaque(anchor) => {
                let Some(node_id) = anchor_side_table.get(anchor.id.0).copied() else {
                    return Err(CompilerMessages::from_error_ref(
                        CompilerError::compiler_error(format!(
                            "TIR formatter view received invalid opaque anchor id {}; only {} anchors exist for this formatter run.",
                            anchor.id.0,
                            anchor_side_table.len()
                        )),
                        string_table,
                    ));
                };

                nodes.push(node_id);
            }
        }
    }

    Ok((nodes, content_changed))
}

/// Appends a formatter-produced node to the store that owns the current view.
///
/// WHAT: obtains the mutable store borrow only after formatter input has been
///       extracted into owned local data. WHY: `TirView` reads through the
///       registry/store `RefCell` model, so writeback must be a separate short
///       phase rather than holding a mutable store borrow during view reads.
fn push_formatter_node(
    view: &TirView<'_>,
    store_id: TemplateStoreId,
    node: TemplateIrNode,
    reactive_subscription: Option<ReactiveSubscription>,
    string_table: &StringTable,
) -> Result<TemplateIrNodeId, CompilerMessages> {
    let mut store = view
        .registry_ref()
        .store_mut(store_id)
        .map_err(|error| compiler_error_messages(error, string_table))?;

    let node_id = store.push_node(node);
    if let Some(subscription) = reactive_subscription {
        store.set_node_reactive_subscription(node_id, subscription);
    }

    Ok(node_id)
}

// -------------------------
//  Source locations
// -------------------------

/// Chooses a `TemplateBodyRunPosition` for a run based on whether it is the
/// first/last run in the parent sequence.
fn run_position_for_run(is_first_run: bool, is_last_run: bool) -> TemplateBodyRunPosition {
    match (is_first_run, is_last_run) {
        (true, true) => TemplateBodyRunPosition::Only,
        (true, false) => TemplateBodyRunPosition::First,
        (false, true) => TemplateBodyRunPosition::Last,
        (false, false) => TemplateBodyRunPosition::Middle,
    }
}

/// Derives a coarse representative source location for a run of TIR nodes.
///
/// WHAT: aggregates body-text node locations when possible; falls back to the
/// location of the first text/child/dynamic node in the run. Both phases share
/// a single pass to avoid reading each node twice.
/// WHY: formatter output can rewrite arbitrary text, so exact per-character
/// provenance is not feasible. A representative span preserves useful
/// diagnostics locations without pretending to be precise.
fn representative_location_for_run(
    view: &TirView<'_>,
    store_id: TemplateStoreId,
    run: &[TemplateIrNodeId],
    string_table: &StringTable,
) -> Result<SourceLocation, CompilerMessages> {
    let mut first_text_location: Option<SourceLocation> = None;
    let mut last_text_location: Option<SourceLocation> = None;
    let mut fallback_location: Option<SourceLocation> = None;

    for &node_id in run {
        let node_ref = TemplateNodeRef::new(store_id, node_id);
        let node = view
            .effective_node(node_ref)
            .map_err(|error| compiler_error_messages(error, string_table))?;

        match &node.kind {
            TemplateIrNodeKind::Text { origin, .. } => {
                if *origin == TemplateSegmentOrigin::Body {
                    if first_text_location.is_none() {
                        first_text_location = Some(node.location.clone());
                    }
                    last_text_location = Some(node.location.clone());
                }

                if fallback_location.is_none() {
                    fallback_location = Some(node.location.clone());
                }
            }

            TemplateIrNodeKind::ChildTemplate { .. }
            | TemplateIrNodeKind::DynamicExpression { .. }
                if fallback_location.is_none() =>
            {
                fallback_location = Some(node.location.clone());
            }

            _ => {}
        }
    }

    // Prefer the aggregated body-text span when body-text nodes exist.
    if let (Some(start), Some(end)) = (first_text_location, last_text_location) {
        if start.scope != end.scope {
            return Ok(start);
        }

        return Ok(SourceLocation {
            scope: start.scope,
            start_pos: start.start_pos,
            end_pos: end.end_pos,
        });
    }

    // Fall back to the first text/child/dynamic node location.
    Ok(fallback_location.unwrap_or_default())
}

/// Derives a representative location for a single body-eligible node.
fn representative_location_for_single_node(
    view: &TirView<'_>,
    node_ref: TemplateNodeRef,
    string_table: &StringTable,
) -> Result<SourceLocation, CompilerMessages> {
    representative_location_for_run(
        view,
        node_ref.store_id,
        std::slice::from_ref(&node_ref.node_id),
        string_table,
    )
}

// -------------------------
//  Diagnostics
// -------------------------

fn compiler_error_messages(error: CompilerError, string_table: &StringTable) -> CompilerMessages {
    CompilerMessages::from_error_ref(error, string_table)
}
