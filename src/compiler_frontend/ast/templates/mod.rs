//! Template parsing, composition, formatting, and folding.
//!
//! Templates are Beanstalk's first-class string-producing construct. This module
//! contains the full AST-stage pipeline: token parsing, structural composition
//! (slots, wrappers, head-chain), style-directed formatting, compile-time
//! folding, and top-level fragment synthesis.

pub(crate) mod create_template_node;
pub(crate) mod styles;
pub(crate) mod template;
pub(crate) mod template_body_parser;
pub(crate) mod template_composition;
pub(crate) mod template_folding;
pub(crate) mod template_formatting;
pub(crate) mod template_head_parser;
pub(crate) mod template_render_plan;
pub(crate) mod template_slots;
pub(crate) mod template_types;
pub(crate) mod top_level_templates;
