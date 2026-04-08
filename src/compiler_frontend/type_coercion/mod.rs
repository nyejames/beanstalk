//! Coercion policy for the Beanstalk compiler frontend.
//!
//! WHAT: Compatibility and coercion policy are centralized here, but assignment-like frontend sites still apply those rules explicitly after parsing.
//! WHY: coercion logic was previously scattered across datatypes, expression
//! evaluation, declarations, returns, and templates — each maintaining its own
//! mini-policy. This module is the single home for all of those decisions.
//!
//! ## Module boundary
//!
//! This module owns:
//! - type-compatibility checks (what values are accepted in what contexts)
//! - numeric contextual coercion (Int → Float in declaration and return slots)
//! - string coercion policy (what is renderable at template boundaries)
//!
//! This module does NOT own:
//! - operator result typing (`eval_expression.rs` still decides `Int + Float → Float`)
//! - builtin explicit casts (`Int(...)` / `Float(...)` syntax lives in `builtins/`)
//! - template formatting, composition, or slot mechanics

pub(crate) mod compatibility;
pub(crate) mod diagnostics;
pub(crate) mod numeric;
pub(crate) mod parse_context;
pub(crate) mod string;
