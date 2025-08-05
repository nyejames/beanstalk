use crate::compiler::compiler_errors::CompileError;
use crate::compiler::compiler_errors::ErrorType;
use crate::compiler::datatypes::{DataType, Ownership};
use crate::compiler::parsers::ast_nodes::{Arg, AstNode};
use crate::compiler::parsers::build_ast::AstBlock;
use crate::compiler::parsers::statements::create_template_node::Template;
use crate::compiler::parsers::template::{Style, TemplateContent};
use crate::compiler::parsers::tokens::TextLocation;
use crate::return_rule_error;

// Expressions represent anything that will turn into a value
// Their kind will represent what their value is.
// Runtime expressions (couldn't be folded) are represented as 'runtime' kinds.
// These runtime expressions are small ASTs that must be represented at runtime.
// Expression kinds are like a subset of the core datatypes, because some data types don't return values or represent more complex structures.
#[derive(Debug, Clone, PartialEq)]
pub struct Expression {
    pub kind: ExpressionKind,
    pub data_type: DataType,
    pub owner_id: u32,
    pub location: TextLocation,
}

impl Expression {
    pub fn as_string(&self) -> String {
        match &self.kind {
            ExpressionKind::String(string) => string.to_owned(),
            ExpressionKind::Int(int) => int.to_string(),
            ExpressionKind::Float(float) => float.to_string(),
            ExpressionKind::Bool(bool) => bool.to_string(),
            ExpressionKind::Template(..) => String::new(),
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
            ExpressionKind::Runtime(..) => String::new(),
            ExpressionKind::None => String::new(),
        }
    }

    pub fn new(
        kind: ExpressionKind,
        location: TextLocation,
        data_type: DataType,
        owner_id: u32,
    ) -> Self {
        Self {
            data_type,
            kind,
            location,
            owner_id,
        }
    }
    pub fn none() -> Self {
        Self {
            data_type: DataType::None,
            kind: ExpressionKind::None,
            location: TextLocation::default(),
            owner_id: 0, // TBH don't know how this should behave yet
        }
    }
    pub fn runtime(
        expressions: Vec<AstNode>,
        data_type: DataType,
        location: TextLocation,
        owner_id: u32,
    ) -> Self {
        Self {
            data_type,
            kind: ExpressionKind::Runtime(expressions),
            location,
            owner_id,
        }
    }
    pub fn int(value: i32, location: TextLocation, owner_id: u32) -> Self {
        Self {
            data_type: DataType::Int(Ownership::default()),
            kind: ExpressionKind::Int(value),
            location,
            owner_id,
        }
    }
    pub fn float(value: f64, location: TextLocation, owner_id: u32) -> Self {
        Self {
            data_type: DataType::Float(Ownership::default()),
            kind: ExpressionKind::Float(value),
            location,
            owner_id,
        }
    }
    pub fn string(value: String, location: TextLocation, owner_id: u32) -> Self {
        Self {
            data_type: DataType::String(Ownership::default()),
            kind: ExpressionKind::String(value),
            location,
            owner_id,
        }
    }
    pub fn bool(value: bool, location: TextLocation, owner_id: u32) -> Self {
        Self {
            data_type: DataType::Bool(Ownership::default()),
            kind: ExpressionKind::Bool(value),
            location,
            owner_id,
        }
    }

    // Creating Functions
    pub fn function(
        owner_id: u32,
        args: Vec<Arg>,
        body: AstBlock,
        return_types: Vec<DataType>,
        location: TextLocation,
    ) -> Self {
        Self {
            data_type: DataType::Function(args.to_owned(), return_types.to_owned()),
            kind: ExpressionKind::Function(args.to_owned(), body.ast, vec![]),
            location,
            owner_id,
        }
    }
    pub fn function_without_signature(
        owner_id: u32,
        body: AstBlock,
        location: TextLocation,
    ) -> Self {
        Self {
            data_type: DataType::Inferred(Ownership::default()),
            kind: ExpressionKind::Function(vec![], body.ast, vec![]),
            location,
            owner_id,
        }
    }
    pub fn function_without_return(
        args: Vec<Arg>,
        body: Vec<AstNode>,
        location: TextLocation,
        owner_id: u32,
    ) -> Self {
        Self {
            data_type: DataType::Inferred(Ownership::default()),
            kind: ExpressionKind::Function(args, body, vec![]),
            location,
            owner_id,
        }
    }
    pub fn function_without_args(
        body: Vec<AstNode>,
        return_types: Vec<DataType>,
        location: TextLocation,
        owner_id: u32,
    ) -> Self {
        Self {
            data_type: DataType::Inferred(Ownership::ImmutableReference),
            kind: ExpressionKind::Function(vec![], body, return_types),
            location,
            owner_id,
        }
    }

    pub fn collection(items: Vec<Expression>, location: TextLocation, owner_id: u32) -> Self {
        Self {
            data_type: DataType::Inferred(Ownership::default()),
            kind: ExpressionKind::Collection(items),
            location,
            owner_id,
        }
    }
    pub fn structure(args: Vec<Arg>, location: TextLocation, owner_id: u32) -> Self {
        Self {
            data_type: DataType::Inferred(Ownership::default()),
            kind: ExpressionKind::Struct(args),
            location,
            owner_id,
        }
    }
    pub fn template(template: Template, lifetime: u32) -> Self {
        Self {
            data_type: DataType::Template(Ownership::default()),
            kind: ExpressionKind::Template(template.content, template.style, template.id),
            location: template.location,
            owner_id: lifetime,
        }
    }

    pub fn is_none(&self) -> bool {
        matches!(self.kind, ExpressionKind::None)
    }

    // Evaluates a binary operation between two expressions based on the operator
    // This helps with constant folding by handling type-specific operations
    pub fn evaluate_operator(
        &self,
        rhs: &Expression,
        op: &Operator,
    ) -> Result<Option<Expression>, CompileError> {
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
                        self.location.to_owned(),
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
                            return_rule_error!(self.location.to_owned(), "Cannot divide by zero")
                        }

                        ExpressionKind::Int(lhs_val / rhs_val)
                    }
                    Operator::Modulus => {
                        if *rhs_val == 0 {
                            return_rule_error!(self.location.to_owned(), "Cannot modulus by zero")
                        }

                        ExpressionKind::Int(lhs_val % rhs_val)
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
                        self.location.to_owned(),
                        "Cannot perform operation {} on integers",
                        op.to_str()
                    ),
                }
            }

            // Boolean operations
            (ExpressionKind::Bool(lhs_val), ExpressionKind::Bool(rhs_val)) => match op {
                Operator::And => ExpressionKind::Bool(*lhs_val && *rhs_val),
                Operator::Or => ExpressionKind::Bool(*lhs_val || *rhs_val),
                Operator::Equality => ExpressionKind::Bool(lhs_val == rhs_val),
                Operator::NotEqual => ExpressionKind::Bool(lhs_val != rhs_val),

                _ => return_rule_error!(
                    self.location.to_owned(),
                    "Cannot perform operation {} on booleans",
                    op.to_str()
                ),
            },

            // String operations
            (ExpressionKind::String(lhs_val), ExpressionKind::String(rhs_val)) => match op {
                Operator::Add => ExpressionKind::String(format!("{}{}", lhs_val, rhs_val)),
                Operator::Equality => ExpressionKind::Bool(lhs_val == rhs_val),
                Operator::NotEqual => ExpressionKind::Bool(lhs_val != rhs_val),
                _ => return_rule_error!(
                    self.location.to_owned(),
                    "Cannot perform operation {} on strings",
                    op.to_str()
                ),
            },

            // Any other combination of types
            _ => return Ok(None),
        };

        Ok(Some(Expression::new(
            kind,
            self.location.to_owned(),
            self.data_type.to_owned(),
            self.owner_id,
        )))
    }
}
#[derive(Debug, Clone, PartialEq)]
pub enum ExpressionKind {
    None,

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

    Template(TemplateContent, Style, String), // Template Body, Styles, ID

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
