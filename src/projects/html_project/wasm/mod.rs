//! HTML-builder Wasm integration modules.
//!
//! WHAT: contains builder-owned Wasm planning, request wiring, and JS bootstrap generation.
//! WHY: isolates Wasm-specific orchestration from JS-only HTML build behavior.

pub(crate) mod artifacts;
pub(crate) mod export_plan;
pub(crate) mod js_bootstrap;
pub(crate) mod request;
