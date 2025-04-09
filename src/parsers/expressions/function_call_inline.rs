use crate::CompileError;
use crate::parsers::ast_nodes::{AstNode, Value};

pub fn inline_function_call(
    arguments: &[Value],
    argument_accessed: &[usize],
    function: &Value,
) -> Result<AstNode, CompileError> {

    // Unpack the function
    let (body, mut declarations, data_type, token_position) = match function {
        Value::Function(_, req_arguments, body, _, return_type, token_position, _) => {
            (body, req_arguments.to_owned(), return_type.to_owned(), token_position.to_owned())
        }
        _ => {
            return Err(CompileError {
                msg: format!("Expected a function, got {:?}", function),
                start_pos: function.dimensions(),
                end_pos: function.dimensions(),
                error_type: crate::ErrorType::Compiler,
            })
        }
    };

    // Replace the required argument values with the passed in arguments
    // So now we have all the declarations ready for parsing the body
    for (i, arg) in declarations.iter_mut().enumerate() {
        arg.value = arguments[i].to_owned();
    }
    
    // TODO - Evaluate the function body ast and return the value
    // We need to step through the function AST (body)
    // And follow any branches - all variables are known at this point if it's got to this point
    // We follow the branches down to the first return statement and return that value
    
    
    // TEMPORARY just find the first return and send back the value
    for node in body {
        if let AstNode::Return(value, ..) = node {
            return Ok(AstNode::Literal(value.to_owned(), token_position));
        }
    }

    
    Err(CompileError {
        msg: "Could not inline function call (this value should not have been passed to inline_function_call as its not pure)".to_string(),
        start_pos: token_position.to_owned(),
        end_pos: token_position,
        error_type: crate::ErrorType::Compiler,
    })
}
