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
/// while return contexts allow narrowly-defined promotions.
///
/// Declaration-site coercion (e.g. `result Float = 1`) is no longer handled
/// through this context — it is applied by `coerce_expression_to_declared_type`
/// before `is_type_compatible` is ever called, so `eval_expression` always sees
/// an already-compatible type and uses `Exact`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CompatibilityContext {
    /// Exact structural match required. Used for function arguments, patterns,
    /// and expression evaluation. Declaration callers apply coercion before
    /// reaching this check, so they always pass `Exact`.
    Exact,
    /// Return value slot. Allows Int → Float promotion.
    ///
    /// WHY kept as a distinct variant: return handling in `function_body_to_ast`
    /// currently calls `is_numeric_coercible` directly rather than routing through
    /// `is_type_compatible`, but this variant is available for when that path is
    /// unified. Keeping it explicit documents the intended future direction.
    #[allow(dead_code)] // Planned: used once return handling routes through is_type_compatible.
    ReturnSlot,
}
