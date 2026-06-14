//! AST expression variants and operator contracts.
//!
//! WHAT: defines the runtime/compile-time shape of expression values once the
//! frontend has resolved their types.
//! WHY: separating the value shape from constructor helpers keeps expression
//! lowering review focused on the data contract first.

use crate::compiler_frontend::ast::ast_nodes::Declaration;
use crate::compiler_frontend::ast::expressions::call_argument::CallArgument;
use crate::compiler_frontend::ast::expressions::expression::Expression;
use crate::compiler_frontend::ast::expressions::expression_rpn::{ExpressionRpn, PlaceExpression};
use crate::compiler_frontend::ast::expressions::expression_types::{
    CastHandling, FallibleCarrierVariant, FallibleExpressionHandling, ResolvedCastEvidence,
};
use crate::compiler_frontend::ast::statements::functions::FunctionSignature;
use crate::compiler_frontend::ast::statements::value_production::types::ValueBlock;
use crate::compiler_frontend::ast::templates::template_types::Template;
use crate::compiler_frontend::builtins::CollectionBuiltinOp;
use crate::compiler_frontend::builtins::casts::targets::BuiltinCastTarget;
use crate::compiler_frontend::builtins::maps::MapBuiltinOp;
use crate::compiler_frontend::compiler_messages::source_location::SourceLocation;
use crate::compiler_frontend::datatypes::ids::TypeId;
use crate::compiler_frontend::external_packages::ExternalFunctionId;
use crate::compiler_frontend::paths::compile_time_paths::CompileTimePaths;
use crate::compiler_frontend::symbols::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::{StringId, StringIdRemap};

/// One key/value pair inside a `{...}` map literal.
///
/// WHAT: AST shape for map entries after key and value expressions have been
///       parsed and coerced.
/// WHY: map literals need a dedicated variant so lowering stages can distinguish
///      them from homogeneous collections.
#[derive(Clone, Debug)]
pub struct MapLiteralEntry {
    pub(crate) key: Expression,
    pub(crate) value: Expression,
}

/// Resolved AST representation of an explicit `cast` expression.
///
/// WHAT: records the source expression, target builtin type, selected evidence,
///      and handling form once the boundary target is known.
/// WHY: keeping the full resolution in one AST node lets later stages (folding,
///      HIR lowering) consume a single resolved fact instead of re-running trait
///      or evidence lookups.
#[derive(Clone, Debug)]
#[allow(dead_code)]
pub struct ResolvedCastExpression {
    pub(crate) source: Box<Expression>,
    pub(crate) source_type_id: TypeId,
    pub(crate) target_type_id: TypeId,
    pub(crate) target: BuiltinCastTarget,
    pub(crate) requires_optional_wrap_after_cast: bool,
    pub(crate) evidence: ResolvedCastEvidence,
    pub(crate) handling: CastHandling,
    pub(crate) location: SourceLocation,
}

#[derive(Clone, Debug)]
pub enum ExpressionKind {
    /// Internal sentinel for "no source value was provided" (for example, a
    /// parameter default that is intentionally absent).
    NoValue,

    /// User-authored `none` literal in an explicit option context.
    OptionNone,

    /// A runtime expression fragment that could not be constant-folded.
    ///
    /// WHAT: carries an expression-owned RPN stack for expressions whose value is only
    ///       known at runtime.
    /// WHY: operands are `Expression` values, not general `AstNode` fragments, so runtime
    ///      RPN cannot smuggle statement bodies into value contexts.
    Runtime(ExpressionRpn),

    Int(i32),
    Float(f64),
    StringSlice(StringId),
    Bool(bool),
    Char(char),

    /// Compile-time path literal(s) — one or more resolved paths from grouped syntax.
    ///
    /// Deferred until source path expression parsing is wired. Retained because const folding,
    /// HIR lowering, and path tests already share this AST shape.
    Path(Box<CompileTimePaths>),

    /// Reference to a variable by name.
    Reference(InternedPath),

    /// Explicitly materialize a fresh value from an aliasing place.
    Copy(PlaceExpression),

    /// Functions are first-class values.
    ///
    /// WHAT: carries callable signature metadata only. Function bodies are statement-level
    /// `AstNode::Function` payloads and must not be stored inside expressions.
    Function(FunctionSignature),

    /// Infallible user function call.
    ///
    /// `result_type_ids` are canonical semantic identities from the active
    /// `TypeEnvironment`; display spelling is recovered separately when needed.
    FunctionCall {
        name: InternedPath,
        args: Vec<CallArgument>,
        result_type_ids: Vec<TypeId>,
    },

    /// Field/member access on a value expression.
    FieldAccess {
        base: Box<Expression>,
        field: StringId,
    },

    /// Receiver method call.
    MethodCall {
        receiver: Box<Expression>,
        method_path: InternedPath,
        method: StringId,
        args: Vec<CallArgument>,
        result_type_ids: Vec<TypeId>,
        location: SourceLocation,
    },

    /// Compiler-owned collection builtin call (`get`, `set`, `push`, `remove`, `length`).
    CollectionBuiltinCall {
        receiver: Box<Expression>,
        op: CollectionBuiltinOp,
        receiver_requires_mutable: bool,
        args: Vec<CallArgument>,
        result_type_ids: Vec<TypeId>,
        location: SourceLocation,
    },

    /// Compiler-owned map builtin call (`get`, `contains`, `set`, `remove`, `clear`, `length`).
    MapBuiltinCall {
        receiver: Box<Expression>,
        op: MapBuiltinOp,
        receiver_requires_mutable: bool,
        args: Vec<CallArgument>,
        result_type_ids: Vec<TypeId>,
        location: SourceLocation,
    },

    /// User function call with explicit `!` or `catch` handling.
    ///
    /// The success slots stay TypeId-first so HIR lowering never needs
    /// diagnostic return spelling to build call result values.
    HandledFallibleFunctionCall {
        name: InternedPath,
        args: Vec<CallArgument>,
        result_type_ids: Vec<TypeId>,
        handling: FallibleExpressionHandling,
    },

    /// External fallible function call with explicit handling.
    ///
    /// The error slot is kept as a TypeId alongside success `result_type_ids`;
    /// backend package spelling is not part of executable AST identity.
    HandledFallibleHostFunctionCall {
        id: ExternalFunctionId,
        args: Vec<CallArgument>,
        result_type_ids: Vec<TypeId>,
        error_type_id: TypeId,
        handling: FallibleExpressionHandling,
    },

    /// Explicit `cast` / `cast!` expression resolved at an explicit typed boundary.
    ///
    /// WHAT: carries the resolved source, target, evidence, and handling form.
    /// WHY: the cast surface models builtin and user-defined evidence, fallibility
    ///      forms, and optional target wrapping in one resolved AST node before HIR
    ///      consumes it.
    Cast(ResolvedCastExpression),

    /// Construct a `Success` or `Failure` carrier value.
    FallibleCarrierConstruct {
        variant: FallibleCarrierVariant,
        value: Box<Expression>,
    },

    /// An expression with explicit fallible handling (`!` or `catch`).
    HandledFallibleExpression {
        value: Box<Expression>,
        handling: FallibleExpressionHandling,
    },

    /// Postfix option propagation (`expr?`).
    ///
    /// WHAT: unwraps `T?` to `T` on the present path and returns `none` from
    ///       the current function on the absent path.
    /// WHY: option propagation is control flow like fallible propagation, but
    ///      options are ordinary values and do not use the internal result carrier.
    OptionPropagation {
        value: Box<Expression>,
    },

    /// Infallible external function call.
    ///
    /// HIR carries the stable external function ID and canonical result TypeIds;
    /// backends map the external ID to target-specific runtime names later.
    HostFunctionCall {
        id: ExternalFunctionId,
        args: Vec<CallArgument>,
        result_type_ids: Vec<TypeId>,
    },

    /// Equivalent to a string when folded at compile time.
    Template(Box<Template>),

    /// Homogeneous collection literal.
    Collection(Vec<Expression>),

    /// Ordered map literal with key = value entries.
    ///
    /// WHAT: carries typed key/value pairs after frontend parsing and coercion.
    /// WHY: map literals are a distinct language construct from collections;
    ///      they require separate HIR lowering and backend runtime support.
    MapLiteral(Vec<MapLiteralEntry>),

    /// Struct type definition literal.
    StructDefinition(Vec<Declaration>),

    /// Struct instance construction literal.
    StructInstance(Vec<Declaration>),

    /// Inclusive range operator (`..`). Kept as a dedicated variant to simplify
    /// constant folding; this may become a general operator in the future.
    Range(Box<Expression>, Box<Expression>),

    /// An implicit contextual coercion applied by the compiler at an explicit
    /// type boundary. The inner value retains its original expression kind;
    /// `to_type` records the canonical promoted target so lowering stages can
    /// emit the correct conversion.
    ///
    /// WHY a separate variant: silent type pretending (e.g. storing an `Int`
    /// expression but calling it `Float`, or passing `String` where `String?`
    /// is expected) makes later lowering fragile. An explicit `Coerced` node
    /// makes the coercion visible and auditable.
    Coerced {
        value: Box<Expression>,
        to_type: TypeId,
    },

    /// Explicit choice variant construction: `Choice::Variant` or `Choice::Variant(...)`.
    ///
    /// WHY: choice values must not masquerade as raw integer literals in AST.
    /// The tag index is deterministic but is an implementation detail; the
    /// nominal path and variant name are the semantic identity.
    /// For unit variants, `fields` is empty. For payload variants, `fields`
    /// carries the resolved constructor arguments in declaration order.
    ChoiceConstruct {
        nominal_path: InternedPath,
        variant: StringId,
        tag: usize,
        fields: Vec<Declaration>,
    },

    /// A value-producing control-flow block used only at closed receiving sites.
    ///
    /// WHAT: wraps `ValueBlock` variants (`if`, future `match`, `catch`) into an
    ///       expression shape so receiving sites can type-check and lower them.
    /// WHY: value blocks are not general expressions; this variant keeps them
    ///      distinguishable from ordinary statements while allowing them to
    ///      appear as declaration initializers and assignment right-hand sides.
    ValueBlock {
        block: Box<ValueBlock>,
    },
}

impl ExpressionKind {
    /// Whether this expression can be folded to a constant at compile time.
    pub fn is_foldable(&self) -> bool {
        matches!(
            self,
            ExpressionKind::Int(_)
                | ExpressionKind::Float(_)
                | ExpressionKind::Bool(_)
                | ExpressionKind::StringSlice(_)
                | ExpressionKind::Char(_)
                | ExpressionKind::Path(_)
                | ExpressionKind::ChoiceConstruct { .. }
        )
    }

    /// Whether this expression contains runtime control flow that must be lowered
    /// into HIR statements rather than kept as a pure nested expression.
    ///
    /// WHAT: value blocks allocate locals and emit terminators during lowering,
    ///       so they cannot stay inside a returned expression tree.
    /// WHY: `lower_child_expression_for_parent` needs to flush pending preludes
    ///      before any expression that manipulates the current HIR block.
    pub fn contains_statement_like_control_flow(&self) -> bool {
        matches!(
            self,
            ExpressionKind::OptionPropagation { .. } | ExpressionKind::ValueBlock { .. }
        )
    }

    /// Remap all interned string IDs and paths in this expression kind recursively.
    ///
    /// Called by per-file frontend output remapping before module-wide dependency sorting.
    pub fn remap_string_ids(&mut self, remap: &StringIdRemap) {
        match self {
            // Leaf variants: nothing to remap.
            ExpressionKind::NoValue
            | ExpressionKind::OptionNone
            | ExpressionKind::Int(_)
            | ExpressionKind::Float(_)
            | ExpressionKind::Bool(_)
            | ExpressionKind::Char(_) => {}

            ExpressionKind::StringSlice(name) => {
                *name = remap.get(*name);
            }

            ExpressionKind::Path(paths) => {
                paths.remap_string_ids(remap);
            }

            ExpressionKind::Reference(path) => {
                path.remap_string_ids(remap);
            }

            ExpressionKind::Copy(place) => {
                place.remap_string_ids(remap);
            }

            ExpressionKind::Function(signature) => {
                signature.remap_string_ids(remap);
            }

            ExpressionKind::FunctionCall { name, args, .. } => {
                name.remap_string_ids(remap);
                for arg in args {
                    arg.remap_string_ids(remap);
                }
            }

            ExpressionKind::FieldAccess { base, field } => {
                base.remap_string_ids(remap);
                *field = remap.get(*field);
            }

            ExpressionKind::MethodCall {
                receiver,
                method_path,
                method,
                args,
                location,
                ..
            } => {
                receiver.remap_string_ids(remap);
                method_path.remap_string_ids(remap);
                *method = remap.get(*method);
                for arg in args {
                    arg.remap_string_ids(remap);
                }
                location.remap_string_ids(remap);
            }

            ExpressionKind::CollectionBuiltinCall {
                receiver,
                args,
                location,
                ..
            }
            | ExpressionKind::MapBuiltinCall {
                receiver,
                args,
                location,
                ..
            } => {
                receiver.remap_string_ids(remap);
                for arg in args {
                    arg.remap_string_ids(remap);
                }
                location.remap_string_ids(remap);
            }

            ExpressionKind::HandledFallibleFunctionCall { name, args, .. } => {
                name.remap_string_ids(remap);
                for arg in args {
                    arg.remap_string_ids(remap);
                }
            }

            ExpressionKind::HandledFallibleHostFunctionCall { args, .. } => {
                for arg in args {
                    arg.remap_string_ids(remap);
                }
            }

            // These variants wrap a single inner expression.
            ExpressionKind::FallibleCarrierConstruct { value, .. }
            | ExpressionKind::OptionPropagation { value }
            | ExpressionKind::Coerced { value, .. } => {
                value.remap_string_ids(remap);
            }

            ExpressionKind::Cast(cast) => {
                cast.source.remap_string_ids(remap);
                cast.evidence.remap_string_ids(remap);
                cast.handling.remap_string_ids(remap);
            }

            ExpressionKind::HandledFallibleExpression { value, .. } => {
                value.remap_string_ids(remap);
            }

            ExpressionKind::HostFunctionCall { args, .. } => {
                for arg in args {
                    arg.remap_string_ids(remap);
                }
            }

            ExpressionKind::Template(template) => {
                template.remap_string_ids(remap);
            }

            ExpressionKind::Collection(items) => {
                for item in items {
                    item.remap_string_ids(remap);
                }
            }

            ExpressionKind::MapLiteral(entries) => {
                for entry in entries {
                    entry.key.remap_string_ids(remap);
                    entry.value.remap_string_ids(remap);
                }
            }

            ExpressionKind::StructDefinition(fields) | ExpressionKind::StructInstance(fields) => {
                for field in fields {
                    field.remap_string_ids(remap);
                }
            }

            ExpressionKind::Range(start, end) => {
                start.remap_string_ids(remap);
                end.remap_string_ids(remap);
            }

            ExpressionKind::ChoiceConstruct {
                nominal_path,
                variant,
                fields,
                ..
            } => {
                nominal_path.remap_string_ids(remap);
                *variant = remap.get(*variant);
                for field in fields {
                    field.remap_string_ids(remap);
                }
            }

            ExpressionKind::Runtime(nodes) => {
                nodes.remap_string_ids(remap);
            }

            ExpressionKind::ValueBlock { block } => {
                block.remap_string_ids(remap);
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Operator {
    // Arithmetic
    Add,
    Subtract,
    Multiply,
    Divide,
    IntDivide,
    Modulus,
    Exponent,

    // Comparison and logical
    And,
    Or,
    GreaterThan,
    GreaterThanOrEqual,
    LessThan,
    LessThanOrEqual,
    Equality,
    NotEqual,

    // Unary logical negation
    Not,
    // Unary numeric negation
    Negate,

    // Range construction
    Range,
}

impl Operator {
    /// Precedence used by expression shunting-yard ordering.
    pub fn precedence(&self) -> u32 {
        match self {
            // Special unary/range operators bind most tightly.
            Operator::Range | Operator::Not | Operator::Negate => 6,

            // Exponentiation is right-associative, but still lower than unary operators.
            Operator::Exponent => 5,

            Operator::Multiply | Operator::Divide | Operator::IntDivide | Operator::Modulus => 4,

            Operator::Add | Operator::Subtract => 3,

            Operator::LessThan
            | Operator::LessThanOrEqual
            | Operator::GreaterThan
            | Operator::GreaterThanOrEqual
            | Operator::Equality
            | Operator::NotEqual => 2,

            Operator::And => 1,

            Operator::Or => 0,
        }
    }

    /// Whether this operator associates left-to-right during shunting-yard ordering.
    pub fn is_left_associative(&self) -> bool {
        !matches!(self, Operator::Exponent)
    }

    /// Number of operand expressions required by this operator.
    pub fn required_values(&self) -> usize {
        match self {
            Operator::Add
            | Operator::Subtract
            | Operator::Multiply
            | Operator::Divide
            | Operator::IntDivide
            | Operator::Modulus
            | Operator::Exponent
            | Operator::And
            | Operator::Or
            | Operator::GreaterThan
            | Operator::GreaterThanOrEqual
            | Operator::LessThan
            | Operator::LessThanOrEqual
            | Operator::Range
            | Operator::Equality
            | Operator::NotEqual => 2,

            Operator::Not | Operator::Negate => 1,
        }
    }

    /// Source spelling for this operator, used in diagnostics and debug output.
    pub fn to_str(&self) -> &str {
        match self {
            Operator::Add => "+",
            Operator::Subtract => "-",
            Operator::Multiply => "*",
            Operator::Divide => "/",
            Operator::IntDivide => "//",
            Operator::Modulus => "%",
            Operator::Exponent => "^",
            Operator::And => "and",
            Operator::Or => "or",
            Operator::GreaterThan => ">",
            Operator::GreaterThanOrEqual => ">=",
            Operator::LessThan => "<",
            Operator::LessThanOrEqual => "<=",
            Operator::Equality => "is",
            Operator::NotEqual => "is not",
            Operator::Not => "not",
            Operator::Negate => "-",
            Operator::Range => "..",
        }
    }
}
