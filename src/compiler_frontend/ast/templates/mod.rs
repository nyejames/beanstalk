//! Template parsing, composition, formatting, folding, and TIR.
//!
//! Templates are Beanstalk's first-class string-producing construct. This module
//! contains the full AST-stage pipeline for templates. The Template IR (`tir/`)
//! is the authoritative representation: templates are emitted into a
//! module-scoped `TemplateIrStore`, composed, formatted, folded, and handed off
//! to HIR as either a folded constant or an owned runtime-template payload.
//!
//! ## Template compilation flow (AST stage)
//!
//! ```text
//! Tokens
//!   │
//!   ▼ template_head_parser/       Parse the head: directives, style config, slot
//!   │                             declarations, head expressions, and control-flow
//!   │                             suffixes.
//!   ▼ template_body_parser.rs     Parse the body: raw text, nested templates,
//!   │                             runtime expression splices.
//!   │ template_body_sentinels.rs  Body-only structural markers (`[else]`,
//!   │                             `[break]`, `[continue]`).
//!   ▼ create_template_node.rs     Orchestrate head + body → `Template`; start and
//!   │                             finish parser-TIR builder state.
//!   │
//!   ▼ TIR path ───────────────────────────────────────────────────────────┐
//!   │  │
//!   │  ▼ tir/parser_builder_state.rs + tir/builder.rs                     │
//!   │     Emit literal parser output (text, dynamic expressions, children, │
//!   │     slots, control flow) into the module-scoped `TemplateIrStore`.    │
//!   │  │
//!   │  ▼ template_render_units.rs                                         │
//!   │     After composition and formatting, install finalized linear and   │
//!   │     control-flow bodies as finalized TIR roots.                     │
//!   │  │
//!   │  ▼ Primary TIR consumers                                            │
//!   │     • tir/fold.rs                     — compile-time TIR folding        │
//!   │     • tir/formatter_view.rs           — style formatters applied to TIR │
//!   │     • tir/slot_plan.rs                — TIR-side runtime slot-routing   │
//!   │     • tir/handoff_materialization.rs  — build owned runtime handoffs    │
//!   │
//!   ▼ Neutral AST/HIR boundary                                            │
//!      runtime_handoff.rs          Owned runtime-template and slot handoff
//!                                 payloads consumed by HIR lowering.
//! ```
//!
//! ## TIR — Template IR
//!
//! The `tir/` submodule provides the AST-local Template IR. It is the
//! authoritative internal representation for parsed templates and the single
//! source of truth for template semantics during AST processing.
//!
//! A `Template` value holds a `tir_reference` into the module-scoped
//! `TemplateIrStore`. TIR is local to AST construction/finalization for one
//! module and is dropped before the `Ast` is returned. HIR and backends never
//! see TIR IDs, stores, views, overlays, or registry values; they receive only
//! folded string constants or neutral owned runtime-handoff payloads from
//! `runtime_handoff.rs`.
//!
//! Submodule roles (see `tir/mod.rs` for the full layout and ownership contract):
//!
//! | File | Role |
//! |---|---|
//! | `tir/mod.rs` | Module entry, narrow re-exports, ownership contract |
//! | `tir/ids.rs` | Typed store IDs (`TemplateIrId`, `TemplateIrNodeId`, …) |
//! | `tir/refs.rs` | Store-qualified final TIR references |
//! | `tir/registry.rs` | Module-local registry for stores, refs, and overlays |
//! | `tir/overlays.rs` | Final overlay set and overlay dimension handles |
//! | `tir/store.rs` | `TemplateIrStore` — central contiguous storage |
//! | `tir/node.rs` | `TemplateIr`, `TemplateIrNode`, `TemplateIrNodeKind` |
//! | `tir/summary.rs` | `TemplateIrSummary` — cheap shape metadata |
//! | `tir/validation.rs` | Structural integrity checks for the store |
//! | `tir/builder.rs` | Parser-facing mutable facade for pushing TIR nodes |
//! | `tir/parser_builder_state.rs` | Parser-emitted TIR builder state |
//! | `tir/expression_payload_walker.rs` | Shared read-only expression-payload traversal |
//! | `tir/construction.rs` | TIR construction helpers (atom-to-node, summary) |
//! | `tir/subtree_copy.rs` | TIR-native active-context subtree copying |
//! | `tir/finalize_sync.rs` | Install finalized TIR roots after render-unit preparation |
//! | `tir/classification.rs` | Store-aware TIR shape queries for classification |
//! | `tir/fold.rs` | TIR-native compile-time folding |
//! | `tir/formatter_view.rs` | TIR-native formatter feed |
//! | `tir/render_unit.rs` | Render-unit and aggregate-wrapper preparation |
//! | `tir/handoff_materialization.rs` | Build owned runtime-template trees for HIR lowering |
//! | `tir/slot_plan.rs` | Runtime slot route handoff side tables |
//! | `tir/slot_composition/` | TIR-native slot schema and contribution routing |
//! | `tir/wrapper_sets.rs` | Wrapper set equivalence and reuse |
//! | `tir/tests/` | TIR-focused tests |
//!
//! ## Module responsibilities at a glance
//!
//! | File / submodule | Owns |
//! |---|---|
//! | `template_head_parser/` | Template head parsing: directives, style config, head expressions, control-flow suffixes |
//! | `template_body_parser.rs` | Template body parsing: text, nested templates, expression splices |
//! | `template_body_sentinels.rs` | Body structural markers (`[else]`, `[break]`, `[continue]`) and diagnostics |
//! | `create_template_node.rs` | Template construction orchestrator; starts/finishes parser-TIR builder state |
//! | `template.rs` | Core data types: `Template`, `TemplateType`, `Style`, `SlotKey`, `SlotPlaceholder`, formatters |
//! | `template_types.rs` | Central `Template` struct and const/renderability queries |
//! | `template_control_flow/` | Structured `if` / `loop` metadata, validation, const-eval helpers |
//! | `template_slots/` | Slot schema, contribution bucketing, runtime plan construction |
//! | `tir/slot_composition/` | TIR-native head-chain composition and `$children(..)` wrapper application |
//! | `template_render_units.rs` | Shared render-unit preparation; installs finalized bodies as TIR roots |
//! | `styles/` | Directive-owned formatters (markdown, raw, whitespace) |
//! | `formatter_contract.rs` | Formatter anchor shapes and pipeline adapters |
//! | `template_folding.rs` | Compile-time folding entry point; TIR-native for finalized templates |
//! | `top_level_templates.rs` | Entry-file const fragment collection and doc fragment extraction |
//! | `tir/mod.rs` | TIR module entry, re-exports, ownership contract |
//! | `tir/ids.rs` | Typed store IDs |
//! | `tir/refs.rs` | Store-qualified final TIR references |
//! | `tir/registry.rs` | Module-local registry for stores, refs, and overlays |
//! | `tir/overlays.rs` | Final overlay set and overlay dimension handles |
//! | `tir/store.rs` | `TemplateIrStore` — central contiguous storage |
//! | `tir/node.rs` | `TemplateIr`, `TemplateIrNode`, `TemplateIrNodeKind` |
//! | `tir/summary.rs` | `TemplateIrSummary` — cheap shape metadata |
//! | `tir/validation.rs` | Structural integrity checks for the TIR store |
//! | `tir/builder.rs` | Parser-facing mutable facade for pushing TIR nodes |
//! | `tir/parser_builder_state.rs` | Parser-emitted TIR builder state |
//! | `tir/expression_payload_walker.rs` | Shared read-only expression-payload traversal |
//! | `tir/construction.rs` | TIR construction helpers (atom-to-node, summary) |
//! | `tir/subtree_copy.rs` | TIR-native active-context subtree copying |
//! | `tir/finalize_sync.rs` | Compatibility-content materialization and control-flow root installation |
//! | `tir/classification.rs` | Store-aware TIR shape queries for classification |
//! | `tir/fold.rs` | TIR-native compile-time folding |
//! | `tir/formatter_view.rs` | TIR-native style-formatter view |
//! | `tir/render_unit.rs` | Render-unit and aggregate-wrapper preparation |
//! | `tir/handoff_materialization.rs` | Build owned runtime-template trees for HIR lowering |
//! | `tir/slot_plan.rs` | Runtime slot route handoff side tables |
//! | `tir/slot_composition/` | TIR-native slot schema and contribution routing |
//! | `tir/wrapper_sets.rs` | Wrapper set equivalence and reuse |
//! | `runtime_handoff.rs` | Neutral AST/HIR owner of owned runtime-template handoff shapes |
//! | `runtime_template_expression.rs` | Unwrap coerced/runtime wrappers to reach a `Template` expression |
//! | `reactive_template_metadata.rs` | Structural traversal merging reactive `$(source)` metadata |
//! | `template_renderability.rs` | Template-head value renderability classification by `TypeId` |
//! | `doc_fragments.rs` | Extract `$doc` comment templates into `AstDocFragment` metadata |
//! | `error.rs` | Local `TemplateError` boundary between diagnostics and infrastructure errors |

// -------------------------
//  Public Modules
// -------------------------

pub(crate) mod create_template_node;
pub(crate) mod formatter_contract;
pub(crate) mod runtime_handoff;
pub(crate) mod styles;
pub(crate) mod template;
pub(crate) mod template_body_parser;
mod template_body_sentinels;
pub(crate) mod template_control_flow;
pub(crate) mod template_folding;
pub(crate) mod template_head_parser;
pub(crate) mod template_render_units;
pub(crate) mod template_renderability;
pub(crate) mod template_slots;
pub(crate) mod template_types;
pub(crate) mod top_level_templates;

// -------------------------
//  Template IR (TIR)
// -------------------------
//
// TIR is the authoritative internal representation for templates during AST
// processing. The `tir/` submodule owns the store, registry, views, overlays,
// folding, formatting, and HIR handoff construction.

pub(crate) mod tir;

// -------------------------
//  AST/HIR Runtime Handoff
// -------------------------

pub(crate) use runtime_handoff::{
    OwnedRuntimeSlotApplicationHandoff, OwnedRuntimeSlotContributionSource, OwnedRuntimeSlotSite,
    OwnedRuntimeSlotSiteRenderPiece, OwnedRuntimeTemplateBody, OwnedRuntimeTemplateBranch,
    OwnedRuntimeTemplateHandoff, OwnedRuntimeTemplateNode,
};

// -------------------------
//  Runtime template expression helpers
// -------------------------

mod runtime_template_expression;
pub(crate) use runtime_template_expression::runtime_template_expression;

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

#[cfg(test)]
#[path = "tests/control_flow_body_ref_helpers.rs"]
pub(crate) mod control_flow_body_ref_test_helpers;

#[cfg(test)]
mod template_folding_tests;
