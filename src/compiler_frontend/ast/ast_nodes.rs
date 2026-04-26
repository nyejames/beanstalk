//! Core AST node declarations and shared expression-ordering helpers.
//!
//! WHAT: defines statement/expression node shapes plus operator precedence metadata used by
//! expression evaluation.
//! WHY: parser output, type checking, and constant folding need one authoritative AST surface and
//! one precedence table so behavior stays deterministic across frontend stages.

use crate::compiler_frontend::ast::expressions::call_argument::{
    CallArgument, normalize_call_arguments,
};
use crate::compiler_frontend::ast::expressions::expression::{
    Expression, ExpressionKind, Operator, ResultCallHandling,
};
use crate::compiler_frontend::ast::statements::functions::FunctionSignature;
use crate::compiler_frontend::ast::statements::match_patterns::MatchArm;
use crate::compiler_frontend::builtins::{BuiltinMethodKind, CollectionBuiltinOp};
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::StringId;
pub(crate) use crate::compiler_frontend::tokenizer::tokens::SourceLocation;
use crate::compiler_frontend::value_mode::ValueMode;
use crate::return_compiler_error;

#[derive(Debug, Clone)]
pub struct Declaration {
    pub id: InternedPath,
    pub value: Expression,
}

impl Declaration {
    pub(crate) fn is_unresolved_constant_placeholder(&self) -> bool {
        matches!(self.value.kind, ExpressionKind::NoValue)
            && matches!(self.value.data_type, DataType::Inferred)
    }
}

#[derive(Debug, Clone)]
pub enum MultiBindTargetKind {
    Declaration,
    Assignment,
}

#[derive(Debug, Clone)]
pub struct MultiBindTarget {
    pub id: InternedPath,
    pub data_type: DataType,
    pub value_mode: ValueMode,
    pub kind: MultiBindTargetKind,
    pub location: SourceLocation,
}

#[derive(Debug, Clone)]
pub struct AstNode {
    pub kind: NodeKind,
    pub location: SourceLocation,
    pub scope: InternedPath,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RangeEndKind {
    Exclusive,
    Inclusive,
}

#[derive(Debug, Clone)]
pub struct LoopBindings {
    pub item: Option<Declaration>,
    pub index: Option<Declaration>,
}

#[derive(Debug, Clone)]
pub struct RangeLoopSpec {
    pub start: Expression,
    pub end: Expression,
    pub end_kind: RangeEndKind,
    pub step: Option<Expression>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchExhaustiveness {
    /// The match has an explicit `else =>` arm.
    HasDefault,
    /// The match has no explicit default, but AST proved it covers every choice variant.
    ExhaustiveChoice,
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone)]
pub enum NodeKind {
    // Control Flow
    Return(Vec<Expression>),                            // Return value,
    ReturnError(Expression),                            // return! value
    If(Expression, Vec<AstNode>, Option<Vec<AstNode>>), // Condition, If true, Else

    Match {
        scrutinee: Expression,
        arms: Vec<MatchArm>,
        default: Option<Vec<AstNode>>,
        exhaustiveness: MatchExhaustiveness,
    },

    ScopedBlock {
        body: Vec<AstNode>,
    },

    RangeLoop {
        bindings: LoopBindings,
        range: RangeLoopSpec,
        body: Vec<AstNode>,
    },
    CollectionLoop {
        bindings: LoopBindings,
        iterable: Expression,
        body: Vec<AstNode>,
    },
    WhileLoop(Expression, Vec<AstNode>), // Condition, Body,
    Break,
    Continue,

    // Basics
    VariableDeclaration(Declaration), // Variable name, Value, Visibility,

    /// Accumulate a runtime string expression into the entry start() fragment list.
    ///
    /// WHAT: replaces the old `VariableDeclaration(#template, ...)` protocol used to
    /// mark top-level runtime templates for later extraction/synthesis passes.
    /// WHY: explicit intent avoids encoding the protocol through synthetic variable names,
    /// and removes the need for post-hoc fragment extraction from the start body.
    /// The HIR builder lowers this into a `PushRuntimeFragment` statement inside entry start().
    PushStartRuntimeFragment(Expression),

    // For simple field access: obj.field
    FieldAccess {
        base: Box<AstNode>,    // The expression being accessed
        field: StringId,       // The field name
        data_type: DataType,   // Resolved type of this field access
        value_mode: ValueMode, // ValueMode of the resolved field
    },

    // For method calls: obj.method(args)
    MethodCall {
        receiver: Box<AstNode>,
        method_path: InternedPath,
        method: StringId,
        builtin: Option<BuiltinMethodKind>,
        args: Vec<CallArgument>,
        result_types: Vec<DataType>,
        location: SourceLocation,
    },

    // For compiler-owned collection builtins: collection.get/set/push/remove/length(...)
    CollectionBuiltinCall {
        receiver: Box<AstNode>,
        op: CollectionBuiltinOp,
        args: Vec<CallArgument>,
        result_types: Vec<DataType>,
        location: SourceLocation,
    },

    FunctionCall {
        name: InternedPath,
        args: Vec<CallArgument>,
        result_types: Vec<DataType>,
        location: SourceLocation,
        // bool, // Function is pure
    },

    ResultHandledFunctionCall {
        name: InternedPath,
        args: Vec<CallArgument>,
        result_types: Vec<DataType>,
        handling: ResultCallHandling,
        location: SourceLocation,
    },

    // Host function call (functions provided by the runtime)
    HostFunctionCall {
        name: InternedPath,
        args: Vec<CallArgument>,
        result_types: Vec<DataType>,
        location: SourceLocation,
    },

    // example: new_struct_instance = MyStructDefinition(arg1, arg2)
    //          new_struct_instance(arg) -- Calls the main function of the struct
    StructDefinition(
        InternedPath,     // Full unique name path
        Vec<Declaration>, // Fields
    ),

    Function(InternedPath, FunctionSignature, Vec<AstNode>),

    // Mutation of existing mutable variables
    Assignment {
        target: Box<AstNode>, // Variable, FieldAccess, Deref, etc.
        value: Expression,
    },

    MultiBind {
        targets: Vec<MultiBindTarget>,
        value: Expression,
    },

    // An actual r-value
    // Currently used for function calls and struct accesses
    Rvalue(Expression),

    // Operators
    // Operator, Precedence
    Operator(Operator), // Operator,
}

impl AstNode {
    pub fn get_expr(&self) -> Result<Expression, CompilerError> {
        match &self.kind {
            NodeKind::VariableDeclaration(arg) => Ok(arg.value.to_owned()),
            NodeKind::Rvalue(value, ..) => Ok(value.to_owned()),
            // NodeKind::Assignment(_, value) => Ok(value.to_owned()),
            NodeKind::FunctionCall {
                name,
                args: arguments,
                result_types,
                location,
            } => Ok(Expression::function_call_with_arguments(
                name.to_owned(),
                normalize_call_arguments(arguments),
                result_types.to_owned(),
                location.to_owned(),
            )),

            NodeKind::HostFunctionCall {
                name,
                args: arguments,
                result_types,
                location,
            } => Ok(Expression::host_function_call_with_arguments(
                name.to_owned(),
                normalize_call_arguments(arguments),
                result_types.to_owned(),
                location.to_owned(),
            )),

            NodeKind::ResultHandledFunctionCall {
                name,
                args: arguments,
                result_types,
                handling,
                location,
            } => Ok(Expression::result_handled_function_call(
                name.to_owned(),
                normalize_call_arguments(arguments),
                result_types.to_owned(),
                handling.to_owned(),
                location.to_owned(),
            )),

            NodeKind::FieldAccess {
                data_type,
                value_mode,
                ..
            } => Ok(Expression::runtime(
                vec![self.to_owned()],
                data_type.to_owned(),
                self.location.to_owned(),
                value_mode.to_owned(),
            )),
            NodeKind::MethodCall {
                result_types,
                location,
                ..
            }
            | NodeKind::CollectionBuiltinCall {
                result_types,
                location,
                ..
            } => Ok(Expression::runtime(
                vec![self.to_owned()],
                Expression::call_result_type(result_types.to_owned()),
                location.to_owned(),
                ValueMode::MutableOwned,
            )),
            // Compiler tried to get the expression of a node that cannot contain expressions
            _ => {
                return_compiler_error!(
                    "Compiler tried to get the expression of a non-expression AST node: {:?}",
                    &self.kind
                );
            }
        }
    }

    pub fn get_precedence(&self) -> u32 {
        match &self.kind {
            NodeKind::Operator(op) => match op {
                // Special Operators with the highest precedence
                Operator::Range => 6,
                Operator::Not => 6,

                // Highest precedence: exponentiation
                Operator::Exponent => 5,

                // High precedence: multiplication, division, modulus
                Operator::Multiply => 4,
                Operator::Divide => 4,
                Operator::IntDivide => 4,
                Operator::Modulus => 4,

                // Medium precedence: addition, subtraction
                Operator::Add => 3,
                Operator::Subtract => 3,

                // Comparisons
                Operator::LessThan => 2,
                Operator::LessThanOrEqual => 2,
                Operator::GreaterThan => 2,
                Operator::GreaterThanOrEqual => 2,
                Operator::Equality => 2,
                Operator::NotEqual => 2,

                // Logical AND
                Operator::And => 1,

                // Logical OR
                Operator::Or => 0,
            },
            _ => 0,
        }
    }

    pub fn is_left_associative(&self) -> bool {
        !matches!(self.kind, NodeKind::Operator(Operator::Exponent, ..))
    }
}
