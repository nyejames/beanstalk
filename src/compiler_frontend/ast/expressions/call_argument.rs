//! Canonical AST call-argument metadata.
//!
//! WHAT: carries per-argument metadata for call-shaped AST nodes.
//! WHY: plain expression vectors cannot represent named-target routing or explicit call-site
//! mutable access markers, and mutable-place vs fresh-rvalue passing semantics.

use crate::compiler_frontend::ast::expressions::expression::Expression;
use crate::compiler_frontend::compiler_errors::SourceLocation;
use crate::compiler_frontend::symbols::string_interning::StringId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallAccessMode {
    Shared,
    Mutable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallPassingMode {
    Shared,
    MutablePlace,
    FreshMutableValue,
}

#[derive(Debug, Clone)]
pub struct CallArgument {
    /// The expression value being passed as this argument.
    pub value: Expression,

    /// For named arguments, the interned parameter name this argument routes to.
    pub target_param: Option<StringId>,

    /// Parse-time access marker from the argument syntax.
    ///
    /// WHAT: preserves whether the user wrote `~` in source.
    /// WHY: call-resolution diagnostics still depend on explicit marker intent.
    pub access_mode: CallAccessMode,

    /// Post-validation passing classification used by lowering/analysis.
    ///
    /// WHAT: distinguishes mutable-place calls from mutable fresh-rvalue calls.
    /// WHY: HIR lowering needs this explicit distinction to synthesize hidden locals only when
    /// needed, without rediscovering policy from expression shape.
    pub passing_mode: CallPassingMode,

    /// Source location of the argument expression.
    pub location: SourceLocation,

    /// For named arguments, the source location of the parameter name token.
    pub target_location: Option<SourceLocation>,
}

impl CallArgument {
    /// Map the parse-time access marker to its default passing-mode classification.
    ///
    /// WHAT: `Shared` stays shared; `Mutable` starts as `MutablePlace` pending validation.
    /// WHY: validation may later upgrade or downgrade the classification via `with_passing_mode`.
    fn passing_mode_from_access_mode(access_mode: CallAccessMode) -> CallPassingMode {
        match access_mode {
            CallAccessMode::Shared => CallPassingMode::Shared,
            // Parse-time `~` is provisional; validation confirms this is actually a mutable place.
            CallAccessMode::Mutable => CallPassingMode::MutablePlace,
        }
    }

    /// Build a positional call argument with the default passing mode derived from `access_mode`.
    pub fn positional(
        value: Expression,
        access_mode: CallAccessMode,
        location: SourceLocation,
    ) -> Self {
        Self {
            value,
            target_param: None,
            access_mode,
            passing_mode: Self::passing_mode_from_access_mode(access_mode),
            location,
            target_location: None,
        }
    }

    /// Build a named call argument with the default passing mode derived from `access_mode`.
    pub fn named(
        value: Expression,
        name: StringId,
        access_mode: CallAccessMode,
        location: SourceLocation,
        target_location: SourceLocation,
    ) -> Self {
        Self {
            value,
            target_param: Some(name),
            access_mode,
            passing_mode: Self::passing_mode_from_access_mode(access_mode),
            location,
            target_location: Some(target_location),
        }
    }

    /// Override the passing mode after validation has refined the provisional parse-time classification.
    pub fn with_passing_mode(mut self, passing_mode: CallPassingMode) -> Self {
        self.passing_mode = passing_mode;
        self
    }
}

/// Clone a call-argument slice into an owned vector.
///
/// WHAT: canonical no-op normalization for argument lists that are already resolved.
/// WHY: provides a single call site for expression constructors that expect `Vec<CallArgument>`.
pub(crate) fn normalize_call_arguments(args: &[CallArgument]) -> Vec<CallArgument> {
    args.to_vec()
}
