//! Match-pattern types.
//!
//! WHAT: defines the data shapes produced by pattern parsing.
//! WHY: separating types from parsing logic keeps the public contract readable
//! and prevents circular imports between parser submodules.

use crate::compiler_frontend::ast::ast_nodes::AstNode;
use crate::compiler_frontend::ast::expressions::expression::Expression;
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::StringId;
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;

#[derive(Debug, Clone)]
pub struct MatchArm {
    pub pattern: MatchPattern,
    pub guard: Option<Expression>,
    pub body: Vec<AstNode>,
}

/// One payload field capture inside a choice-variant match pattern.
///
/// WHY: match arms can destructure payload variants by binding each field to a
/// local name. Captured names must exactly match the declared field
/// names in declaration order.
#[derive(Debug, Clone)]
pub struct ParsedChoicePayloadCapture {
    pub field_name: StringId,
    pub binding_name: StringId,
    pub field_index: usize,
    pub field_type: DataType,
    pub location: SourceLocation,
    pub binding_location: SourceLocation,
}

#[derive(Debug, Clone)]
pub struct ChoicePayloadCapture {
    pub field_name: StringId,
    pub binding_name: StringId,
    pub field_index: usize,
    pub field_type: DataType,
    pub binding_path: InternedPath,
    pub location: SourceLocation,
    pub binding_location: SourceLocation,
}

#[derive(Debug, Clone)]
pub enum MatchPattern {
    Literal(Expression),

    Wildcard {
        location: SourceLocation,
    },

    Relational {
        op: RelationalPatternOp,
        value: Expression,
        location: SourceLocation,
    },

    ChoiceVariant {
        nominal_path: InternedPath,
        variant: StringId,
        tag: usize,
        captures: Vec<ChoicePayloadCapture>,
        location: SourceLocation,
    },
}

impl MatchPattern {
    pub fn location(&self) -> &SourceLocation {
        match self {
            MatchPattern::Literal(expression) => &expression.location,
            MatchPattern::Wildcard { location }
            | MatchPattern::Relational { location, .. }
            | MatchPattern::ChoiceVariant { location, .. } => location,
        }
    }

    /// Return the capture list if this is a choice-variant pattern.
    pub fn choice_captures(&self) -> Option<&[ChoicePayloadCapture]> {
        match self {
            MatchPattern::ChoiceVariant { captures, .. } => Some(captures),
            _ => None,
        }
    }
}

/// Result of parsing a choice-variant pattern in a match arm.
pub struct ParsedChoicePattern {
    pub nominal_path: InternedPath,
    pub variant: StringId,
    pub tag: usize,
    pub captures: Vec<ParsedChoicePayloadCapture>,
    pub location: SourceLocation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelationalPatternOp {
    LessThan,
    LessThanOrEqual,
    GreaterThan,
    GreaterThanOrEqual,
}

/// Unwrap a `Reference` wrapper so pattern checks compare against the inner value type.
pub fn normalized_subject_type(data_type: &DataType) -> &DataType {
    match data_type {
        DataType::Reference(inner) => inner.as_ref(),
        _ => data_type,
    }
}
