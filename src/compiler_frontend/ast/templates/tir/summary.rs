//! TIR summary metadata.
//!
//! WHAT: `TemplateIrSummary` records cheap shape facts about a template —
//! estimated output size, node counts, depth, and feature flags — so capacity
//! planning, folding, and HIR preparation can make decisions without traversing
//! the full node tree.
//!
//! WHY: the current codebase already benefits from render-piece byte estimation
//! (Phase A3) and atom count capacity hints (Phase A2). TIR summaries extend
//! that pattern to the full tree shape so future phases can pre-size buffers,
//! skip unnecessary traversals, and flag templates that need special handling.
//!
//! ## Ownership contract
//!
//! Summaries are computed once during conversion (Phase B1) and stored with
//! each `TemplateIr`. They do not own AST, HIR, or backend data. Summary
//! fields are cheap to copy and cheap to compute.

// -------------------------
//  Template IR Summary
// -------------------------

/// Shape metadata for a single TIR template.
///
/// WHAT: collects counts, byte estimates, depth, and boolean feature flags.
/// WHY: avoiding a second traversal for common shape queries keeps the
/// folding and formatting paths fast. Counters also feed profiling so
/// the compiler can identify templates that deserve special handling.
///
/// ## Invariants
///
/// - All counts are zero for an empty template.
/// - `max_depth` is 0 for a flat (single-node) template.
/// - `estimated_output_bytes` is a conservative lower bound; actual output
///   may be larger when runtime expressions are involved.
#[derive(Clone, Debug)]
pub(crate) struct TemplateIrSummary {
    /// Conservative estimate of the final folded output size in bytes.
    ///
    /// Used to seed `String::with_capacity` in the TIR fold path (Phase B2).
    /// This is a lower bound — runtime expressions contribute zero to the estimate.
    pub(crate) estimated_output_bytes: usize,

    /// Number of `Text` nodes in the template tree.
    pub(crate) text_node_count: u32,

    /// Total interned text bytes across all `Text` nodes.
    pub(crate) text_byte_count: usize,

    /// Number of `DynamicExpression` nodes.
    pub(crate) dynamic_expression_count: u32,

    /// Number of `ChildTemplate` references.
    pub(crate) child_template_count: u32,

    /// Number of `Slot` placeholders.
    pub(crate) slot_count: u32,

    /// Number of wrapper set entries (from `$children(..)` directives).
    pub(crate) wrapper_count: u32,

    /// Maximum nesting depth of the node tree (root is depth 0).
    pub(crate) max_depth: u16,

    /// True if the template contains at least one `Slot` placeholder.
    pub(crate) has_slots: bool,

    /// True if the template has an associated style formatter.
    pub(crate) has_formatter: bool,

    /// True if the template contains `BranchChain`, `Loop`, or `LoopControl` nodes.
    pub(crate) has_control_flow: bool,

    /// True if the template contains at least one `DynamicExpression` with a
    /// reactive `$(source)` subscription.
    pub(crate) has_reactivity: bool,

    /// True if the template shape can be fully evaluated at compile time.
    ///
    /// This mirrors `Template::is_const_evaluable_value()` but is computed
    /// from TIR node structure so the fold path can check it without
    /// reaching back into the AST representation.
    pub(crate) is_const_evaluable_shape: bool,
}

impl TemplateIrSummary {
    /// Creates a summary with all counts zeroed and all flags false.
    pub(crate) fn empty() -> Self {
        Self {
            estimated_output_bytes: 0,
            text_node_count: 0,
            text_byte_count: 0,
            dynamic_expression_count: 0,
            child_template_count: 0,
            slot_count: 0,
            wrapper_count: 0,
            max_depth: 0,
            has_slots: false,
            has_formatter: false,
            has_control_flow: false,
            has_reactivity: false,
            is_const_evaluable_shape: true,
        }
    }
}

impl Default for TemplateIrSummary {
    fn default() -> Self {
        Self::empty()
    }
}
