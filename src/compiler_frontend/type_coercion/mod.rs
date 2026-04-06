//! Coercion policy for the Beanstalk compiler frontend.
//!
//! WHAT: owns all contextual coercion and type-compatibility decisions.
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
pub(crate) mod numeric;
pub(crate) mod string;

/// The context in which a coercion is being applied.
///
/// WHAT: distinguishes the site where a contextual coercion is permitted.
/// WHY: coercion rules differ by context — declarations allow Int→Float,
/// but function arguments and match patterns do not.
#[allow(dead_code)] // Planned: used once coercion helpers carry context through diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CoercionContext {
    /// A `result Type = expr` explicit-type declaration.
    ExplicitDeclaration,
    /// A `return expr` statement inside a typed function.
    ReturnSlot,
}

/// The context in which a compatibility check is being performed.
///
/// WHAT: distinguishes what level of leniency is applied when deciding
/// whether a value of one type is acceptable in a position expecting another.
/// WHY: function arguments and match patterns require exact type matches,
/// while declaration and return contexts allow narrowly-defined promotions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CompatibilityContext {
    /// Exact structural match required. Used for function arguments and patterns.
    Exact,
    /// Declaration initializer or return value slot. Allows Int → Float promotion.
    ///
    /// WHY a single variant for both: the permitted set of promotions is identical
    /// today. If the two contexts diverge in a future release, split this variant.
    Declaration,
    /// Return value slot. Allows Int → Float promotion.
    ///
    /// NOTE: currently carries the same rules as `Declaration`. Kept as a
    /// distinct variant so call-sites document their intent explicitly.
    #[allow(dead_code)] // Planned: used as callers document their return-slot context.
    ReturnSlot,
}
