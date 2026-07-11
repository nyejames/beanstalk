//! TIR-native child-contribution classification.
//!
//! WHAT: `ContributionShape` classifies a single TIR contribution node as a
//!       potential child-template contribution, capturing whether it represents
//!       child output and whether it opts out of parent `$children(..)` wrappers.
//!
//! WHY: Both TIR slot composition and the atom-based runtime slot planner need
//!      the same facts about each contribution so wrapper application stays
//!      consistent regardless of which composition path produced the runtime
//!      slot plan. Keeping the shared type in TIR reflects that the TIR path is
//!      the production authority.

use crate::compiler_frontend::ast::templates::tir::{
    TemplateIrNodeId, TemplateIrNodeKind, TemplateIrStore,
};

/// Classification of a contribution's relationship to child-template wrapping.
///
/// WHAT: Answers two questions for every contribution:
/// 1. Is this contribution child-template output that should receive
///    `$children(..)` wrappers?
/// 2. Does the contribution's source template opt out of parent-applied
///    wrappers via `$fresh`?
///
/// WHY: Both compile-time expansion and runtime site planning need identical
/// answers so wrapper application is consistent regardless of when slot
/// resolution happens.
#[derive(Clone, Debug)]
pub(crate) struct ContributionShape {
    /// True when the contribution represents output from a child template.
    /// This drives whether `$children(..)` wrappers are applied.
    pub(crate) is_child_template_contribution: bool,

    /// True when the contribution's source template carries `$fresh`, meaning
    /// the parent should skip applying its own `$children(..)` wrappers.
    pub(crate) skips_parent_child_wrappers: bool,
}

/// Classifies a TIR contribution node for child-contribution purposes.
///
/// WHAT: Inspects routed TIR fill nodes and determines whether they represent
///       child-template output and whether their source template opts out of
///       parent `$children(..)` wrappers.
///
/// WHY: The TIR-native head-chain composition path needs the same
///      `ContributionShape` facts as the atom-based runtime slot planner so
///      slot-site wrapper application stays consistent regardless of which
///      composition path produced the runtime slot plan.
pub(crate) fn classify_tir_contribution_node(
    store: &TemplateIrStore,
    node_id: TemplateIrNodeId,
) -> ContributionShape {
    let Some(node) = store.get_node(node_id) else {
        return ContributionShape {
            is_child_template_contribution: false,
            skips_parent_child_wrappers: false,
        };
    };

    match &node.kind {
        TemplateIrNodeKind::ChildTemplate { reference, .. } => {
            let template = reference
                .template_id_in_store(store.store_id())
                .and_then(|template_id| store.get_template(template_id));

            ContributionShape {
                is_child_template_contribution: true,
                skips_parent_child_wrappers: template.is_some_and(|template_definition| {
                    template_definition.style.skip_parent_child_wrappers
                }),
            }
        }

        TemplateIrNodeKind::InsertContribution { template } => {
            let referenced_template = store.get_template(*template);

            ContributionShape {
                is_child_template_contribution: true,
                skips_parent_child_wrappers: referenced_template.is_some_and(
                    |template_definition| template_definition.style.skip_parent_child_wrappers,
                ),
            }
        }

        // Non-child contributions are not wrapped and do not opt out of wrapping.
        _ => ContributionShape {
            is_child_template_contribution: false,
            skips_parent_child_wrappers: false,
        },
    }
}
