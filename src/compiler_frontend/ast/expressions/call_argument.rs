//! Canonical AST call-argument metadata.
//!
//! WHAT: carries per-argument metadata for call-shaped AST nodes.
//! WHY: plain expression vectors cannot represent named-target routing or explicit call-site
//! mutable access markers.

use crate::compiler_frontend::ast::expressions::expression::Expression;
use crate::compiler_frontend::string_interning::StringId;

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
}

impl CallArgument {
    pub fn positional(value: Expression, access_mode: CallAccessMode) -> Self {
        Self {
            value,
            target_param: None,
            access_mode,
        }
    }

    pub fn named(value: Expression, name: StringId, access_mode: CallAccessMode) -> Self {
        Self {
            value,
            target_param: Some(name),
            access_mode,
        }
    }
}

pub(crate) fn call_argument_values(args: &[CallArgument]) -> Vec<Expression> {
    args.iter().map(|argument| argument.value.clone()).collect()
}
