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

mod composition;
mod contributions;
mod diagnostics;
mod schema;

pub(crate) use composition::{compose_template_with_slots, ensure_no_slot_insertions_remain};

#[cfg(test)]
#[path = "../tests/slots_tests.rs"]
mod slots_tests;
