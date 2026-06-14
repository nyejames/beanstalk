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
//!   │ template_body_sentinels.rs Body-only structural markers such as `[else]`.
//!   ▼ create_template_node.rs   Assemble a typed `Template` from head + body.
//!   ▼ template_types.rs         Core AST template data types.
//!   ▼ template_control_flow/    Structured template `if` / `loop` metadata,
//!                               validation, const-eval checks, and remapping.
//!   │
//!   ▼ template_slots/           Slot schema, contribution bucketing, and composition.
//!   ▼ template_composition.rs   Wrapper / head-chain composition rules.
//!   │
//!   ▼ template_render_units.rs  Shared content → render-plan preparation for
//!   │                           linear templates and control-flow branches/bodies.
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
//! | `template_body_sentinels.rs` | Body sentinel classification, policy, and diagnostics |
//! | `create_template_node.rs` | Head + body → `Template`; owns final runtime metadata |
//! | `template_types.rs` | `Template`, `TemplateNode`, slot and directive types |
//! | `template_control_flow/` | Template control-flow AST, validation, const-eval checks, remapping |
//! | `template_slots/` | Slot schema, contributions, and composition |
//! | `template_composition.rs` | Wrapper chaining and head composition |
//! | `template_render_units.rs` | Shared composition / formatting / render-plan preparation |
//! | `styles/` | Style-directive output transformers |
//! | `template_render_plan.rs` | Which style applies to which template |
//! | `template_folding.rs` | Constant-time folding of pure templates |
//! | `top_level_templates.rs` | Entry-file fragment synthesis and capture planning |
//! | `template_formatting.rs` | Shared formatting helpers |

// -------------------------
//  Public Modules
// -------------------------

pub(crate) mod create_template_node;
pub(crate) mod styles;
pub(crate) mod template;
pub(crate) mod template_body_parser;
mod template_body_sentinels;
pub(crate) mod template_composition;
pub(crate) mod template_control_flow;
pub(crate) mod template_folding;
pub(crate) mod template_formatting;
pub(crate) mod template_head_parser;
pub(crate) mod template_render_plan;
pub(crate) mod template_render_units;
pub(crate) mod template_renderability;
pub(crate) mod template_slots;
pub(crate) mod template_types;
pub(crate) mod top_level_templates;

// -------------------------
//  Reactive metadata traversal
// -------------------------
//
// Owned by the template subsystem because it only depends on template shape.
// AST finalization supplies its own expression resolver for flow-aware lookup.

pub(crate) mod reactive_template_metadata;

// -------------------------
//  Private Modules
// -------------------------

mod doc_fragments;
pub(crate) mod error;
