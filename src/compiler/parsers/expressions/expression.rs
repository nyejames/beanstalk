use crate::compiler::datatypes::{DataType, Ownership};
use crate::compiler::host_functions::registry::HostFunctionId;
use crate::compiler::parsers::ast_nodes::{AstNode, Var};
use crate::compiler::parsers::statements::create_template_node::Template;
use crate::compiler::parsers::statements::functions::FunctionSignature;
use crate::compiler::parsers::tokenizer::tokens::TextLocation;
use crate::compiler::string_interning::{InternedString, StringTable};

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
    pub fn as_string(&self, string_table: &StringTable) -> String {
        match &self.kind {
            ExpressionKind::StringSlice(interned_string) => {
                string_table.resolve(*interned_string).to_owned()
            }
            ExpressionKind::Int(int) => int.to_string(),
            ExpressionKind::Float(float) => float.to_string(),
            ExpressionKind::Bool(bool) => bool.to_string(),
            ExpressionKind::Char(char) => char.to_string(),
            ExpressionKind::Reference(interned_name) => {
                string_table.resolve(*interned_name).to_string()
            }
            ExpressionKind::Template(..) => String::new(),
            ExpressionKind::Collection(items, ..) => {
                let mut all_items = String::new();
                for item in items {
                    all_items.push_str(&item.as_string(string_table));
                }
                all_items
            }
            ExpressionKind::StructInstance(args) | ExpressionKind::StructDefinition(args) => {
                let mut all_items = String::new();
                for arg in args {
                    all_items.push_str(&arg.value.as_string(string_table));
                }
                all_items
            }
            ExpressionKind::Function(..) => String::new(),
            ExpressionKind::FunctionCall(..) => String::new(),
            ExpressionKind::HostFunctionCall(..) => String::new(),
            ExpressionKind::Runtime(..) => String::new(),
            ExpressionKind::Range(lower, upper) => {
                format!(
                    "{} to {}",
                    lower.as_string(string_table),
                    upper.as_string(string_table)
                )
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
    pub fn string_slice(
        value: InternedString,
        location: TextLocation,
        ownership: Ownership,
    ) -> Self {
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
    pub fn char(value: char, location: TextLocation, ownership: Ownership) -> Self {
        Self {
            data_type: DataType::Char,
            kind: ExpressionKind::Char(value),
            location,
            ownership,
        }
    }

    pub fn reference(arg: &Var) -> Self {
        Self {
            data_type: arg.value.data_type.clone(),
            kind: ExpressionKind::Reference(arg.id),
            location: arg.value.location.to_owned(),
            ownership: arg.value.ownership.clone(),
        }
    }

    // Creating Functions
    pub fn function(
        receiver: Option<DataType>,
        signature: FunctionSignature,
        body: Vec<AstNode>,
        location: TextLocation,
    ) -> Self {
        Self {
            data_type: DataType::Function(Box::new(receiver), signature.to_owned()),
            kind: ExpressionKind::Function(signature, body),
            location,
            ownership: Ownership::ImmutableReference,
        }
    }

    pub fn function_without_signature(body: Vec<AstNode>, location: TextLocation) -> Self {
        Self {
            data_type: DataType::Inferred,
            kind: ExpressionKind::Function(
                FunctionSignature {
                    parameters: vec![],
                    returns: vec![],
                },
                body,
            ),
            location,
            ownership: Ownership::ImmutableReference,
        }
    }
    pub fn function_without_return(
        args: Vec<Var>,
        body: Vec<AstNode>,
        location: TextLocation,
    ) -> Self {
        let signature = FunctionSignature {
            parameters: args,
            returns: vec![],
        };
        Self {
            data_type: DataType::Inferred,
            kind: ExpressionKind::Function(signature, body),
            location,
            ownership: Ownership::ImmutableReference,
        }
    }
    pub fn function_without_args(
        body: Vec<AstNode>,
        returns: Vec<DataType>,
        location: TextLocation,
    ) -> Self {
        let signature = FunctionSignature {
            parameters: vec![],
            returns,
        };
        Self {
            data_type: DataType::Inferred,
            kind: ExpressionKind::Function(signature, body),
            location,
            ownership: Ownership::ImmutableReference,
        }
    }

    // Function calls
    pub fn function_call(
        name: InternedString,
        args: Vec<Expression>,
        returns: Vec<DataType>,
        location: TextLocation,
    ) -> Self {
        Self {
            data_type: DataType::Returns(returns),
            kind: ExpressionKind::FunctionCall(name, args),
            location,
            // TODO: Need to set the ownership based on the return signature
            ownership: Ownership::MutableOwned,
        }
    }

    pub fn host_function_call(
        host_function_id: HostFunctionId,
        args: Vec<Expression>,
        returns: Vec<DataType>,
        location: TextLocation,
    ) -> Self {
        Self {
            data_type: DataType::Returns(returns),
            kind: ExpressionKind::HostFunctionCall(host_function_id, args),
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
    pub fn struct_instance(args: Vec<Var>, location: TextLocation, ownership: Ownership) -> Self {
        Self {
            data_type: DataType::Inferred,
            kind: ExpressionKind::StructInstance(args),
            location,
            ownership,
        }
    }
    pub fn struct_definition(args: Vec<Var>, location: TextLocation, ownership: Ownership) -> Self {
        Self {
            data_type: DataType::Inferred,
            kind: ExpressionKind::StructDefinition(args),
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
        name: InternedString,
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

    pub fn is_constant(&self) -> bool {
        !self.ownership.is_mutable() && self.kind.is_foldable()
    }
}
#[derive(Clone, Debug)]
pub enum ExpressionKind {
    None,

    Runtime(Vec<AstNode>),

    Int(i64),
    Float(f64),
    StringSlice(InternedString),
    Bool(bool),
    Char(char),

    // Reference to a variable by name
    Reference(InternedString),

    // Because functions can all be values
    Function(
        FunctionSignature,
        Vec<AstNode>, // body
    ),

    FunctionCall(
        InternedString,  // Function name
        Vec<Expression>, // Arguments
    ),

    HostFunctionCall(HostFunctionId, Vec<Expression>),

    // Also equivalent to a String if it folds into a string
    Template(Box<Template>), // Template Body, Styles, ID

    Collection(Vec<Expression>),

    StructDefinition(Vec<Var>),
    StructInstance(Vec<Var>),

    // This is a special case for the range operator
    // This implementation will probably change in the future to be a more general operator
    // Upper and lower bounds are inclusive,
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
