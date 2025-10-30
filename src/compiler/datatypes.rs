use crate::compiler::parsers::ast_nodes::Arg;
use crate::compiler::parsers::expressions::expression::{Expression, ExpressionKind};
use crate::compiler::parsers::statements::functions::FunctionSignature;
use std::fmt::Display;
use crate::compiler::parsers::tokenizer::tokens::TextLocation;

#[derive(Debug, Clone, PartialEq)]
pub enum Ownership {
    MutableOwned,
    MutableReference,
    ImmutableOwned,
    ImmutableReference,
}

impl Ownership {
    pub fn default() -> Ownership {
        Ownership::ImmutableOwned
    }
    pub fn is_mutable(&self) -> bool {
        match &self {
            Ownership::MutableOwned => true,
            Ownership::MutableReference => true,
            _ => false,
        }
    }
    pub fn is_reference(&self) -> bool {
        match &self {
            Ownership::MutableReference => true,
            Ownership::ImmutableReference => true,
            _ => false,
        }
    }

    pub fn to_reference(&mut self) {
        match self {
            Ownership::MutableOwned => {
                *self = Ownership::MutableReference;
            }
            Ownership::ImmutableOwned => {
                *self = Ownership::ImmutableReference;
            }
            _ => {}
        }
    }

    pub fn get_owned(&self) -> Ownership {
        match self {
            Ownership::MutableReference => Ownership::MutableOwned,
            Ownership::ImmutableReference => Ownership::ImmutableOwned,
            _ => self.to_owned(),
        }
    }

    pub fn get_reference(&self) -> Ownership {
        match self {
            Ownership::MutableOwned => Ownership::MutableReference,
            Ownership::ImmutableOwned => Ownership::ImmutableReference,
            _ => self.to_owned(),
        }
    }

    pub fn as_string(&self) -> String {
        match &self {
            Ownership::MutableOwned => String::from("mutable"),
            Ownership::MutableReference => String::from("mutable reference"),
            Ownership::ImmutableOwned => String::from("immutable"),
            Ownership::ImmutableReference => String::from("immutable reference"),
        }
    }
}

#[derive(Debug, Clone)]
pub enum DataType {
    // Mutability is part of the type
    // This helps with compile time constant folding

    // Type is inferred, This only exists before the type checking stage.
    // All 'inferred' variables must be evaluated to other types after the AST stage for the program to compile.
    // At the header parsing stage, 'inferred' is used where a symbol type is not yet known (as the type might be another header).
    Inferred,

    Reference(Box<DataType>, Ownership),

    Bool,
    Range, // Iterable that must always be owned.

    // Immutable Data Types
    // In practice, these types should not be deliberately used much at all
    None, // The None result of an option, or empty argument
    True,
    False,

    // Strings
    String, // UTF-8

    // Any type can be used in the expression and will be coerced to a string (for scenes only).
    // Mathematical operations will still work and take priority, but strings can be used in these expressions.
    // All types will finally be coerced to strings after everything is evaluated.
    CoerceToString,

    // Numbers
    Float,
    Int,
    Decimal,

    // Collections.
    // A collection of single types, dynamically sized
    Collection(Box<DataType>, Ownership),

    // Structs
    Parameters(Vec<Arg>),        // Struct definitions and parameters
    Struct(Vec<Arg>, Ownership), // Struct instance

    // Special Beanstalk Types
    // Template types may have more static structure to them in the future
    // They are basically functions that accept a style and return a string
    Template, // is_mutable

    Function(FunctionSignature), // Arg constructor, Returned args

    // Type Types
    // Unions allow types such as option and result

    // TODO: IS THIS JUST MULTIPLE TYPES FOR FUNCTION RETURNS?
    // Choices should actually just be enums for now
    Choices(Vec<Arg>),     // Union of types
    Option(Box<DataType>), // Shorthand for a choice of a type or None
}

impl DataType {
    // IGNORES MUTABILITY
    pub fn is_valid_type(&self, accepted_type: &mut DataType) -> bool {
        // Has to make sure if either type is a union, that the other type is also a member of the union
        // red_ln!("checking if: {:?} is accepted by: {:?}", data_type, accepted_type);

        match self {
            DataType::Bool => {
                matches!(
                    accepted_type,
                    DataType::Bool | DataType::Int | DataType::Float
                )
            }
            //
            // DataType::Choices(types) => {
            //     for t in types {
            //         if !t.value.data_type.is_valid_type(accepted_type) {
            //             return false;
            //         }
            //     }
            //     true
            // }
            DataType::Range => {
                matches!(
                    accepted_type,
                    DataType::Collection(..)
                        | DataType::Parameters(_)
                        | DataType::Float
                        | DataType::Int
                        | DataType::Decimal
                        | DataType::String
                )
            }

            _ => {
                // For other 'self' types, check the accepted_type
                match accepted_type {
                    // Might be needed here later?
                    // DataType::Pointer => true,
                    DataType::Inferred => {
                        *accepted_type = self.to_owned();
                        true
                    }
                    DataType::CoerceToString => true,

                    // DataType::Choices(types) => {
                    //     for t in types {
                    //         if !self.is_valid_type(t.value.data_type) {
                    //             return false;
                    //         }
                    //     }
                    //     true
                    // }
                    DataType::Bool => {
                        matches!(self, &DataType::Bool | &DataType::Int | &DataType::Float)
                    }

                    _ => false,
                }
            }
        }
    }

    // Special Types that might change (basically the same as rust with more syntax sugar)
    pub fn to_option(self) -> DataType {
        match self {
            DataType::Bool => DataType::Option(Box::new(DataType::Bool)),
            DataType::Float => DataType::Option(Box::new(DataType::Float)),
            DataType::Int => DataType::Option(Box::new(DataType::Int)),
            DataType::Decimal => DataType::Option(Box::new(DataType::Decimal)),
            DataType::String => DataType::Option(Box::new(DataType::String)),
            DataType::Collection(inner_type, ownership) => {
                DataType::Option(Box::new(DataType::Collection(inner_type, ownership)))
            }
            DataType::Parameters(args) => DataType::Option(Box::new(DataType::Parameters(args))),
            DataType::Struct(args, ownership) => {
                DataType::Option(Box::new(DataType::Struct(args, ownership)))
            }
            DataType::Function(signature) => {
                DataType::Option(Box::new(DataType::Function(signature)))
            }
            DataType::Template => DataType::Option(Box::new(DataType::Template)),
            DataType::Inferred => DataType::Option(Box::new(DataType::Inferred)),
            DataType::CoerceToString => DataType::Option(Box::new(DataType::CoerceToString)),
            DataType::True => DataType::Option(Box::new(DataType::True)),
            DataType::False => DataType::Option(Box::new(DataType::False)),
            DataType::Choices(inner_types) => {
                DataType::Option(Box::new(DataType::Choices(inner_types)))
            }

            DataType::Reference(inner_type, ownership) => {
                DataType::Option(Box::new(DataType::Reference(inner_type, ownership)))
            }

            // TODO: Probably should error for these
            DataType::None => DataType::Option(Box::new(DataType::None)),
            DataType::Range => DataType::Option(Box::new(DataType::Range)),
            DataType::Option(_) => DataType::Option(Box::new(DataType::Option(Box::new(self)))),
        }
    }

    pub fn is_iterable(&self) -> bool {
        match self {
            DataType::Range => true,
            DataType::Collection(..) => true,
            DataType::Parameters(_) => true,
            DataType::String => true,
            DataType::Float => true,
            DataType::Int => true,
            DataType::Decimal => true,
            DataType::Inferred => true, // Will need to be type checked later
            _ => false,
        }
    }

    pub fn get_iterable_type(&self) -> DataType {
        match self {
            DataType::Collection(inner_type, ..) => *inner_type.to_owned(),
            _ => self.to_owned(),
        }
    }

    pub fn to_arg(&self) -> Arg {
        Arg {
            name: String::new(),
            value: Expression::new(
                ExpressionKind::None,
                TextLocation::default(),
                self.to_owned(),
                Ownership::MutableOwned,
            ),
        }
    }
}

impl PartialEq for DataType {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (DataType::Inferred, DataType::Inferred) => true,
            (DataType::Reference(a, oa), DataType::Reference(b, ob)) => a == b && oa == ob,
            (DataType::Bool, DataType::Bool) => true,
            (DataType::Range, DataType::Range) => true,
            (DataType::None, DataType::None) => true,
            (DataType::True, DataType::True) => true,
            (DataType::False, DataType::False) => true,
            (DataType::String, DataType::String) => true,
            (DataType::CoerceToString, DataType::CoerceToString) => true,
            (DataType::Float, DataType::Float) => true,
            (DataType::Int, DataType::Int) => true,
            (DataType::Decimal, DataType::Decimal) => true,
            (DataType::Collection(a, oa), DataType::Collection(b, ob)) => a == b && oa == ob,
            (DataType::Template, DataType::Template) => true,
            (DataType::Option(a), DataType::Option(b)) => a == b,
            // For Args, Struct, Function, and Choices, we compare by name/structure
            // but not by the actual Arg values since they contain Expressions
            (DataType::Parameters(a), DataType::Parameters(b)) => {
                a.len() == b.len()
                    && a.iter()
                        .zip(b.iter())
                        .all(|(arg_a, arg_b)| arg_a.name == arg_b.name)
            }
            (DataType::Struct(a, oa), DataType::Struct(b, ob)) => {
                oa == ob
                    && a.len() == b.len()
                    && a.iter()
                        .zip(b.iter())
                        .all(|(arg_a, arg_b)| arg_a.name == arg_b.name)
            }
            (DataType::Function(signature1), DataType::Function(signature2)) => {
                // If both functions have the same signature.returns types,
                // then they are equal
                signature1.returns.len() == signature2.returns.len()
                    && signature1
                        .returns
                        .iter()
                        .zip(signature2.returns.iter())
                        .all(|(return1, return2)| {
                            return1.value.data_type == return2.value.data_type
                        })
            }
            (DataType::Choices(a), DataType::Choices(b)) => {
                a.len() == b.len()
                    && a.iter()
                        .zip(b.iter())
                        .all(|(arg_a, arg_b)| arg_a.name == arg_b.name)
            }
            _ => false,
        }
    }
}

impl Display for DataType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DataType::Reference(inner_type, ownership) => {
                let ownership = ownership.as_string();
                write!(f, "{inner_type} {ownership} Reference")
            }
            DataType::Inferred => {
                write!(f, "Inferred")
            }
            DataType::CoerceToString => {
                write!(f, "CoerceToString")
            }
            DataType::Bool => write!(f, "Bool"),
            DataType::String => {
                write!(f, "String")
            }
            DataType::Float => {
                write!(f, "Float")
            }
            DataType::Int => write!(f, "Int"),
            DataType::Decimal => {
                write!(f, "Decimal")
            }
            DataType::Collection(inner_type, _mutable) => {
                write!(f, "{inner_type} Collection")
            }
            DataType::Parameters(args) => {
                let mut arg_str = String::new();
                for arg in args {
                    arg_str.push_str(&format!("{}: {}, ", arg.name, arg.value.data_type));
                }
                write!(f, "{self:?} Arguments({arg_str})")
            }
            DataType::Struct(args, ..) => {
                let mut arg_str = String::new();
                for arg in args {
                    arg_str.push_str(&format!("{}: {}, ", arg.name, arg.value.data_type));
                }
                write!(f, "{self:?} Arguments({arg_str})")
            }

            DataType::Function(signature) => {
                let mut arg_str = String::new();
                let mut returns_string = String::new();
                for arg in &signature.parameters {
                    arg_str.push_str(&format!("{}: {}, ", arg.name, arg.value.data_type));
                }
                for return_type in &signature.returns {
                    returns_string.push_str(&format!("{}, ", return_type.name));
                }

                write!(f, "Function({arg_str} -> {returns_string})")
            }
            DataType::Template => {
                write!(f, "Template")
            }
            DataType::None => write!(f, "None"),
            DataType::True => write!(f, "True"),
            DataType::False => write!(f, "False"),
            DataType::Range => write!(f, "Range"),
            DataType::Option(inner_type) => write!(f, "Option({inner_type})"),
            DataType::Choices(inner_types) => {
                let mut inner_types_str = String::new();
                for inner_type in inner_types {
                    let inner_type = inner_type.value.data_type.to_owned();
                    inner_types_str.push_str(&format!("{inner_type}"));
                }
                write!(f, "Choices({inner_types_str})")
            }
        }
    }
}

// pub fn get_rgba_args() -> DataType {
//     DataType::Args(vec![
//         Arg {
//             name: "red".to_string(),
//             data_type: DataType::Choices(vec![DataType::Float(false), DataType::Int(false)]),
//             default_value: ExpressionKind::Float(0.0),
//         },
//         Arg {
//             name: "green".to_string(),
//             data_type: DataType::Choices(vec![DataType::Float(false), DataType::Int(false)]),
//             default_value: ExpressionKind::Float(0.0),
//         },
//         Arg {
//             name: "blue".to_string(),
//             data_type: DataType::Choices(vec![DataType::Float(false), DataType::Int(false)]),
//             default_value: ExpressionKind::Float(0.0),
//         },
//         Arg {
//             name: "alpha".to_string(),
//             data_type: DataType::Choices(vec![DataType::Float(false), DataType::Int(false)]),
//             default_value: ExpressionKind::Float(1.0),
//         },
//     ])
// }
