use crate::compiler::compiler_errors::ErrorType;
use crate::compiler::compiler_errors::CompileError;
use crate::compiler::datatypes::DataType;
use crate::compiler::parsers::ast_nodes::{Arg, AstNode};
use crate::compiler::parsers::build_ast::AstBlock;
use crate::compiler::parsers::scene::{SceneContent, Style};
use crate::compiler::parsers::tokens::TextLocation;
use crate::return_rule_error;

// The possible values of any type.
// Return 'Runtime' if the value is not known at compile time
#[derive(Debug, Clone, PartialEq)]
pub struct Expression {
    pub kind: ExpressionKind,
    pub data_type: DataType,
    pub location: TextLocation,
}

impl Expression {
    pub fn as_string(&self) -> String {
        match &self.kind {
            ExpressionKind::String(string) => string.to_owned(),
            ExpressionKind::Int(int) => int.to_string(),
            ExpressionKind::Float(float) => float.to_string(),
            ExpressionKind::Bool(bool) => bool.to_string(),
            ExpressionKind::Scene(..) => String::new(),
            ExpressionKind::Collection(items, ..) => {
                let mut all_items = String::new();
                for item in items {
                    all_items.push_str(&item.as_string());
                }
                all_items
            }
            ExpressionKind::Struct(args) => {
                let mut all_items = String::new();
                for arg in args {
                    all_items.push_str(&arg.value.as_string());
                }
                all_items
            }
            ExpressionKind::Function(..) => String::new(),
            ExpressionKind::Reference(..) => String::new(),
            ExpressionKind::Runtime(..) => String::new(),
            ExpressionKind::None => String::new(),
        }
    }

    pub fn new(kind: ExpressionKind, location: TextLocation) -> Self {
        Self {
            data_type: kind.get_type(),
            kind,
            location,
        }
    }
    pub fn none() -> Self {
        Self {
            data_type: DataType::None,
            kind: ExpressionKind::None,
            location: TextLocation::default(),
        }
    }
    pub fn reference(name: String, location: TextLocation) -> Self {
        Self {
            data_type: DataType::Inferred(false),
            kind: ExpressionKind::Reference(name),
            location,
        }
    }
    pub fn runtime(expressions: Vec<AstNode>, data_type: DataType, location: TextLocation) -> Self {
        Self {
            data_type,
            kind: ExpressionKind::Runtime(expressions),
            location,
        }
    }
    pub fn int(value: i32, location: TextLocation) -> Self {
        Self {
            data_type: DataType::Int(false),
            kind: ExpressionKind::Int(value),
            location,
        }
    }
    pub fn float(value: f64, location: TextLocation) -> Self {
        Self {
            data_type: DataType::Float(false),
            kind: ExpressionKind::Float(value),
            location,
        }
    }
    pub fn string(value: String, location: TextLocation) -> Self {
        Self {
            data_type: DataType::String(false),
            kind: ExpressionKind::String(value),
            location,
        }
    }
    pub fn bool(value: bool, location: TextLocation) -> Self {
        Self {
            data_type: DataType::Bool(false),
            kind: ExpressionKind::Bool(value),
            location,
        }
    }

    // Creating Functions
    pub fn function(
        args: Vec<Arg>,
        body: AstBlock,
        return_types: Vec<DataType>,
        location: TextLocation,
    ) -> Self {
        Self {
            data_type: DataType::Function(args.to_owned(), return_types.to_owned()),
            kind: ExpressionKind::Function(args.to_owned(), body.ast, vec![]),
            location,
        }
    }
    pub fn function_without_signature(body: AstBlock, location: TextLocation) -> Self {
        Self {
            data_type: DataType::Inferred(false),
            kind: ExpressionKind::Function(vec![], body.ast, vec![]),
            location,
        }
    }
    pub fn function_without_return(
        args: Vec<Arg>,
        body: Vec<AstNode>,
        location: TextLocation,
    ) -> Self {
        Self {
            data_type: DataType::Inferred(false),
            kind: ExpressionKind::Function(args, body, vec![]),
            location,
        }
    }
    pub fn function_without_args(
        body: Vec<AstNode>,
        return_types: Vec<DataType>,
        location: TextLocation,
    ) -> Self {
        Self {
            data_type: DataType::Inferred(false),
            kind: ExpressionKind::Function(vec![], body, return_types),
            location,
        }
    }

    pub fn collection(items: Vec<Expression>, location: TextLocation) -> Self {
        Self {
            data_type: DataType::Inferred(false),
            kind: ExpressionKind::Collection(items),
            location,
        }
    }
    pub fn structure(args: Vec<Arg>, location: TextLocation) -> Self {
        Self {
            data_type: DataType::Inferred(false),
            kind: ExpressionKind::Struct(args),
            location,
        }
    }
    pub fn scene(body: SceneContent, styles: Style, id: String, location: TextLocation) -> Self {
        Self {
            data_type: DataType::Scene(false),
            kind: ExpressionKind::Scene(body, styles, id),
            location,
        }
    }

    pub fn is_none(&self) -> bool {
        matches!(self.kind, ExpressionKind::None)
    }

    // Evaluates a binary operation between two expressions based on the operator
    // This helps with constant folding by handling type-specific operations
    pub fn evaluate_operator(&self, rhs: &Expression, op: &Operator) -> Result<Option<Expression>, CompileError> {
        
        let kind: ExpressionKind = match (&self.kind, &rhs.kind) {
            // Float operations
            (ExpressionKind::Float(lhs_val), ExpressionKind::Float(rhs_val)) => {
                match op {
                    Operator::Add => ExpressionKind::Float(lhs_val + rhs_val),
                    Operator::Subtract => ExpressionKind::Float(lhs_val - rhs_val),
                    Operator::Multiply => ExpressionKind::Float(lhs_val * rhs_val),
                    Operator::Divide => ExpressionKind::Float(lhs_val / rhs_val),
                    Operator::Modulus => ExpressionKind::Float(lhs_val % rhs_val),
                    Operator::Exponent => ExpressionKind::Float(lhs_val.powf(*rhs_val)),

                    // Logical operations with float operands
                    Operator::Equality => ExpressionKind::Bool(lhs_val == rhs_val),
                    Operator::NotEqual => ExpressionKind::Bool(lhs_val != rhs_val),
                    Operator::GreaterThan => ExpressionKind::Bool(lhs_val > rhs_val),
                    Operator::GreaterThanOrEqual => ExpressionKind::Bool(lhs_val >= rhs_val),
                    Operator::LessThan => ExpressionKind::Bool(lhs_val < rhs_val),
                    Operator::LessThanOrEqual => ExpressionKind::Bool(lhs_val <= rhs_val),

                    // Other operations are not applicable to floats
                    _ => return_rule_error!(
                        self.location,
                        "Cannot perform operation {} on floats", 
                        op.to_str()
                    ), 
                }
            }

            // Integer operations
            (ExpressionKind::Int(lhs_val), ExpressionKind::Int(rhs_val)) => {
                match op {
                    Operator::Add => ExpressionKind::Int(lhs_val + rhs_val),
                    Operator::Subtract => ExpressionKind::Int(lhs_val - rhs_val),
                    Operator::Multiply => ExpressionKind::Int(lhs_val * rhs_val),
                    Operator::Divide => {
                        // Handle division by zero and integer division
                        if *rhs_val == 0 {
                            return return_rule_error!(
                                self.location,
                                "Cannot divide by zero"
                            )
                        } else {
                            ExpressionKind::Int(lhs_val / rhs_val)
                        }
                    }
                    Operator::Modulus => {
                        if *rhs_val == 0 {
                            return_rule_error!(
                                self.location,
                                "Cannot modulus by zero"
                            )
                        } else {
                            ExpressionKind::Int(lhs_val % rhs_val)
                        }
                    }
                    Operator::Exponent => {
                        // For integer exponentiation, we need to be careful with negative exponents
                        if *rhs_val < 0 {
                            // Convert to float for negative exponents
                            let lhs_float = *lhs_val as f64;
                            let rhs_float = *rhs_val as f64;
                            ExpressionKind::Float(lhs_float.powf(rhs_float))
                        } else {
                            // Use integer exponentiation for positive exponents
                            ExpressionKind::Int(lhs_val.pow(*rhs_val as u32))
                        }
                    }

                    // Logical operations with integer operands
                    Operator::Equality => ExpressionKind::Bool(lhs_val == rhs_val),
                    Operator::NotEqual => ExpressionKind::Bool(lhs_val != rhs_val),
                    Operator::GreaterThan => ExpressionKind::Bool(lhs_val > rhs_val),
                    Operator::GreaterThanOrEqual => ExpressionKind::Bool(lhs_val >= rhs_val),
                    Operator::LessThan => ExpressionKind::Bool(lhs_val < rhs_val),
                    Operator::LessThanOrEqual => ExpressionKind::Bool(lhs_val <= rhs_val),


                    _ => return_rule_error!(
                        self.location,
                        "Cannot perform operation {} on integers", 
                        op.to_str()
                    ), 
                }
            }

            // Boolean operations
            (ExpressionKind::Bool(lhs_val), ExpressionKind::Bool(rhs_val)) => {
                match op {
                    Operator::And => ExpressionKind::Bool(*lhs_val && *rhs_val),
                    Operator::Or => ExpressionKind::Bool(*lhs_val || *rhs_val),
                    Operator::Equality => ExpressionKind::Bool(lhs_val == rhs_val),
                    Operator::NotEqual => ExpressionKind::Bool(lhs_val != rhs_val),
                    
                    _ => return_rule_error!(
                        self.location,
                        "Cannot perform operation {} on booleans", 
                        op.to_str()
                    )
                }
            }

            // String operations
            (ExpressionKind::String(lhs_val), ExpressionKind::String(rhs_val)) => {
                match op {
                    Operator::Add => {
                        ExpressionKind::String(format!("{}{}", lhs_val, rhs_val))
                    }
                    Operator::Equality => ExpressionKind::Bool(lhs_val == rhs_val),
                    Operator::NotEqual => ExpressionKind::Bool(lhs_val != rhs_val),
                    _ => return_rule_error!(
                        self.location,
                        "Cannot perform operation {} on strings", 
                        op.to_str()
                    )
                }
            }

            // Any other combination of types
            _ => return Ok(None)
        };
        
        Ok(Some(Expression::new(kind, self.location)))
    }
}
#[derive(Debug, Clone, PartialEq)]
pub enum ExpressionKind {
    None,

    // For variables, function calls and structs / collection access
    // Name, DataType, Specific argument accessed
    // Arg accessed might be useful for built-in methods on any type
    Reference(String),

    Runtime(Vec<AstNode>),

    Int(i32),
    Float(f64),
    String(String),
    Bool(bool),

    // Because functions can all be values
    Function(
        Vec<Arg>,      // arguments
        Vec<AstNode>,  // body
        Vec<DataType>, // return args
    ),

    Scene(SceneContent, Style, String), // Scene Body, Styles, ID

    Collection(Vec<Expression>),

    Struct(Vec<Arg>),
}

impl ExpressionKind {
    pub fn get_function_nodes(&self) -> &[AstNode] {
        match self {
            ExpressionKind::Function(_, nodes, ..) => nodes,
            _ => &[],
        }
    }

    pub fn is_foldable(&self) -> bool {
        matches!(
            self,
            ExpressionKind::Int(_)
                | ExpressionKind::Float(_)
                | ExpressionKind::Bool(_)
                | ExpressionKind::String(_)
        )
    }

    pub fn get_type(&self) -> DataType {
        match self {
            ExpressionKind::None => DataType::None,
            ExpressionKind::Int(_) => DataType::Int(false),
            ExpressionKind::Float(_) => DataType::Float(false),
            ExpressionKind::String(_) => DataType::String(false),
            ExpressionKind::Bool(_) => DataType::Bool(false),
            ExpressionKind::Reference(_) => DataType::Inferred(false),
            ExpressionKind::Runtime(_) => DataType::Inferred(false),
            ExpressionKind::Function(args, _, returns) => {
                DataType::Function(args.to_owned(), returns.to_owned())
            }
            ExpressionKind::Collection(inner_nodes) => match inner_nodes.first() {
                Some(inner_node) => inner_node.data_type.to_owned(),
                None => DataType::Inferred(false),
            },
            ExpressionKind::Struct(args) => DataType::Args(args.to_owned()),
            ExpressionKind::Scene(..) => DataType::Inferred(false),
        }
    }

    // pub fn is_pure(&self) -> bool {
    //     match self {
    //         ExpressionKind::Runtime(..)
    //         | ExpressionKind::Reference(..)
    //         | ExpressionKind::Function(..) => false,
    //         ExpressionKind::Collection(values) => {
    //             for value in values {
    //                 if !value.is_pure() {
    //                     return false;
    //                 }
    //             }
    //             true
    //         }
    //         ExpressionKind::Object(args) => {
    //             for arg in args {
    //                 if !arg.value.kind.is_pure() {
    //                     return false;
    //                 }
    //             }
    //             true
    //         }
    //
    //         // Not sure about how to handle this yet
    //         ExpressionKind::Scene(..) => false,
    //         _ => true,
    //     }
    // }

    pub fn is_iterable(&self) -> bool {
        match self {
            ExpressionKind::Collection(..) => true,
            ExpressionKind::Int(_) => true,
            ExpressionKind::Float(_) => true,
            ExpressionKind::String(_) => true,
            _ => false,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Operator {
    Add,
    Subtract,
    Multiply,
    Divide,
    Modulus,
    // Remainder,
    Root,
    Exponent,

    // Logical
    And,
    Or,
    GreaterThan,
    GreaterThanOrEqual,
    LessThan,
    LessThanOrEqual,
    Equality,
    NotEqual,
}

impl Operator {
    pub fn to_str(&self) -> &str {
        match self { 
            Operator::Add => "+",
            Operator::Subtract => "-",
            Operator::Multiply => "*",
            Operator::Divide => "/",
            Operator::Modulus => "%",
            Operator::Root => "root",
            Operator::Exponent => "^",
            Operator::And => "and",
            Operator::Or => "or",
            Operator::GreaterThan => ">",
            Operator::GreaterThanOrEqual => ">=",
            Operator::LessThan => "<",
            Operator::LessThanOrEqual => "<=",
            Operator::Equality => "is",
            Operator::NotEqual => "is not",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum AssignmentOperator {
    Assign,
    AddAssign,
    SubtractAssign,
    MultiplyAssign,
    DivideAssign,
}

impl AssignmentOperator {
    pub fn to_string(&self) -> String {
        match self {
            AssignmentOperator::Assign => "=".to_string(),
            AssignmentOperator::AddAssign => "+=".to_string(),
            AssignmentOperator::SubtractAssign => "-=".to_string(),
            AssignmentOperator::MultiplyAssign => "*=".to_string(),
            AssignmentOperator::DivideAssign => "/=".to_string(),
        }
    }
}
