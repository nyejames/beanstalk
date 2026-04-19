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
    pub value: Expression,
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
    pub location: SourceLocation,
    pub target_location: Option<SourceLocation>,
}

impl CallArgument {
    fn passing_mode_from_access_mode(access_mode: CallAccessMode) -> CallPassingMode {
        match access_mode {
            CallAccessMode::Shared => CallPassingMode::Shared,
            // Parse-time `~` is provisional; validation confirms this is actually a mutable place.
            CallAccessMode::Mutable => CallPassingMode::MutablePlace,
        }
    }

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

    pub fn with_passing_mode(mut self, passing_mode: CallPassingMode) -> Self {
        self.passing_mode = passing_mode;
        self
    }
}

pub(crate) fn normalize_call_arguments(args: &[CallArgument]) -> Vec<CallArgument> {
    args.to_vec()
}
