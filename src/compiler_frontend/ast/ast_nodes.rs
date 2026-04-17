//! Core AST node declarations and shared expression-ordering helpers.
//!
//! WHAT: defines statement/expression node shapes plus operator precedence metadata used by
//! expression evaluation.
//! WHY: parser output, type checking, and constant folding need one authoritative AST surface and
//! one precedence table so behavior stays deterministic across frontend stages.

use crate::compiler_frontend::ast::expressions::call_argument::{
    CallArgument, normalize_call_argument_values,
};
use crate::compiler_frontend::ast::expressions::expression::{
    Expression, ExpressionKind, Operator, ResultCallHandling,
};
use crate::compiler_frontend::ast::statements::branching::MatchArm;
use crate::compiler_frontend::ast::statements::functions::FunctionSignature;
use crate::compiler_frontend::builtins::BuiltinMethodKind;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::datatypes::{DataType, Ownership};
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::StringId;
pub(crate) use crate::compiler_frontend::tokenizer::tokens::SourceLocation;
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
    pub ownership: Ownership,
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

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone)]
pub enum NodeKind {
    // Control Flow
    Return(Vec<Expression>),                            // Return value,
    ReturnError(Expression),                            // return! value
    If(Expression, Vec<AstNode>, Option<Vec<AstNode>>), // Condition, If true, Else

    Match(
        Expression,           // Subject (condition)
        Vec<MatchArm>,        // Arms
        Option<Vec<AstNode>>, // for the wildcard/else case
    ),

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
        base: Box<AstNode>,   // The expression being accessed
        field: StringId,      // The field name
        data_type: DataType,  // Resolved type of this field access
        ownership: Ownership, // Ownership of the resolved field
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
            } => Ok(Expression::function_call(
                name.to_owned(),
                normalize_call_argument_values(arguments),
                result_types.to_owned(),
                location.to_owned(),
            )),

            NodeKind::HostFunctionCall {
                name,
                args: arguments,
                result_types,
                location,
            } => Ok(Expression::host_function_call(
                name.to_owned(),
                normalize_call_argument_values(arguments),
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
                normalize_call_argument_values(arguments),
                result_types.to_owned(),
                handling.to_owned(),
                location.to_owned(),
            )),

            NodeKind::FieldAccess {
                data_type,
                ownership,
                ..
            } => Ok(Expression::runtime(
                vec![self.to_owned()],
                data_type.to_owned(),
                self.location.to_owned(),
                ownership.to_owned(),
            )),
            NodeKind::MethodCall {
                result_types,
                location,
                ..
            } => Ok(Expression::runtime(
                vec![self.to_owned()],
                Expression::call_result_type(result_types.to_owned()),
                location.to_owned(),
                Ownership::MutableOwned,
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
                Operator::Root => 5,

                // High precedence: multiplication, division, modulus
                Operator::Multiply => 4,
                Operator::Divide => 4,
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
