use crate::compiler::datatypes::{DataType, Ownership};
use crate::compiler::parsers::ast_nodes::{Arg, AstNode};
use crate::compiler::parsers::build_ast::AstBlock;
use crate::compiler::parsers::statements::create_template_node::Template;
use crate::compiler::parsers::tokens::TextLocation;

// Expressions represent anything that will turn into a value
// Their kind will represent what their value is.
// Runtime expressions (couldn't be folded) are represented as 'runtime' kinds.
// These runtime expressions are small ASTs that must be represented at runtime.
// Expression kinds are like a subset of the core datatypes because some data types don't return values or represent more complex structures.
#[derive(Clone, Debug)]
pub struct Expression {
    pub kind: ExpressionKind,
    pub data_type: DataType,
    pub ownership: Ownership,
    pub location: TextLocation,
}

impl Expression {
    pub fn as_string(&self) -> String {
        match &self.kind {
            ExpressionKind::StringSlice(string) => string.to_owned(),
            ExpressionKind::Int(int) => int.to_string(),
            ExpressionKind::Float(float) => float.to_string(),
            ExpressionKind::Bool(bool) => bool.to_string(),
            ExpressionKind::Reference(name) => name.to_string(),
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
            ExpressionKind::FunctionCall(..) => String::new(),
            ExpressionKind::Runtime(..) => String::new(),
            ExpressionKind::Range(lower, upper) => {
                format!("{} to {}", lower.as_string(), upper.as_string())
            }
            ExpressionKind::None => String::new(),
        }
    }

    pub fn new(
        kind: ExpressionKind,
        location: TextLocation,
        data_type: DataType,
        ownership: Ownership,
    ) -> Self {
        Self {
            data_type,
            kind,
            location,
            ownership,
        }
    }
    pub fn none() -> Self {
        Self {
            data_type: DataType::None,
            kind: ExpressionKind::None,
            location: TextLocation::default(),
            ownership: Ownership::default(),
        }
    }
    pub fn runtime(
        expressions: Vec<AstNode>,
        data_type: DataType,
        location: TextLocation,
        ownership: Ownership,
    ) -> Self {
        Self {
            data_type,
            kind: ExpressionKind::Runtime(expressions),
            location,
            ownership,
        }
    }
    pub fn int(value: i64, location: TextLocation, ownership: Ownership) -> Self {
        Self {
            data_type: DataType::Int,
            kind: ExpressionKind::Int(value),
            location,
            ownership,
        }
    }
    pub fn float(value: f64, location: TextLocation, ownership: Ownership) -> Self {
        Self {
            data_type: DataType::Float,
            kind: ExpressionKind::Float(value),
            location,
            ownership,
        }
    }
    pub fn string_slice(value: String, location: TextLocation, ownership: Ownership) -> Self {
        Self {
            data_type: DataType::String,
            kind: ExpressionKind::StringSlice(value),
            location,
            ownership,
        }
    }
    pub fn bool(value: bool, location: TextLocation, ownership: Ownership) -> Self {
        Self {
            data_type: DataType::Bool,
            kind: ExpressionKind::Bool(value),
            location,
            ownership,
        }
    }

    pub fn reference(
        name: String,
        data_type: DataType,
        location: TextLocation,
        ownership: Ownership,
    ) -> Self {
        Self {
            data_type,
            kind: ExpressionKind::Reference(name),
            location,
            ownership,
        }
    }

    // Creating Functions
    pub fn function(
        args: Vec<Arg>,
        body: AstBlock,
        return_types: Vec<Arg>,
        location: TextLocation,
    ) -> Self {
        Self {
            data_type: DataType::Function(args.to_owned(), return_types.to_owned()),
            kind: ExpressionKind::Function(args.to_owned(), body.ast, vec![]),
            location,
            ownership: Ownership::ImmutableReference,
        }
    }
    pub fn function_without_signature(body: AstBlock, location: TextLocation) -> Self {
        Self {
            data_type: DataType::Inferred,
            kind: ExpressionKind::Function(vec![], body.ast, vec![]),
            location,
            ownership: Ownership::ImmutableReference,
        }
    }
    pub fn function_without_return(
        args: Vec<Arg>,
        body: Vec<AstNode>,
        location: TextLocation,
    ) -> Self {
        Self {
            data_type: DataType::Inferred,
            kind: ExpressionKind::Function(args, body, vec![]),
            location,
            ownership: Ownership::ImmutableReference,
        }
    }
    pub fn function_without_args(
        body: Vec<AstNode>,
        returns: Vec<Arg>,
        location: TextLocation,
    ) -> Self {
        Self {
            data_type: DataType::Inferred,
            kind: ExpressionKind::Function(vec![], body, returns),
            location,
            ownership: Ownership::ImmutableReference,
        }
    }

    // Function calls
    pub fn function_call(
        name: String,
        args: Vec<Arg>,
        returns: Vec<Arg>,
        location: TextLocation,
    ) -> Self {
        let data_type = if returns.len() == 1 {
            returns[0].value.data_type.to_owned()
        } else {
            DataType::Args(returns)
        };

        Self {
            data_type,
            kind: ExpressionKind::FunctionCall(name, args),
            location,
            // TODO: Need to set the ownership based on the return signature
            ownership: Ownership::MutableOwned,
        }
    }

    pub fn collection(
        items: Vec<Expression>,
        location: TextLocation,
        ownership: Ownership,
    ) -> Self {
        Self {
            data_type: DataType::Inferred,
            kind: ExpressionKind::Collection(items),
            location,
            ownership,
        }
    }
    pub fn structure(args: Vec<Arg>, location: TextLocation, ownership: Ownership) -> Self {
        Self {
            data_type: DataType::Inferred,
            kind: ExpressionKind::Struct(args),
            location,
            ownership,
        }
    }
    pub fn template(template: Template, ownership: Ownership) -> Self {
        Self {
            data_type: DataType::Template,
            location: template.location.to_owned(),
            kind: ExpressionKind::Template(Box::new(template)),
            ownership,
        }
    }

    pub fn range(
        lower: Expression,
        upper: Expression,
        location: TextLocation,
        ownership: Ownership,
    ) -> Self {
        Self {
            data_type: DataType::Inferred,
            kind: ExpressionKind::Range(Box::new(lower), Box::new(upper)),
            location,
            ownership,
        }
    }

    pub fn parameter(
        name: String,
        data_type: DataType,
        location: TextLocation,
        ownership: Ownership,
    ) -> Self {
        Self {
            data_type,
            kind: ExpressionKind::Reference(name),
            location,
            ownership,
        }
    }

    pub fn is_none(&self) -> bool {
        matches!(self.kind, ExpressionKind::None)
    }
}
#[derive(Clone, Debug)]
pub enum ExpressionKind {
    None,

    Runtime(Vec<AstNode>),

    Int(i64),
    Float(f64),
    StringSlice(String),
    Bool(bool),

    // Reference to a variable by name
    Reference(String),

    // Because functions can all be values
    Function(
        Vec<Arg>,     // arguments (can be named)
        Vec<AstNode>, // body
        Vec<Arg>,     // return args (can be named)
    ),

    FunctionCall(
        String,   // Function name
        Vec<Arg>, // Arguments
    ),

    // Also equivalent to a String if it folds into a string
    Template(Box<Template>), // Template Body, Styles, ID

    Collection(Vec<Expression>),

    Struct(Vec<Arg>),

    // This is a special case for the range operator
    // This implementation will probably change in the future to be a more general operator
    // Upper and lower bounds are inclusive
    // Instead of making this a function; it has its own special case to make constant folding easier
    Range(Box<Expression>, Box<Expression>),
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
                | ExpressionKind::StringSlice(_)
        )
    }

    pub fn is_iterable(&self) -> bool {
        match self {
            ExpressionKind::Collection(..) => true,
            ExpressionKind::Int(_) => true,
            ExpressionKind::Float(_) => true,
            ExpressionKind::StringSlice(_) => true,
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
    Not,

    // Special
    Range,
}

impl Operator {
    pub fn required_values(&self) -> usize {
        match self {
            Operator::Add
            | Operator::Subtract
            | Operator::Multiply
            | Operator::Divide
            | Operator::Modulus
            | Operator::Root
            | Operator::Exponent
            | Operator::And
            | Operator::Or
            | Operator::GreaterThan
            | Operator::GreaterThanOrEqual
            | Operator::LessThan
            | Operator::LessThanOrEqual
            | Operator::Range
            | Operator::Equality => 2,

            // Not is a unary operator
            _ => 1,
        }
    }
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
            Operator::Not => "not",
            Operator::Range => "..",
        }
    }
}
