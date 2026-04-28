//! Match-pattern parsing and validation.
//!
//! WHAT: parses literal, relational, and choice-variant case patterns.
//! WHY: pattern syntax and type validation evolve separately from match arm/body parsing.

mod choice;
mod diagnostics;
mod literal;
mod relational;
mod types;

// Public types consumed by AST statement parsing, HIR lowering, and tests.
pub use types::{ChoicePayloadCapture, MatchArm, MatchPattern, RelationalPatternOp};

// pub(super) surface re-exported to the `statements` parent module.
pub(super) use types::{ParsedChoicePattern, normalized_subject_type};

pub(super) use choice::parse_choice_variant_pattern;
pub(super) use literal::parse_non_choice_pattern;
