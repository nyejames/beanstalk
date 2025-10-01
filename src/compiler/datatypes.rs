use crate::compiler::parsers::ast_nodes::Arg;
use crate::compiler::parsers::expressions::expression::Expression;
use crate::compiler::parsers::statements::create_template_node::Template;

use crate::compiler::parsers::tokens::TextLocation;
use std::fmt::Display;

#[derive(Debug, Clone, PartialEq)]
pub enum Ownership {
    // Bool = is last use?
    MutableOwned(bool),
    MutableReference(bool),
    ImmutableOwned(bool),
    ImmutableReference(bool),
}

impl Ownership {
    pub fn default() -> Ownership {
        Ownership::ImmutableOwned(false)
    }
    pub fn is_mutable(&self) -> bool {
        match &self {
            Ownership::MutableOwned(..) => true,
            Ownership::MutableReference(..) => true,
            _ => false,
        }
    }
    pub fn as_string(&self) -> String {
        match &self {
            Ownership::MutableOwned(..) => String::from("mutable"),
            Ownership::MutableReference(..) => String::from("mutable reference"),
            Ownership::ImmutableOwned(..) => String::from("immutable"),
            Ownership::ImmutableReference(..) => String::from("immutable reference"),
        }
    }
}

#[derive(Debug, Clone)]
pub enum DataType {
    // Mutability is part of the type
    // This helps with compile time constant folding

    // Type is inferred, This only exists before the type checking stage
    // All 'inferred' variables must be evaluated to other types after the AST stage for the program to compile
    Inferred(Ownership),

    Bool(Ownership),
    Range, // Iterable that must always be owned.

    // Immutable Data Types
    // In practice, these types should not be deliberately used much at all
    None, // The None result of an option, or empty argument
    True,
    False,

    // Strings
    String(Ownership), // UTF-8 (will probably just be utf 16 because js for now).

    // Any type can be used in the expression and will be coerced to a string (for scenes only).
    // Mathematical operations will still work and take priority, but strings can be used in these expressions.
    // All types will finally be coerced to strings after everything is evaluated.
    CoerceToString(Ownership),

    // Numbers
    Float(Ownership),
    Int(Ownership),
    Decimal(Ownership),

    // Collections.
    // A collection of single types, dynamically sized
    Collection(Box<DataType>, Ownership),

    // Structs
    Args(Vec<Arg>),              // Type
    Struct(Vec<Arg>, Ownership), // Struct instance

    // Special Beanstalk Types
    // Template types may have more static structure to them in the future
    // They are basically functions that accept a style and return a string
    Template(Ownership), // is_mutable

    Function(Vec<Arg>, Vec<DataType>), // Arg constructor, Returned args

    // Type Types
    // Unions allow types such as option and result
    Choices(Vec<DataType>), // Union of types
    Option(Box<DataType>),  // Shorthand for a choice of a type or None
}

impl DataType {
    // IGNORES MUTABILITY
    pub fn is_valid_type(&self, accepted_type: &mut DataType) -> bool {
        // Has to make sure if either type is a union, that the other type is also a member of the union
        // red_ln!("checking if: {:?} is accepted by: {:?}", data_type, accepted_type);

        match self {
            DataType::Bool(_) => {
                return matches!(
                    accepted_type,
                    DataType::Bool(_) | DataType::Int(_) | DataType::Float(_)
                );
            }

            DataType::Choices(types) => {
                for t in types {
                    if !t.is_valid_type(accepted_type) {
                        return false;
                    }
                }
                return true;
            }

            DataType::Range => {
                return matches!(
                    accepted_type,
                    DataType::Collection(..)
                        | DataType::Args(_)
                        | DataType::Float(_)
                        | DataType::Int(_)
                        | DataType::Decimal(_)
                        | DataType::String(_)
                );
            }

            accepted_type => return true,

            _ => {}
        }

        match accepted_type {
            // Might be needed here later?
            // DataType::Pointer => true,
            DataType::Inferred(_) => {
                *accepted_type = self.to_owned();
                true
            }
            DataType::CoerceToString(_) => true,

            DataType::Choices(types) => {
                for t in types {
                    if !self.is_valid_type(t) {
                        return false;
                    }
                }
                true
            }

            DataType::Bool(_) => {
                matches!(
                    self,
                    &DataType::Bool(_) | &DataType::Int(_) | &DataType::Float(_)
                )
            }

            _ => false,
        }
    }

    // Special Types that might change (basically the same as rust with more syntax sugar)
    pub fn to_option(self) -> DataType {
        match self {
            DataType::Bool(mutable) => DataType::Option(Box::new(DataType::Bool(mutable))),
            DataType::Float(mutable) => DataType::Option(Box::new(DataType::Float(mutable))),
            DataType::Int(mutable) => DataType::Option(Box::new(DataType::Int(mutable))),
            DataType::Decimal(mutable) => DataType::Option(Box::new(DataType::Decimal(mutable))),
            DataType::String(mutable) => DataType::Option(Box::new(DataType::String(mutable))),
            DataType::Collection(inner_type, ownership) => {
                DataType::Option(Box::new(DataType::Collection(inner_type, ownership)))
            }
            DataType::Args(args) => DataType::Option(Box::new(DataType::Args(args))),
            DataType::Struct(args, ownership) => {
                DataType::Option(Box::new(DataType::Struct(args, ownership)))
            }
            DataType::Function(args, return_type) => {
                DataType::Option(Box::new(DataType::Function(args, return_type)))
            }
            DataType::Template(mutable) => DataType::Option(Box::new(DataType::Template(mutable))),
            DataType::Inferred(mutable) => DataType::Option(Box::new(DataType::Inferred(mutable))),
            DataType::CoerceToString(mutable) => {
                DataType::Option(Box::new(DataType::CoerceToString(mutable)))
            }
            DataType::True => DataType::Option(Box::new(DataType::True)),
            DataType::False => DataType::Option(Box::new(DataType::False)),
            DataType::Choices(inner_types) => {
                DataType::Option(Box::new(DataType::Choices(inner_types)))
            }

            // TODO: Probably should error for these
            DataType::None => DataType::Option(Box::new(DataType::None)),
            DataType::Range => DataType::Option(Box::new(DataType::Range)),
            DataType::Option(_) => DataType::Option(Box::new(DataType::Option(Box::new(self)))),
        }
    }

    pub fn is_mutable(&self) -> bool {
        match self {
            DataType::Inferred(ownership) => ownership.is_mutable(),
            DataType::CoerceToString(ownership) => ownership.is_mutable(),
            DataType::Bool(ownership) => ownership.is_mutable(),
            DataType::True => false,
            DataType::False => false,
            DataType::String(ownership) => ownership.is_mutable(),
            DataType::Float(ownership) => ownership.is_mutable(),
            DataType::Int(ownership) => ownership.is_mutable(),
            DataType::Decimal(ownership) => ownership.is_mutable(),
            DataType::Collection(_, ownership) => ownership.is_mutable(),
            DataType::Args(args) => {
                for arg in args {
                    if arg.value.data_type.is_mutable() {
                        return true;
                    }
                }
                false
            }
            _ => false,
        }
    }

    pub fn is_iterable(&self) -> bool {
        match self {
            DataType::Range => true,
            DataType::Collection(..) => true,
            DataType::Args(_) => true,
            DataType::String(_) => true,
            DataType::Float(_) => true,
            DataType::Int(_) => true,
            DataType::Decimal(_) => true,
            DataType::Inferred(_) => true, // Will need to be type checked later
            _ => false,
        }
    }

    pub fn get_iterable_type(&self) -> DataType {
        match self {
            DataType::Collection(inner_type, ..) => *inner_type.to_owned(),
            _ => self.to_owned(),
        }
    }

    pub fn to_compiler_owned(&self) -> DataType {
        match self {
            DataType::Inferred(_) => DataType::Inferred(Ownership::MutableOwned(false)),
            DataType::CoerceToString(_) => DataType::CoerceToString(Ownership::MutableOwned(false)),
            DataType::Bool(_) => DataType::Bool(Ownership::MutableOwned(false)),
            DataType::String(_) => DataType::String(Ownership::MutableOwned(false)),
            DataType::Float(_) => DataType::Float(Ownership::MutableOwned(false)),
            DataType::Int(_) => DataType::Int(Ownership::MutableOwned(false)),
            DataType::Decimal(_) => DataType::Decimal(Ownership::MutableOwned(false)),
            DataType::Collection(inner_type, ..) => DataType::Collection(
                Box::new(inner_type.to_compiler_owned()),
                Ownership::MutableOwned(false),
            ),
            DataType::Args(args) => {
                let mut new_args = Vec::new();
                for arg in args {
                    new_args.push(Arg {
                        name: arg.name.to_owned(),
                        value: arg.value.to_owned(),
                    });
                }

                DataType::Args(new_args)
            }
            _ => self.to_owned(),
        }
    }

    pub fn get_zero_value(&self, location: TextLocation) -> Expression {
        match self {
            DataType::Float(_) => Expression::float(0.0, location),
            DataType::Int(_) => Expression::int(0, location),
            DataType::Bool(_) => Expression::bool(false, location),
            DataType::String(_) | DataType::CoerceToString(_) => {
                Expression::string(String::new(), location)
            }
            DataType::Template(_) => Expression::template(Template::create_default(None)),
            _ => Expression::none(),
        }
    }
}

impl Display for DataType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DataType::Inferred(mutable) => {
                write!(f, "Inferred {} ", mutable.as_string())
            }
            DataType::CoerceToString(mutable) => {
                write!(f, "CoerceToString {} ", mutable.as_string())
            }
            DataType::Bool(mutable) => write!(f, "Bool {} ", mutable.as_string()),
            DataType::String(mutable) => {
                write!(f, "String {} ", mutable.as_string())
            }
            DataType::Float(mutable) => {
                write!(f, "Float {} ", mutable.as_string())
            }
            DataType::Int(mutable) => write!(f, "{} Int", mutable.as_string()),
            DataType::Decimal(mutable) => {
                write!(f, "Decimal {} ", mutable.as_string())
            }
            DataType::Collection(inner_type, mutable) => {
                write!(f, "{inner_type} {} Collection", mutable.as_string())
            }
            DataType::Args(args) => {
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

            DataType::Function(args, return_types) => {
                let mut arg_str = String::new();
                let mut returns_string = String::new();
                for arg in args {
                    arg_str.push_str(&format!("{}: {}, ", arg.name, arg.value.data_type));
                }
                for return_type in return_types {
                    returns_string.push_str(&format!("{return_type}, "));
                }

                write!(f, "Function({arg_str} -> {returns_string})")
            }
            DataType::Template(mutable) => {
                write!(f, "Template {} ", mutable.as_string())
            }
            DataType::None => write!(f, "None"),
            DataType::True => write!(f, "True"),
            DataType::False => write!(f, "False"),
            DataType::Range => write!(f, "Range"),
            DataType::Option(inner_type) => write!(f, "Option({inner_type})"),
            DataType::Choices(inner_types) => {
                let mut inner_types_str = String::new();
                for inner_type in inner_types {
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


