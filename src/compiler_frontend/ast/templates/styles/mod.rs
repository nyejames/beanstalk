//! Built-in template style directive modules.
//!
//! WHAT:
//! - Groups compiler-owned style directive implementations (`$markdown`, `$code`, `$css`).
//! - Exposes shared formatting guard constants used across style formatters.
//!
//! WHY:
//! - Keeps style logic modular and separate from generic template parsing.
//! - Avoids duplicated hidden-marker constants that can drift between formatters.

pub(crate) mod code;
pub(crate) mod css;
pub(crate) mod markdown;

/// Shared invisible boundary marker used to protect already-formatted segments
/// from parent template formatters.
pub(crate) const TEMPLATE_FORMAT_GUARD_CHAR: char = '\u{FFFC}';
