//! Template head parsing split into focused modules.
//!
//! WHAT:
//! - Exposes the stable template-head parsing entrypoints.
//! - Routes directive parsing and head expression handling to focused submodules.
//!
//! WHY:
//! - Keeps each piece of head parsing responsible for one concern.
//! - Makes parser control-flow and directive ownership boundaries easier to maintain.

mod children_directive;
mod core_directives;
mod handler_directives;
mod head_expressions;
mod head_parser;

pub(crate) use core_directives::apply_doc_comment_defaults;
pub use head_parser::parse_template_head;
