//! Slot schema, contribution bucketing, and composition.
//!
//! WHAT: Fills wrapper template `$slot` placeholders with authored content,
//! handling `$insert(...)` routing, loose-atom grouping, and child-wrapper
//! application.
//!
//! WHY: Template slots are Beanstalk's mechanism for reusable structural
//! wrappers (tables, lists, conditional blocks). Keeping schema discovery,
//! contribution partitioning, and expansion in focused submodules makes the
//! slot pipeline easier to test and modify without affecting other template
//! stages.
//!
//! ## Data flow
//!
//! ```text
//! wrapper template + fill content
//!        │
//!        ▼ schema.rs          discover declared $slot targets
//!        │
//!        ▼ contributions.rs   partition fill atoms into explicit/loose buckets
//!        │
//!        ▼ composition.rs     replace SlotPlaceholder atoms with expanded content
//! ```

// -------------------------
//  Submodules
// -------------------------

mod composition;
mod contributions;
mod diagnostics;
mod error;
mod runtime_plan;
mod schema;

// -------------------------
//  Re-exports
// -------------------------

pub(in crate::compiler_frontend::ast::templates) use composition::ensure_no_slot_insertions_remain;
pub(in crate::compiler_frontend::ast::templates) use error::TemplateSlotError;
pub(crate) use runtime_plan::{RuntimeSlotApplicationPlan, RuntimeSlotContributionContent};
#[cfg(test)]
pub(crate) use runtime_plan::{RuntimeSlotContribution, RuntimeSlotContributionPlan};
pub(in crate::compiler_frontend::ast::templates) use runtime_plan::{
    SlotResolutionMode, SlotResolutionOutcome, resolve_slot_application,
};
#[cfg(test)]
pub(crate) use schema::SlotSchema;

#[cfg(test)]
#[path = "../tests/slots_tests.rs"]
mod slots_tests;
