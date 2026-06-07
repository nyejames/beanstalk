//! Shared child-contribution classification for slot composition and runtime planning.
//!
//! WHAT:
//! - `ContributionShape` classifies a single template atom as a potential child
//!   template contribution, capturing whether it represents child output and
//!   whether it opts out of parent `$children(..)` wrappers.
//! - `classify_contribution_atom` is the single entry point for both compile-time
//!   slot expansion and runtime slot-site planning.
//!
//! WHY:
//! - Both `composition.rs` and `runtime_plan/sites.rs` need the same facts about
//!   each atom. Sharing one classifier prevents duplicated predicate logic from
//!   drifting out of sync when the set of child-contribution indicators changes.

use crate::compiler_frontend::ast::expressions::expression::ExpressionKind;
use crate::compiler_frontend::ast::templates::template::{TemplateAtom, TemplateSegment};
use crate::compiler_frontend::ast::templates::template_types::Template;

/// Classification of an atom's relationship to child-template wrapping.
///
/// WHAT: Answers two questions for every atom:
/// 1. Is this atom a child-template contribution that should receive
///    `$children(..)` wrappers?
/// 2. Does the atom's source template opt out of parent-applied wrappers via
///    `$fresh`?
///
/// WHY: Both compile-time expansion and runtime site planning need identical
/// answers so wrapper application is consistent regardless of when slot
/// resolution happens.
pub(super) struct ContributionShape {
    /// True when the atom represents output from a child template, either as a
    /// folded content segment or as a direct template expression. This drives
    /// whether `$children(..)` wrappers are applied.
    pub(super) is_child_template_contribution: bool,

    /// True when the atom's source template carries `$fresh`, meaning the
    /// parent should skip applying its own `$children(..)` wrappers to this
    /// atom.
    pub(super) skips_parent_child_wrappers: bool,
}

/// Classifies a single template atom for child-contribution purposes.
///
/// WHAT: Inspects content segments for folded child-template output, direct
/// template expressions, and `source_child_template` references, then derives
/// whether the atom participates in child-wrapper application and whether it
/// opts out of parent wrappers.
///
/// WHY: One shared classification keeps compile-time and runtime slot paths
/// consistent. Any future change to what counts as a child contribution only
/// needs to happen here.
pub(super) fn classify_contribution_atom(atom: &TemplateAtom) -> ContributionShape {
    let TemplateAtom::Content(segment) = atom else {
        return ContributionShape {
            is_child_template_contribution: false,
            skips_parent_child_wrappers: false,
        };
    };

    let template = source_template_from_segment(segment);
    let is_child_template_contribution = segment.is_child_template_output
        || segment.source_child_template.is_some()
        || matches!(segment.expression.kind, ExpressionKind::Template(_));

    let skips_parent_child_wrappers =
        template.is_some_and(|template| template.style.skip_parent_child_wrappers);

    ContributionShape {
        is_child_template_contribution,
        skips_parent_child_wrappers,
    }
}

fn source_template_from_segment(segment: &TemplateSegment) -> Option<&Template> {
    if let Some(source_child_template) = &segment.source_child_template {
        return Some(source_child_template.as_ref());
    }

    match &segment.expression.kind {
        ExpressionKind::Template(template) => Some(template.as_ref()),
        _ => None,
    }
}
