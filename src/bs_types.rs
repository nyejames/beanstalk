use crate::parsers::ast_nodes::{Arg, Value};

#[derive(Debug, Clone, PartialEq)]
pub enum DataType {
    // Mutability is part of the type
    // This helps with compile time constant folding

    // Mutable Data Types will have an additional bool to indicate whether they are mutable
    Inferred, // Type is inferred, this only gets to the emitter stage if it will definitely be JS rather than WASM
    Bool,

    // Immutable Data Types
    // In practice, these types should not be deliberately used much at all
    // The result / option types will be worked with directly instead
    Error(String),
    None, // The None result of an option, or empty argument
    True,
    False,

    // Strings
    String, // UTF-8 (will probably just be utf 16 because js for now)
    // Any type can be used in the expression and will be coerced to a string (for scenes only)
    // Mathematical operations will still work and take priority, but strings can be used in these expressions
    // And all types will finally be coerced to strings after everything is evaluated
    CoerceToString,

    // Numbers
    Float,
    Int,
    Decimal,

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
    // Scene and style types may have more static structure to them in the future
    Scene,
    Style,

    // Functions have named arguments
    // These arguments are effectively identical to tuples
    // We don't use a Datatypes here (to put two tuples there) as it just adds an extra unwrapping step
    // And we want to be able to have optional names / default values for even single arguments
    Function(Vec<Arg>, Vec<Arg>), // Arguments, Return type

    // Type Types
    // Unions allow types such as option and result
    Choice(Vec<DataType>), // Union of types

    // For generics
    Type,
}

// Special Types that might change (basically same as rust with a bit more syntax sugar)
pub fn create_option_datatype(datatype: DataType) -> DataType {
    match datatype {
        DataType::Inferred => DataType::Choice(vec![DataType::None, DataType::Inferred]),
        DataType::CoerceToString => {
            DataType::Choice(vec![DataType::None, DataType::CoerceToString])
        }
        DataType::Bool => DataType::Choice(vec![DataType::None, DataType::Bool]),
        DataType::True => DataType::Choice(vec![DataType::None, DataType::True]),
        DataType::False => DataType::Choice(vec![DataType::None, DataType::False]),
        DataType::String => DataType::Choice(vec![DataType::None, DataType::String]),
        DataType::Float => DataType::Choice(vec![DataType::None, DataType::Float]),
        DataType::Int => DataType::Choice(vec![DataType::None, DataType::Int]),
        DataType::Collection(inner_type) => {
            DataType::Choice(vec![DataType::None, DataType::Collection(inner_type)])
        }
        DataType::Decimal => DataType::Choice(vec![DataType::None, DataType::Decimal]),
        DataType::Type => DataType::Choice(vec![DataType::None, DataType::Type]),
        DataType::Style => DataType::Choice(vec![DataType::None, DataType::Style]),
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
            datatype
        )),
    }
}

pub fn get_any_number_datatype() -> DataType {
    DataType::Choice(vec![DataType::Float, DataType::Int, DataType::Decimal])
}

pub fn get_rgba_args() -> DataType {
    DataType::Structure(vec![
        Arg {
            name: "red".to_string(),
            data_type: DataType::Choice(vec![DataType::Float, DataType::Int]),
            value: Value::Float(0.0),
        },
        Arg {
            name: "green".to_string(),
            data_type: DataType::Choice(vec![DataType::Float, DataType::Int]),
            value: Value::Float(0.0),
        },
        Arg {
            name: "blue".to_string(),
            data_type: DataType::Choice(vec![DataType::Float, DataType::Int]),
            value: Value::Float(0.0),
        },
        Arg {
            name: "alpha".to_string(),
            data_type: DataType::Choice(vec![DataType::Float, DataType::Int]),
            value: Value::Float(1.0),
        },
    ])
}

pub fn get_reference_data_type(data_type: &DataType, arguments_accessed: &[usize]) -> DataType {
    match arguments_accessed.first() {
        Some(index) => match &data_type {
            DataType::Structure(inner_types) | DataType::Function(_, inner_types) => {
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

            DataType::Collection(inner_type) => {
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

pub fn get_type_keyword_length(data_type: &DataType) -> u32 {
    match data_type {
        DataType::Inferred => 0,
        DataType::CoerceToString => 0,
        DataType::Bool => 4,
        DataType::True => 4,
        DataType::False => 5,
        DataType::String => 6,
        DataType::Float => 5,
        DataType::Int => 3,
        DataType::Decimal => 6,
        DataType::Collection(inner_type) => get_type_keyword_length(inner_type),

        DataType::Structure(_) => 1,
        DataType::Choice(inner_types) => {
            let mut length = 0;
            for arg in inner_types {
                length += get_type_keyword_length(arg);
            }
            length
        }
        DataType::Function(..) => 2,
        DataType::Type => 4,
        DataType::Scene => 5,
        DataType::Style => 5,
        DataType::Error(_) => 1,

        DataType::None => 4,
    }
}
