use crate::bs_types::DataType;
use crate::parsers::scene::{SceneContent, Style};
use crate::tokenizer::TokenPosition;
use crate::tokens::{string_dimensions, VarVisibility};
use colour::red_ln;
use std::path::PathBuf;
use wasm_encoder::ValType;

#[derive(Debug, PartialEq, Clone)]
// Args are abstractions on top of Datatypes
// They are used to store the name, data type and optional value of an argument
// These are used for structs and functions
// Args should basically disappear once the AST is parsed. Everything will be converted into just indexes
pub struct Arg {
    pub name: String, // Optional Name of the argument (empty string if unnamed)
    pub data_type: DataType,
    pub default_value: Expr, // Optional Value of the argument - 'None' if no value
}

impl Arg {
    pub fn to_wasm_type(&self) -> Vec<ValType> {
        match &self.data_type {
            DataType::Float(_) => vec![ValType::F64],
            DataType::Int(_)
            | DataType::Bool(_)
            | DataType::None
            | DataType::True
            | DataType::False => {
                vec![ValType::I32]
            }

            DataType::Decimal(_) => vec![ValType::F64],

            DataType::String(_) => vec![ValType::I32, ValType::I32],
            DataType::CoerceToString(_) => vec![ValType::I32, ValType::I32],

            DataType::Object(args) => args
                .iter()
                .flat_map(|arg| arg.to_wasm_type())
                .collect::<Vec<ValType>>(),

            DataType::Collection(_) => vec![ValType::I32, ValType::I32],

            _ => vec![ValType::I32],
        }
    }
}

// The possible values of any type.
// Return 'Runtime' if the value is not known at compile time
#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    None,

    // For variables, function calls and structs / collection access
    // Name, DataType, Specific argument accessed
    // Arg accessed might be useful for built-in methods on any type
    Reference(String, DataType),

    Runtime(Vec<AstNode>, DataType),

    Int(i32),
    Float(f64),
    String(String),
    Bool(bool),

    // Because blocks (functions / classes) can all be values
    Block(
        Vec<Arg>,      // arguments
        Vec<AstNode>,  // body
        Vec<DataType>, // return args
    ),

    Scene(SceneContent, Style, String), // Scene Body, Styles, Scene head, ID

    Collection(Vec<Expr>, DataType),

    Args(Vec<Arg>),
}

impl Expr {
    // Evaluates a binary operation between two expressions based on the operator
    // This helps with constant folding by handling type-specific operations
    pub fn evaluate_operator(&self, rhs: &Expr, op: &Operator) -> Option<Expr> {
        match (self, rhs) {
            // Float operations
            (Expr::Float(lhs_val), Expr::Float(rhs_val)) => {
                match op {
                    Operator::Add => Some(Expr::Float(lhs_val + rhs_val)),
                    Operator::Subtract => Some(Expr::Float(lhs_val - rhs_val)),
                    Operator::Multiply => Some(Expr::Float(lhs_val * rhs_val)),
                    Operator::Divide => Some(Expr::Float(lhs_val / rhs_val)),
                    Operator::Modulus => Some(Expr::Float(lhs_val % rhs_val)),
                    Operator::Exponent => Some(Expr::Float(lhs_val.powf(*rhs_val))),

                    // Logical operations with float operands
                    Operator::Equality => Some(Expr::Bool(lhs_val == rhs_val)),
                    Operator::NotEqual => Some(Expr::Bool(lhs_val != rhs_val)),
                    Operator::GreaterThan => Some(Expr::Bool(lhs_val > rhs_val)),
                    Operator::GreaterThanOrEqual => Some(Expr::Bool(lhs_val >= rhs_val)),
                    Operator::LessThan => Some(Expr::Bool(lhs_val < rhs_val)),
                    Operator::LessThanOrEqual => Some(Expr::Bool(lhs_val <= rhs_val)),
                    _ => None, // Other operations are not applicable to floats
                }
            }

            // Integer operations
            (Expr::Int(lhs_val), Expr::Int(rhs_val)) => {
                match op {
                    Operator::Add => Some(Expr::Int(lhs_val + rhs_val)),
                    Operator::Subtract => Some(Expr::Int(lhs_val - rhs_val)),
                    Operator::Multiply => Some(Expr::Int(lhs_val * rhs_val)),
                    Operator::Divide => {
                        // Handle division by zero and integer division
                        if *rhs_val == 0 {
                            None
                        } else {
                            Some(Expr::Int(lhs_val / rhs_val))
                        }
                    }
                    Operator::Modulus => {
                        if *rhs_val == 0 {
                            None
                        } else {
                            Some(Expr::Int(lhs_val % rhs_val))
                        }
                    }
                    Operator::Exponent => {
                        // For integer exponentiation, we need to be careful with negative exponents
                        if *rhs_val < 0 {
                            // Convert to float for negative exponents
                            let lhs_float = *lhs_val as f64;
                            let rhs_float = *rhs_val as f64;
                            Some(Expr::Float(lhs_float.powf(rhs_float)))
                        } else {
                            // Use integer exponentiation for positive exponents
                            Some(Expr::Int(lhs_val.pow(*rhs_val as u32)))
                        }
                    }

                    // Logical operations with integer operands
                    Operator::Equality => Some(Expr::Bool(lhs_val == rhs_val)),
                    Operator::NotEqual => Some(Expr::Bool(lhs_val != rhs_val)),
                    Operator::GreaterThan => Some(Expr::Bool(lhs_val > rhs_val)),
                    Operator::GreaterThanOrEqual => Some(Expr::Bool(lhs_val >= rhs_val)),
                    Operator::LessThan => Some(Expr::Bool(lhs_val < rhs_val)),
                    Operator::LessThanOrEqual => Some(Expr::Bool(lhs_val <= rhs_val)),
                    _ => None, // Other operations not applicable to integers
                }
            }

            // Boolean operations
            (Expr::Bool(lhs_val), Expr::Bool(rhs_val)) => {
                match op {
                    Operator::And => Some(Expr::Bool(*lhs_val && *rhs_val)),
                    Operator::Or => Some(Expr::Bool(*lhs_val || *rhs_val)),
                    Operator::Equality => Some(Expr::Bool(lhs_val == rhs_val)),
                    Operator::NotEqual => Some(Expr::Bool(lhs_val != rhs_val)),
                    _ => None, // Other operations not applicable to booleans
                }
            }

            // String operations
            (Expr::String(lhs_val), Expr::String(rhs_val)) => {
                match op {
                    Operator::Add => Some(Expr::String(format!("{}{}", lhs_val, rhs_val))),
                    Operator::Equality => Some(Expr::Bool(lhs_val == rhs_val)),
                    Operator::NotEqual => Some(Expr::Bool(lhs_val != rhs_val)),
                    _ => None, // Other operations are not applicable to strings
                }
            }

            // Any other combination of types
            _ => None,
        }
    }
    pub fn get_block_nodes(&self) -> &[AstNode] {
        match self {
            Expr::Block(_, nodes, ..) => nodes,
            _ => &[],
        }
    }
    pub fn is_foldable(&self) -> bool {
        matches!(
            self,
            Expr::Int(_) | Expr::Float(_) | Expr::Bool(_) | Expr::String(_)
        )
    }

    pub fn is_collection(&self) -> bool {
        match self {
            Expr::Collection(..) => true,
            Expr::Reference(_, data_type, ..) => matches!(data_type, DataType::Collection(_)),
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

#[derive(Clone, Debug, PartialEq)]
pub enum AssignmentOperator {
    Assign,
    AddAssign,
    SubtractAssign,
    MultiplyAssign,
    DivideAssign,
}

impl AssignmentOperator {
    pub fn to_js(&self) -> String {
        match self {
            AssignmentOperator::Assign => "=".to_string(),
            AssignmentOperator::AddAssign => "+=".to_string(),
            AssignmentOperator::SubtractAssign => "-=".to_string(),
            AssignmentOperator::MultiplyAssign => "*=".to_string(),
            AssignmentOperator::DivideAssign => "/=".to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum AstNode {
    // Warning Message
    // This could be stuff like unused variables, possible race conditions, etc
    Warning(String, TokenPosition), // Message, Line number, Start pos, End pos

    // Config settings
    Settings(Vec<Arg>, TokenPosition), // Settings, Line number

    // Named an import path for the module
    // Import(String, TokenPosition), // Path, Line number

    // Path to a module that will automatically import all styles and scenes
    // into the scope of the current module. Doesn't automatically import variables or functions into the scope
    Use(PathBuf, TokenPosition), // Path, Line number

    // Control Flow
    Access,
    Return(Vec<Expr>, TokenPosition),  // Return value, Line number
    If(Expr, Expr, TokenPosition),     // Condition, If true, Line number
    Else(Vec<AstNode>, TokenPosition), // Body, Line number

    ForLoop(Box<Arg>, Expr, Expr, TokenPosition), // Item, Collection, Body, Line number
    WhileLoop(Expr, Expr, TokenPosition),         // Condition, Body, Line number

    // Basics
    FunctionCall(
        String,
        Vec<Expr>,     // Arguments passed in
        Vec<DataType>, // return types
        TokenPosition,
        // bool, // Function is pure
    ),

    Comment(String),

    // Variable names should be the full namespace (module path + variable name)
    Declaration(String, Expr, VarVisibility, DataType, TokenPosition), // Variable name, Value, Visibility, Type, Line number

    // Built-in Functions (Would probably be standard lib in other languages)
    // Print can accept multiple arguments and will coerce them to strings
    Print(Expr, TokenPosition), // Value, Line number

    // Not even sure if this is needed
    JSStringReference(String, TokenPosition), // Variable name, Line number

    // Other language code blocks
    JS(String, TokenPosition),  // Code, Line number
    Css(String, TokenPosition), // Code, Line number
    // Wasm(String, TokenPosition), // Code, Line number

    // Literals
    Reference(Expr, TokenPosition),  // Token, Line number
    Expression(Expr, TokenPosition), // Token, Line number
    
    // Name, Assignment operator, Value, Line number
    Mutation(String, AssignmentOperator, Expr, TokenPosition),

    SceneTemplate,
    Slot,
    Empty(TokenPosition), // Line number

    // Operators
    // Operator, Precedence
    Operator(Operator, TokenPosition), // Operator, Line number
    // UnaryOperator(Token, bool, TokenPosition), // Operator, is_postfix, Line number
    Newline,
    Spaces(u32),
}

impl AstNode {
    pub fn get_type(&self) -> DataType {
        match self {
            AstNode::Reference(value, _) => match value {
                Expr::Float(_) => DataType::Float(false),
                Expr::Int(_) => DataType::Int(false),
                Expr::String(_) => DataType::String(false),
                Expr::Bool(value) => {
                    if *value {
                        DataType::True
                    } else {
                        DataType::False
                    }
                }

                Expr::Scene(..) => DataType::Scene(false),
                Expr::Collection(_, data_type) => data_type.to_owned(),
                Expr::Args(args) => {
                    let mut data_type = DataType::Inferred(false);
                    for arg in args {
                        data_type = arg.data_type.to_owned();
                    }
                    data_type
                }
                Expr::Reference(_, data_type) => {
                    data_type.to_owned()
                }
                Expr::Block(args, _, return_types, ..) => {
                    DataType::Block(args.to_owned(), return_types.to_owned())
                }

                Expr::Runtime(_, data_type) => data_type.to_owned(),
                Expr::None => DataType::None,
            },

            AstNode::Empty(_) => DataType::None,
            AstNode::Declaration(_, _, _, data_type, ..) => data_type.to_owned(),

            AstNode::FunctionCall(_, _, return_types, ..) => DataType::Object(
                return_types
                    .iter()
                    .map(|t| Arg {
                        name: String::new(),
                        data_type: t.to_owned(),
                        default_value: Expr::None,
                    })
                    .collect(),
            ),

            _ => {
                red_ln!(
                    "Probably compiler issue?: Datatype return not implemented for: {:?}",
                    self
                );

                DataType::Inferred(false)
            }
        }
    }

    pub(crate) fn get_expr(&self) -> Expr {
        match self {
            AstNode::Reference(value, ..) | AstNode::Declaration(_, value, ..) => value.to_owned(),
            _ => Expr::Runtime(Vec::from([self.to_owned()]), self.get_type()),
        }
    }

    pub fn get_precedence(&self) -> u32 {
        match self {
            AstNode::Operator(op, _) => match op {
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
        !matches!(self, AstNode::Operator(Operator::Exponent, ..))
    }

    pub fn dimensions(&self) -> TokenPosition {
        match self {
            AstNode::Reference(value, _) => value.dimensions(),

            AstNode::Declaration(name, _, _, _, token_position) => TokenPosition {
                line_number: token_position.char_column + name.to_string().len() as i32,
                char_column: token_position.line_number,
            },

            _ => TokenPosition {
                line_number: 0,
                char_column: 0,
            },
        }
    }

    pub fn get_position(&self) -> TokenPosition {
        match self {
            AstNode::Reference(.., position) => position.to_owned(),
            AstNode::Declaration(.., position) => position.to_owned(),
            AstNode::Operator(.., position) => position.to_owned(),
            AstNode::WhileLoop(.., position) => position.to_owned(),
            AstNode::ForLoop(.., position) => position.to_owned(),
            AstNode::If(.., position) => position.to_owned(),
            AstNode::Else(.., position) => position.to_owned(),
            AstNode::Return(.., position) => position.to_owned(),
            AstNode::JS(.., position) => position.to_owned(),
            AstNode::Css(.., position) => position.to_owned(),
            AstNode::Use(.., position) => position.to_owned(),
            AstNode::Settings(.., position) => position.to_owned(),
            AstNode::Expression(.., position) => position.to_owned(),
            AstNode::Mutation(.., position) => position.to_owned(),
            AstNode::Print(.., position) => position.to_owned(),
            AstNode::Warning(.., position) => position.to_owned(),
            _ => TokenPosition {
                line_number: 0,
                char_column: 0,
            },
        }
    }
}

impl Expr {
    pub fn get_type(&self, is_mutable: bool) -> DataType {
        match self {
            Expr::None => DataType::None,
            Expr::Runtime(_, data_type) => data_type.to_owned(),
            Expr::Int(_) => DataType::Int(is_mutable),
            Expr::Float(_) => DataType::Float(is_mutable),
            Expr::String(_) => DataType::String(is_mutable),
            Expr::Bool(_) => DataType::Bool(is_mutable),
            Expr::Scene(..) => DataType::Scene(is_mutable),
            Expr::Collection(_, data_type) => data_type.to_owned(),
            Expr::Args(args) => DataType::Object(args.to_owned()),
            Expr::Block(args, _, return_type, ..) => {
                DataType::Block(args.to_owned(), return_type.to_owned())
            }
            // Need to check accessed args
            Expr::Reference(_, data_type) => {
                data_type.to_owned()
            }
        }
    }

    pub fn as_string(&self) -> String {
        match self {
            Expr::String(string) => string.to_owned(),
            Expr::Int(int) => int.to_string(),
            Expr::Float(float) => float.to_string(),
            Expr::Bool(bool) => bool.to_string(),
            Expr::Scene(..) => String::new(),
            Expr::Collection(items, ..) => {
                let mut all_items = String::new();
                for item in items {
                    all_items.push_str(&item.as_string());
                }
                all_items
            }
            Expr::Args(args) => {
                let mut all_items = String::new();
                for arg in args {
                    all_items.push_str(&arg.default_value.as_string());
                }
                all_items
            }
            Expr::Block(..) => String::new(),
            Expr::Reference(..) => String::new(),
            Expr::Runtime(..) => String::new(),
            Expr::None => String::new(),
        }
    }

    pub fn is_pure(&self) -> bool {
        match self {
            Expr::Runtime(..) | Expr::Reference(..) | Expr::Block(..) => false,
            Expr::Collection(values, _) => {
                for value in values {
                    if !value.is_pure() {
                        return false;
                    }
                }
                true
            }
            Expr::Args(args) => {
                for arg in args {
                    if !arg.default_value.is_pure() {
                        return false;
                    }
                }
                true
            }

            // Not sure about how to handle this yet
            Expr::Scene(..) => false,
            _ => true,
        }
    }

    pub fn dimensions(&self) -> TokenPosition {
        match self {
            Expr::None => TokenPosition {
                line_number: 0,
                char_column: 0,
            },

            Expr::Int(val) => TokenPosition {
                line_number: 0,
                char_column: val.to_string().len() as i32,
            },

            Expr::Float(val) => TokenPosition {
                line_number: 0,
                char_column: val.to_string().len() as i32,
            },

            Expr::String(val) => string_dimensions(val),

            Expr::Bool(val) => {
                if *val {
                    TokenPosition {
                        line_number: 4,
                        char_column: 0,
                    }
                } else {
                    TokenPosition {
                        line_number: 5,
                        char_column: 0,
                    }
                }
            }

            Expr::Reference(name, ..) => TokenPosition {
                line_number: 0,
                char_column: name.len() as i32,
            },

            Expr::Block(_, nodes, ..) => {
                let mut combined_dimensions = TokenPosition::default();

                // Get the largest dimensions of all the nodes
                for node in nodes {
                    if node.dimensions().line_number > combined_dimensions.line_number {
                        combined_dimensions.line_number = node.dimensions().line_number;
                    }

                    if node.dimensions().char_column > combined_dimensions.char_column {
                        combined_dimensions.char_column = node.dimensions().char_column;
                    }
                }

                combined_dimensions
            }

            // Get the position of the first node, and the last node
            // And get the positions
            // This just gets the widest char line
            // So error formatting will need to clip the line to each length
            Expr::Scene(nodes, ..) => {
                let first_node = &nodes.before.first().unwrap_or(&Expr::None);
                let last_node = &nodes.after.last().unwrap_or(&Expr::None);
                TokenPosition {
                    line_number: last_node.dimensions().line_number,
                    char_column: last_node.dimensions().char_column
                        - first_node.dimensions().char_column,
                }
            }

            Expr::Runtime(nodes, ..) => {
                let first_node = &nodes[0];
                let last_node = &nodes[nodes.len() - 1];
                TokenPosition {
                    line_number: last_node.dimensions().line_number,
                    char_column: last_node.dimensions().char_column
                        - first_node.dimensions().char_column,
                }
            }

            Expr::Collection(nodes, ..) => {
                let first_node = &nodes[0];
                let last_node = &nodes[nodes.len() - 1];
                TokenPosition {
                    line_number: last_node.dimensions().line_number,
                    char_column: last_node.dimensions().char_column
                        - first_node.dimensions().char_column,
                }
            }

            Expr::Args(args) => {
                let mut combined_dimensions = TokenPosition {
                    line_number: args[0].default_value.dimensions().line_number,
                    char_column: args[0].default_value.dimensions().char_column,
                };

                for arg in args {
                    combined_dimensions.char_column += arg.default_value.dimensions().char_column;
                }

                combined_dimensions
            }
        }
    }

    pub fn is_iterable(&self) -> bool {
        match self {
            Expr::Collection(..) => true,
            Expr::Int(_) => true,
            Expr::Float(_) => true,
            Expr::Runtime(_, data_type) => data_type.is_iterable(),
            Expr::String(_) => true,
            _ => false,
        }
    }
}
