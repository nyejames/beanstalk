//! Tokenizer stage modules.
//!
//! WHAT: lexes source text into tokens and applies newline-handling policy for template/string
//! bodies before parsing.

pub(crate) mod lexer;
pub(crate) mod line_scanning;
pub(crate) mod newline_handling;
mod numeric;
mod text_modes;
pub(crate) mod tokens;
