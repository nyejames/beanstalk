//! Match-pattern parsing and validation.
//!
//! WHAT: parses literal, relational, option-presence, and choice-variant match patterns.
//! WHY: pattern syntax and type validation evolve separately from match arm/body parsing.

mod choice;
mod diagnostics;
mod literal;
mod option;
mod relational;
mod types;

// --------------------------
//  Re-exports
// --------------------------
//
// Public types consumed by AST statement parsing, HIR lowering, and tests.
pub use types::{ChoicePayloadCapture, MatchArm, MatchPattern, RelationalPatternOp};

// pub(super) surface re-exported to the `statements` parent module.
pub(super) use types::ParsedChoicePattern;

pub(super) use choice::parse_choice_variant_pattern;
pub(super) use literal::parse_non_choice_pattern;
pub(super) use option::parse_option_pattern;
