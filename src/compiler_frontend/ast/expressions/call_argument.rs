//! Canonical AST call-argument metadata.
//!
//! WHAT: carries per-argument metadata for call-shaped AST nodes.
//! WHY: plain expression vectors cannot represent named-target routing or explicit call-site
//! mutable access markers.

use crate::compiler_frontend::ast::expressions::expression::Expression;
use crate::compiler_frontend::compiler_errors::SourceLocation;
use crate::compiler_frontend::symbols::string_interning::StringId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallAccessMode {
    Shared,
    Mutable,
}

#[derive(Debug, Clone)]
pub struct CallArgument {
    pub value: Expression,
    pub target_param: Option<StringId>,
    pub access_mode: CallAccessMode,
    pub location: SourceLocation,
    pub target_location: Option<SourceLocation>,
}

impl CallArgument {
    pub fn positional(
        value: Expression,
        access_mode: CallAccessMode,
        location: SourceLocation,
    ) -> Self {
        Self {
            value,
            target_param: None,
            access_mode,
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
            location,
            target_location: Some(target_location),
        }
    }
}

/// Converts validated, slot-ordered call arguments into value-only form.
///
/// WHAT: this is the explicit AST-to-lowering boundary for call arguments.
/// WHY: parser/validation layers require full `CallArgument` metadata, but HIR and expression
/// nodes consume only the final ordered value list after call normalization is complete.
pub(crate) fn normalize_call_argument_values(args: &[CallArgument]) -> Vec<Expression> {
    args.iter().map(|argument| argument.value.clone()).collect()
}
