use crate::compiler_frontend::ast::expressions::expression::{
    Expression, ExpressionKind, Operator, ResultCallHandling,
};
use crate::compiler_frontend::ast::expressions::call_argument::{
    CallArgument, normalize_call_argument_values,
};
use crate::compiler_frontend::ast::statements::branching::MatchArm;
use crate::compiler_frontend::ast::statements::functions::FunctionSignature;
use crate::compiler_frontend::builtins::BuiltinMethodKind;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::datatypes::{DataType, Ownership};
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::string_interning::{StringId, StringTable};
pub(crate) use crate::compiler_frontend::tokenizer::tokens::SourceLocation;
use crate::{return_compiler_error, return_type_error};

#[derive(Debug, Clone)]
pub struct Declaration {
    pub id: InternedPath,
    pub value: Expression,
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
pub struct ForLoopRange {
    pub start: Expression,
    pub end: Expression,
    pub end_kind: RangeEndKind,
    pub step: Option<Expression>,
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone)]
pub enum NodeKind {
    // Warning Message
    // This could be stuff like unused variables, possible race conditions, etc
    #[allow(dead_code)] // Planned: surfaced warning nodes for editor/diagnostic pipelines.
    Warning(String), // Message, Start pos, End pos

    // Config settings
    #[allow(dead_code)] // Planned: config AST nodes for unified config parsing.
    Config(Vec<Declaration>), // Settings,

    // Control Flow
    Return(Vec<Expression>),                            // Return value,
    ReturnError(Expression),                            // return! value
    If(Expression, Vec<AstNode>, Option<Vec<AstNode>>), // Condition, If true, Else

    Match(
        Expression,           // Subject (condition)
        Vec<MatchArm>,        // Arms
        Option<Vec<AstNode>>, // for the wildcard/else case
    ),

    ForLoop(Box<Declaration>, ForLoopRange, Vec<AstNode>), // Item, Range, Body,
    WhileLoop(Expression, Vec<AstNode>),                   // Condition, Body,
    Break,
    Continue,

    // Basics
    VariableDeclaration(Declaration), // Variable name, Value, Visibility,

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

    #[allow(dead_code)] // Planned: explicit template statement nodes in later AST cleanup.
    Template(Expression),
    #[allow(dead_code)] // Planned: dedicated top-level template nodes.
    TopLevelTemplate(Expression),
    #[allow(dead_code)] // Planned: explicit slot marker nodes during template lowering.
    Slot,
    #[allow(dead_code)] // Planned: placeholder nodes used during parser normalization.
    Empty, //

    // Operators
    // Operator, Precedence
    Operator(Operator), // Operator,

    #[allow(dead_code)] // Planned: newline sentinel nodes for formatting-aware passes.
    Newline,
    #[allow(dead_code)] // Planned: whitespace sentinel nodes for formatting-aware passes.
    Spaces(u32),
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

    // If this is a boolean value, flip it to the opposite value
    pub fn flip(&mut self, _string_table: &StringTable) -> Result<bool, CompilerError> {
        if let NodeKind::Rvalue(value) = &mut self.kind {
            match value.kind {
                ExpressionKind::Bool(val) => {
                    value.kind = ExpressionKind::Bool(!val);
                    return Ok(true);
                }
                ExpressionKind::Runtime(_) => {
                    if !value.ownership.is_mutable() {
                        return_type_error!(
                            "Tried to use the 'not' operator on a non-mutable value",
                            self.location.to_owned(), {
                                ExpectedType => "Boolean",
                                BorrowKind => "Shared",
                                LifetimeHint => "This value is borrowed",
                            }
                        )
                    } else {
                        return_type_error!(
                            format!(
                                "Tried to use the 'not' operator on value of type {:?}",
                                value.data_type
                            ),
                            self.location.to_owned(), {
                                ExpectedType => "Boolean",
                                BorrowKind => "Shared",
                                LifetimeHint => "This value is borrowed",
                            }
                        )
                    }
                }
                _ => {}
            }
        }

        return_type_error!(
            format!("Tried to use the 'not' operator on a non-boolean: {:?}", self.kind),
            self.location.to_owned(), {
                ExpectedType => "Boolean",
            }
        );
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
