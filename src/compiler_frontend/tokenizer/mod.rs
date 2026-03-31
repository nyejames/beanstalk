//! Tokenizer stage modules.
//!
//! WHAT: lexes source text into tokens and applies newline-handling policy for template/string
//! bodies before parsing.

pub(crate) mod lexer;
pub(crate) mod newline_handling;
pub(crate) mod tokens;
