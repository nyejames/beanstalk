//! Match-pattern types.
//!
//! WHAT: defines the data shapes produced by pattern parsing.
//! WHY: separating types from parsing logic keeps the public contract readable
//! and prevents circular imports between parser submodules.

use crate::compiler_frontend::ast::ast_nodes::AstNode;
use crate::compiler_frontend::ast::expressions::expression::Expression;
use crate::compiler_frontend::datatypes::ids::TypeId;
use crate::compiler_frontend::symbols::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::{StringId, StringIdRemap};
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;

/// One arm of a match expression, pairing a pattern with an optional guard and body.
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
    pub type_id: TypeId,
    pub location: SourceLocation,
    pub binding_location: SourceLocation,
}

/// Resolved payload capture for a choice-variant match pattern.
///
/// Produced after path resolution; carries the fully-qualified `binding_path`
/// that later lowering stages use to register the arm-local binding.
#[derive(Debug, Clone)]
pub struct ChoicePayloadCapture {
    pub field_name: StringId,
    pub binding_name: StringId,
    pub field_index: usize,
    pub type_id: TypeId,
    pub binding_path: InternedPath,
    pub location: SourceLocation,
    pub binding_location: SourceLocation,
}

#[derive(Debug, Clone)]
pub enum MatchPattern {
    Literal(Expression),

    /// Presence check for compiler-owned option values.
    ///
    /// WHAT: `none =>` matches only the absent branch of a `T?` scrutinee.
    /// WHY: option matching intentionally supports presence checks and capture
    /// patterns without introducing public `Option` constructors.
    OptionNone {
        location: SourceLocation,
    },

    /// Value comparison against the inner payload of a compiler-owned option.
    ///
    /// WHAT: `<literal> =>` on a `T?` scrutinee checks the option is present
    /// and then compares the contained `T` value.
    /// WHY: option matches use explicit option-aware pattern forms rather than
    /// exposing public `Option` constructors in source code.
    OptionValue {
        value: Expression,
        location: SourceLocation,
    },

    /// Present-value capture for compiler-owned option values.
    ///
    /// WHAT: `|name| =>` on a `T?` scrutinee matches any present value and binds
    /// the inner `T` payload to `name` for the guard and arm body.
    /// WHY: option unwrapping uses the same capture-local registration and
    /// guard-substitution model as choice payload captures.
    OptionPresentCapture {
        name: StringId,
        binding_path: InternedPath,
        inner_type_id: TypeId,
        location: SourceLocation,
        binding_location: SourceLocation,
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

    /// General capture pattern that binds the entire scrutinee value.
    ///
    /// WHAT: a bare symbol in pattern position that is not a known constructor
    /// becomes a capture binding visible only in the arm guard and body.
    Capture {
        name: StringId,
        binding_path: InternedPath,
        location: SourceLocation,
    },
}

impl MatchPattern {
    pub fn location(&self) -> &SourceLocation {
        match self {
            MatchPattern::Literal(expression) => &expression.location,

            MatchPattern::OptionNone { location }
            | MatchPattern::OptionValue { location, .. }
            | MatchPattern::OptionPresentCapture { location, .. } => location,

            MatchPattern::Relational { location, .. }
            | MatchPattern::ChoiceVariant { location, .. }
            | MatchPattern::Capture { location, .. } => location,
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

impl MatchArm {
    /// Remap pattern, guard, and body expressions/nodes recursively.
    ///
    /// Called by per-file frontend output remapping before module-wide dependency sorting.
    pub fn remap_string_ids(&mut self, remap: &StringIdRemap) {
        self.pattern.remap_string_ids(remap);
        if let Some(guard) = &mut self.guard {
            guard.remap_string_ids(remap);
        }
        for node in &mut self.body {
            node.remap_string_ids(remap);
        }
    }
}

impl MatchPattern {
    /// Remap interned names, paths, and nested expressions in this match pattern.
    ///
    /// Called by per-file frontend output remapping before module-wide dependency sorting.
    pub fn remap_string_ids(&mut self, remap: &StringIdRemap) {
        match self {
            MatchPattern::Literal(expression) => {
                expression.remap_string_ids(remap);
            }

            MatchPattern::OptionNone { location } => {
                location.remap_string_ids(remap);
            }

            MatchPattern::OptionValue { value, location } => {
                value.remap_string_ids(remap);
                location.remap_string_ids(remap);
            }

            MatchPattern::OptionPresentCapture {
                name,
                binding_path,
                location,
                binding_location,
                ..
            } => {
                *name = remap.get(*name);
                binding_path.remap_string_ids(remap);
                location.remap_string_ids(remap);
                binding_location.remap_string_ids(remap);
            }

            MatchPattern::Relational {
                value, location, ..
            } => {
                value.remap_string_ids(remap);
                location.remap_string_ids(remap);
            }

            MatchPattern::ChoiceVariant {
                nominal_path,
                variant,
                captures,
                location,
                ..
            } => {
                nominal_path.remap_string_ids(remap);
                *variant = remap.get(*variant);
                for capture in captures {
                    capture.remap_string_ids(remap);
                }
                location.remap_string_ids(remap);
            }

            MatchPattern::Capture {
                name,
                binding_path,
                location,
            } => {
                *name = remap.get(*name);
                binding_path.remap_string_ids(remap);
                location.remap_string_ids(remap);
            }
        }
    }
}

impl ChoicePayloadCapture {
    /// Remap field names, binding names, and paths in this capture.
    ///
    /// Called by per-file frontend output remapping before module-wide dependency sorting.
    pub fn remap_string_ids(&mut self, remap: &StringIdRemap) {
        self.field_name = remap.get(self.field_name);
        self.binding_name = remap.get(self.binding_name);
        self.binding_path.remap_string_ids(remap);
        self.location.remap_string_ids(remap);
        self.binding_location.remap_string_ids(remap);
    }
}

/// Relational operators allowed in match patterns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelationalPatternOp {
    LessThan,
    LessThanOrEqual,
    GreaterThan,
    GreaterThanOrEqual,
}
