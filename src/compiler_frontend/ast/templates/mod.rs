//! Template parsing, composition, formatting, and folding.
//!
//! Templates are Beanstalk's first-class string-producing construct. This module
//! contains the full AST-stage pipeline for templates.
//!
//! ## Template compilation flow (AST stage)
//!
//! ```text
//! Tokens
//!   │
//!   ▼ template_head_parser/     Parse the head: style directive, slot declarations,
//!   │                           children/handler directives, and head expressions.
//!   ▼ template_body_parser.rs   Parse the body: raw text, nested templates, runtime
//!   │                           expression splices.
//!   ▼ create_template_node.rs   Assemble a typed `Template` from head + body.
//!   ▼ template_types.rs         Core AST template data types.
//!   │
//!   ▼ template_slots.rs         Slot insertion and children-directive composition.
//!   ▼ template_composition.rs   Wrapper / head-chain composition rules.
//!   │
//!   ▼ styles/                   Per-directive output formatting (markdown, raw, whitespace, …).
//!   ▼ template_render_plan.rs   High-level rendering plan: determines which style to apply.
//!   │
//!   ▼ template_folding.rs       Compile-time constant folding: pure templates → string literals.
//!   │
//!   ▼ top_level_templates.rs    Fragment synthesis for the entry file:
//!                               - const templates become ConstString start fragments
//!                               - runtime templates become PushStartRuntimeFragment AST nodes
//!                               - mutation/reference analysis for runtime capture plans
//! ```
//!
//! ## Module responsibilities at a glance
//!
//! | File / submodule | Owns |
//! |---|---|
//! | `template_head_parser/` | Head token parsing, directive dispatch |
//! | `template_body_parser.rs` | Body token parsing (raw text, splices, nesting) |
//! | `create_template_node.rs` | Head + body → `Template`; owns final runtime metadata |
//! | `template_types.rs` | `Template`, `TemplateNode`, slot and directive types |
//! | `template_slots.rs` | Slot resolution and children directive semantics |
//! | `template_composition.rs` | Wrapper chaining and head composition |
//! | `styles/` | Style-directive output transformers |
//! | `template_render_plan.rs` | Which style applies to which template |
//! | `template_folding.rs` | Constant-time folding of pure templates |
//! | `top_level_templates.rs` | Entry-file fragment synthesis and capture planning |
//! | `template_formatting.rs` | Shared formatting helpers |

pub(crate) mod create_template_node;
mod doc_fragments;
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
