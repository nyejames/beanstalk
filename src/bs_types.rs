use crate::parsers::ast_nodes::{Arg, Value};

#[derive(Debug, Clone, PartialEq)]
pub enum DataType {
    // Mutability is part of the type
    // This helps with compile time constant folding

    // Mutable Data Types will have an additional bool to indicate whether they are mutable
    Inferred, // Type is inferred, this only gets to the emitter stage if it will definitely be JS rather than WASM
    Bool(bool),

    // Immutable Data Types
    // In practice, these types should not be deliberately used much at all
    // The result / option types will be worked with directly instead
    Error(String),
    None, // The None result of an option, or empty argument
    True,
    False,

    // Strings
    String(bool), // UTF-8 (will probably just be utf 16 because js for now)
    // Any type can be used in the expression and will be coerced to a string (for scenes only)
    // Mathematical operations will still work and take priority, but strings can be used in these expressions
    // And all types will finally be coerced to strings after everything is evaluated
    CoerceToString(bool),

    // Numbers
    Float(bool),
    Int(bool),
    Decimal(bool),

    // Collections
    // Collection of a single type, dynamically sized
    // Uses curly brackets {}
    Collection(Box<DataType>),

    // Structures (structs)
    // They are just one or more arguments (can be named and have default values) inside of regular brackets ()
    // They can have mixed types but must be a fixed size (like structs)
    // Since this is a list of args, there is some unique implicit behaviour with these tuples
    // An empty tuple is equivalent to None
    // A tuple of one item is equivalent to that item (this is automatically casted by the language)
    // TODO - Could a tuple of one item as a datatype represent a tuple that can have any number of arguments of that type when being created?
    // (but it maintains a fixed size once it's created - basically an Array)
    // This would superficially just be a fixed array type (like a collection but with a fixed size)
    // It would need it's own syntax for specifying it's size

    /*
        Examples for compiler stuff:

        - A function call that takes any sized list of ints would have its argument datatype be:
        DataType::Tuple(Box::new(DataType::Int))

        - A function call that can take any argument (will be converted into a string)
        DataType::CoerceToString

        - A function call that takes no arguments
        EITHER:
            DataType::Tuple(Vec::new()) -- this is bad practice but would still result in None
            OR
            DataType::None -- correct way to say None
    */
    Structure(Vec<Arg>),

    // Special Beanstalk Types
    // Scene types may have more static structure to them in the future
    Scene,

    // Functions have named arguments
    // These arguments are effectively identical to tuples
    // We don't use a Datatypes here (to put two tuples there) as it just adds an extra unwrapping step
    // And we want to be able to have optional names / default values for even single arguments
    Function(Vec<Arg>, Box<DataType>), // Arguments, Return type

    // Type Types
    // Unions allow types such as option and result
    Choice(Vec<DataType>), // Union of types

    // For generics
    Type,
}


impl DataType {
    pub fn is_valid_type(&self, accepted_type: &mut DataType) -> bool {
        // Has to make sure if either type is a union, that the other type is also a member of the union
        // red_ln!("checking if: {:?} is accepted by: {:?}", data_type, accepted_type);

        if let DataType::Choice(types) = self {
            for t in types {
                if t.is_valid_type(accepted_type) {
                    return true;
                }
            }
            return false;
        }

        match accepted_type {
            DataType::Inferred => {
                *accepted_type = self.to_owned();
                true
            }
            DataType::CoerceToString(_) => true,
            DataType::Choice(types) => {
                for t in types {
                    if self == t {
                        return true;
                    }
                }
                false
            }
            _ => {
                self == accepted_type
            },
        }
    }

    pub fn length(&self) -> u32 {
        match self {
            DataType::Inferred => 0,
            DataType::CoerceToString(_) => 0,
            DataType::Bool(_) => 4,
            DataType::True => 4,
            DataType::False => 5,
            DataType::String(_) => 6,
            DataType::Float(_) => 5,
            DataType::Int(_) => 3,
            DataType::Decimal(_) => 6,
            DataType::Collection(inner_type) => inner_type.length(),

            DataType::Structure(_) => 1,
            DataType::Choice(inner_types) => {
                let mut length = 0;
                for arg in inner_types {
                    length += arg.length();
                }
                length
            }
            DataType::Function(..) => 2,
            DataType::Type => 4,
            DataType::Scene => 5,
            DataType::Error(_) => 1,

            DataType::None => 4,
        }
    }

    // Special Types that might change (basically same as rust with a bit more syntax sugar)
    pub fn create_option_datatype(self) -> DataType {
        match self {
            DataType::Inferred => DataType::Choice(vec![DataType::None, DataType::Inferred]),
            DataType::CoerceToString(mutable) => {
                DataType::Choice(vec![DataType::None, DataType::CoerceToString(mutable)])
            }
            DataType::Bool(mutable) => DataType::Choice(vec![DataType::None, DataType::Bool(mutable)]),
            DataType::True => DataType::Choice(vec![DataType::None, DataType::True]),
            DataType::False => DataType::Choice(vec![DataType::None, DataType::False]),
            DataType::String(mutable) => DataType::Choice(vec![DataType::None, DataType::String(mutable)]),
            DataType::Float(mutable) => DataType::Choice(vec![DataType::None, DataType::Float(mutable)]),
            DataType::Int(mutable) => DataType::Choice(vec![DataType::None, DataType::Int(mutable)]),
            DataType::Collection(inner_type) => {
                DataType::Choice(vec![DataType::None, DataType::Collection(inner_type)])
            }
            DataType::Decimal(mutable) => DataType::Choice(vec![DataType::None, DataType::Decimal(mutable)]),
            DataType::Type => DataType::Choice(vec![DataType::None, DataType::Type]),
            DataType::Choice(inner_types) => {
                DataType::Choice(vec![DataType::None, DataType::Choice(inner_types)])
            }
            DataType::Function(args, return_type) => {
                DataType::Choice(vec![DataType::None, DataType::Function(args, return_type)])
            }
            DataType::Structure(args) => {
                DataType::Choice(vec![DataType::None, DataType::Structure(args)])
            }
            _ => DataType::Error(format!(
                "You can't create an option of {:?} and None",
                self
            )),
        }
    }
    
    pub fn is_mutable(&self) -> bool {
        match self {
            DataType::Inferred => false,
            DataType::CoerceToString(mutable) => *mutable,
            DataType::Bool(mutable) => *mutable,
            DataType::True => false,
            DataType::False => false,
            DataType::String(mutable) => *mutable,
            DataType::Float(mutable) => *mutable,
            DataType::Int(mutable) => *mutable,
            DataType::Decimal(mutable) => *mutable,
            DataType::Collection(inner_type) => inner_type.is_mutable(),
            DataType::Structure(args) => {
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
}

pub fn get_any_number_datatype(mutable: bool) -> DataType {
    DataType::Choice(vec![DataType::Float(mutable), DataType::Int(mutable), DataType::Decimal(mutable)])
}

pub fn get_rgba_args() -> DataType {
    DataType::Structure(vec![
        Arg {
            name: "red".to_string(),
            data_type: DataType::Choice(vec![DataType::Float(false), DataType::Int(false)]),
            value: Value::Float(0.0),
        },
        Arg {
            name: "green".to_string(),
            data_type: DataType::Choice(vec![DataType::Float(false), DataType::Int(false)]),
            value: Value::Float(0.0),
        },
        Arg {
            name: "blue".to_string(),
            data_type: DataType::Choice(vec![DataType::Float(false), DataType::Int(false)]),
            value: Value::Float(0.0),
        },
        Arg {
            name: "alpha".to_string(),
            data_type: DataType::Choice(vec![DataType::Float(false), DataType::Int(false)]),
            value: Value::Float(1.0),
        },
    ])
}

pub fn get_reference_data_type(data_type: &DataType, arguments_accessed: &[usize]) -> DataType {
    match arguments_accessed.first() {
        Some(index) => match &data_type {
            DataType::Structure(inner_types) => {
                // This should never happen (caught earlier in compiler)
                debug_assert!(index < &inner_types.len());
                debug_assert!(index >= &0);

                // This part could be recursively check if there are more arguments to access
                if arguments_accessed.len() > 1 {
                    get_reference_data_type(
                        &inner_types[*index].data_type,
                        &arguments_accessed[1..],
                    )
                } else {
                    inner_types[*index].data_type.to_owned()
                }
            }

            DataType::Collection(inner_type) | DataType::Function(_, inner_type) => {
                // This part could be recursive as get_type() can call this function again
                let inner_type = *inner_type.to_owned();
                if arguments_accessed.len() > 1 {
                    // Could be trying to access a non-collection or struct,
                    // But this should be caught earlier in the compiler
                    get_reference_data_type(&inner_type, &arguments_accessed[1..])
                } else {
                    inner_type
                }
            }

            _ => {
                // TODO - get any implemented or built in methods on this data type
                data_type.to_owned()
            }
        },

        None => data_type.to_owned(),
    }
}


