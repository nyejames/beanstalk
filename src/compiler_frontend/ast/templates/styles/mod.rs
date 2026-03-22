//! Built-in template style directive modules.
//!
//! WHAT:
//! - Groups compiler-owned style directive implementations (`$markdown`, `$code`, `$css`, `$html`, `$raw`, `$escape_html`).
//!
//! WHY:
//! - Keeps style logic modular and separate from generic template parsing.

pub(crate) mod code;
pub(crate) mod css;
pub(crate) mod escape_html;
pub(crate) mod html;
pub(crate) mod markdown;
pub(crate) mod raw;
pub(crate) mod whitespace;

/// Internal boundary marker used by `invoke_legacy_formatter` and string-based
/// formatters (e.g. `$markdown`) to protect opaque anchor positions inside a
/// concatenated text buffer. Not part of the structured formatter interface.
pub(crate) const TEMPLATE_FORMAT_GUARD_CHAR: char = '\u{FFFC}';
