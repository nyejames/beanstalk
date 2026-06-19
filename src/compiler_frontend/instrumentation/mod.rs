//! Frontend performance instrumentation.
//!
//! WHAT: exposes counters for clone-heavy, cache-sensitive, and remap-heavy frontend paths.
//! WHY: detailed benchmark runs need enough local evidence to interpret small
//! end-to-end timing changes, while normal compiler builds must not pay for or
//! print this diagnostic data.

pub(crate) mod ast_counters;
pub(crate) mod frontend_counters;

pub(crate) use ast_counters::*;
pub(crate) use frontend_counters::*;

#[cfg(test)]
mod tests;
