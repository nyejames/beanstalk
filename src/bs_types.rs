use crate::parsers::ast_nodes::{Arg, Expr};

#[derive(Debug, Clone, PartialEq)]
pub enum DataType {
    // Mutability is part of the type
    // This helps with compile time constant folding

    // Custom types (references to user types) must store their name to be type-checked later and replaced with the correct shape.
    // We use this at the AST stage for imports or references when we can't figure out the type yet
    UnknownReference(String, bool),

    // Type is inferred, This only exists before the type checking stage
    // All 'inferred' variables must be evaluated to other types after the AST stage for the program to compile
    Inferred(bool),

    Bool(bool),
    Range, // Iterable

    // Immutable Data Types
    // In practice, these types should not be deliberately used much at all
    None, // The None result of an option, or empty argument
    True,
    False,

    // Strings
    String(bool), // UTF-8 (will probably just be utf 16 because js for now).

    // Any type can be used in the expression and will be coerced to a string (for scenes only).
    // Mathematical operations will still work and take priority, but strings can be used in these expressions.
    // All types will finally be coerced to strings after everything is evaluated.
    CoerceToString(bool),

    // Numbers
    Float(bool),
    Int(bool),
    Decimal(bool),

    // Collections.
    // A collection of single types, dynamically sized
    Collection(Box<DataType>),

    // Used for constructing new types
    Object(Vec<Arg>),

    // Special Beanstalk Types
    // Scene types may have more static structure to them in the future
    Scene(bool), // is_mutable

    // Blocks are either functions or classes or both depending on their signature
    Block(Vec<Arg>, Vec<DataType>), // Arg constructor, Returned args

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
                    DataType::Collection(_)
                        | DataType::Object(_)
                        | DataType::Float(_)
                        | DataType::Int(_)
                        | DataType::Decimal(_)
                        | DataType::String(_)
                );
            }

            DataType::UnknownReference(..) => {
                return true;
            }

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

            _ => self == accepted_type,
        }
    }

    pub fn length(&self) -> u32 {
        match self {
            DataType::UnknownReference(name, mutable) => name.len() as u32 + if *mutable { 1 } else { 0 },
            DataType::Inferred(_) => 0,
            DataType::CoerceToString(_) => 0,
            DataType::Bool(_) => 4,
            DataType::Range => 0,
            DataType::True => 4,
            DataType::False => 5,
            DataType::String(_) => 6,
            DataType::Float(_) => 5,
            DataType::Int(_) => 3,
            DataType::Decimal(_) => 6,
            DataType::Collection(inner_type) => inner_type.length(),

            DataType::Object(_) => 1,
            DataType::Choices(inner_types) => {
                let mut length = 0;
                for arg in inner_types {
                    length += arg.length();
                }
                length
            }
            DataType::Block(..) => 2,
            DataType::Scene(_) => 5,

            DataType::Option(inner_type) => inner_type.length() + 1,
            DataType::None => 4,
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
            DataType::Collection(inner_type) => {
                DataType::Option(Box::new(DataType::Collection(inner_type)))
            }
            DataType::Object(args) => DataType::Option(Box::new(DataType::Object(args))),
            DataType::Block(args, return_type) => {
                DataType::Option(Box::new(DataType::Block(args, return_type)))
            }
            DataType::Scene(mutable) => DataType::Option(Box::new(DataType::Scene(mutable))),
            DataType::UnknownReference(name, mutable) => DataType::Option(Box::new(DataType::UnknownReference(name, mutable))),
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
            DataType::Inferred(mutable) => *mutable,
            DataType::CoerceToString(mutable) => *mutable,
            DataType::Bool(mutable) => *mutable,
            DataType::True => false,
            DataType::False => false,
            DataType::String(mutable) => *mutable,
            DataType::Float(mutable) => *mutable,
            DataType::Int(mutable) => *mutable,
            DataType::Decimal(mutable) => *mutable,
            DataType::Collection(inner_type) => inner_type.is_mutable(),
            DataType::Object(args) => {
                for arg in args {
                    if arg.data_type.is_mutable() {
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
            DataType::Collection(_) => true,
            DataType::Object(_) => true,
            DataType::String(_) => true,
            DataType::Float(_) => true,
            DataType::Int(_) => true,
            DataType::Decimal(_) => true,
            DataType::UnknownReference(..) => false,
            DataType::Inferred(_) => true, // Will need to be type checked later
            _ => false,
        }
    }

    pub fn get_iterable_type(&self) -> DataType {
        match self {
            DataType::Collection(inner_type) => *inner_type.to_owned(),
            _ => self.to_owned(),
        }
    }

    pub fn to_mutable(&self) -> DataType {
        match self {
            DataType::Inferred(_) => DataType::Inferred(true),
            DataType::CoerceToString(_) => DataType::CoerceToString(true),
            DataType::Bool(_) => DataType::Bool(true),
            DataType::String(_) => DataType::String(true),
            DataType::Float(_) => DataType::Float(true),
            DataType::Int(_) => DataType::Int(true),
            DataType::Decimal(_) => DataType::Decimal(true),
            DataType::Collection(inner_type) => {
                DataType::Collection(Box::new(inner_type.to_mutable()))
            }
            DataType::Object(args) => {
                let mut new_args = Vec::new();
                for arg in args {
                    new_args.push(Arg {
                        name: arg.name.to_owned(),
                        data_type: arg.data_type.to_mutable(),
                        default_value: arg.default_value.to_owned(),
                    });
                }

                DataType::Object(new_args)
            }
            _ => self.to_owned(),
        }
    }
    
    pub fn to_string(&self) -> String {
        match self {
            DataType::Inferred(mutable) => format!("{} Inferred", if *mutable {"mutable"} else {""}),
            DataType::CoerceToString(mutable) => format!("{} CoerceToString", if *mutable {"mutable"} else {""}),
            DataType::Bool(mutable) => format!("{} Bool", if *mutable {"mutable"} else {""}),
            DataType::String(mutable) => format!("{} String", if *mutable {"mutable"} else {""}),
            DataType::Float(mutable) => format!("{} Float", if *mutable {"mutable"} else {""}),
            DataType::Int(mutable) => format!("{} Int", if *mutable {"mutable"} else {""}),
            DataType::Decimal(mutable) => format!("{} Decimal", if *mutable {"mutable"} else {""}),
            DataType::Collection(inner_type) => format!("{} Collection", inner_type.to_string()),
            DataType::Object(args) => {
                let mut arg_str = String::new();
                for arg in args {
                    arg_str.push_str(&format!("{}: {}, ", arg.name, arg.data_type.to_string()));
                }
                format!("{} Arguments({})", self.to_string(), arg_str)
            }
            DataType::Block(args, return_types) => {
                let mut arg_str = String::new();
                let mut returns_string = String::new();
                for arg in args {
                    arg_str.push_str(&format!("{}: {}, ", arg.name, arg.data_type.to_string()));
                }
                for return_type in return_types {
                    returns_string.push_str(&format!("{}, ", return_type.to_string()));
                }
                
                format!("Block({} -> {})", arg_str, returns_string)
            }
            DataType::Scene(mutable) => format!("{} Scene", if *mutable {"mutable"} else {""}),
            DataType::UnknownReference(name, ..) => format!("{} Pointer", name),
            DataType::None => "None".to_owned(),
            DataType::True => "True".to_owned(),
            DataType::False => "False".to_owned(),
            DataType::Range => "Range".to_owned(),
            DataType::Option(inner_type) => format!("Option({})", inner_type.to_string()),
            DataType::Choices(inner_types) => {
                let mut inner_types_str = String::new();
                for inner_type in inner_types {
                    inner_types_str.push_str(&format!("{}, ", inner_type.to_string()));
                }
                format!("Choices({})", inner_types_str)
            }
        }
    }
}

pub fn get_any_number_datatype(mutable: bool) -> DataType {
    DataType::Choices(vec![
        DataType::Float(mutable),
        DataType::Int(mutable),
        DataType::Decimal(mutable),
    ])
}

pub fn get_rgba_args() -> DataType {
    DataType::Object(vec![
        Arg {
            name: "red".to_string(),
            data_type: DataType::Choices(vec![DataType::Float(false), DataType::Int(false)]),
            default_value: Expr::Float(0.0),
        },
        Arg {
            name: "green".to_string(),
            data_type: DataType::Choices(vec![DataType::Float(false), DataType::Int(false)]),
            default_value: Expr::Float(0.0),
        },
        Arg {
            name: "blue".to_string(),
            data_type: DataType::Choices(vec![DataType::Float(false), DataType::Int(false)]),
            default_value: Expr::Float(0.0),
        },
        Arg {
            name: "alpha".to_string(),
            data_type: DataType::Choices(vec![DataType::Float(false), DataType::Int(false)]),
            default_value: Expr::Float(1.0),
        },
    ])
}

pub fn get_accessed_data_type(data_type: &DataType, members_accessed: &[Arg]) -> DataType {
    match members_accessed.first() {
        Some(member) => match &data_type {
            DataType::Object(inner_types) => {
                // This part could be recursively check if there are more arguments to access
                if members_accessed.len() > 1 {
                    get_accessed_data_type(
                        &inner_types
                            .iter()
                            .find(|t| t.name == member.name)
                            .unwrap()
                            .data_type,
                        &members_accessed[1..],
                    )

                } else {
                    inner_types
                        .iter()
                        .find(|t| t.name == member.name)
                        .unwrap()
                        .data_type
                        .to_owned()
                }
            }

            // Needs to check built-in methods same as get_accessed_args() does
            _ => {
                // TODO - get any implemented or built in methods on this data type
                data_type.to_owned()
            }
        },

        None => data_type.to_owned(),
    }
}
