//! Template style formatter/validator modules.
//!
//! WHAT:
//! - Groups reusable formatter implementations used by core and build-system-provided directives.
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
