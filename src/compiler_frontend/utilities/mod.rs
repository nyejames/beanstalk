//! Shared frontend utility helpers.
//!
//! WHAT: small cross-cutting helpers for path formatting, character classification,
//!       token scanning, and delimiter-depth bookkeeping.
//! WHY: these utilities predate some newer subsystem boundaries and are reused
//! across formatting, parsing, and declaration code.

pub(crate) mod basic;
pub(crate) mod token_scan;

#[cfg(test)]
mod tests;
