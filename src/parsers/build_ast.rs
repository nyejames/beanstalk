use super::{
    ast::AstNode, collections::new_array, create_scene_node::new_scene, parse_expression::{create_expression, eval_expression}
};
use crate::{bs_types::DataType, Token};

#[derive(PartialEq, Debug)]
enum Attribute {
    Exported,
    Constant,
    Mutable,
    Comptime,
    ComptimeConstant,
}

pub fn new_ast(tokens: Vec<Token>, start_index: usize) -> (Vec<AstNode>, usize) {
    let mut ast = Vec::new();
    let mut i = start_index;
    let mut attributes: Vec<Attribute> = Vec::new();

    while i < tokens.len() {
        match &tokens[i] {
            Token::Comment(value) => {
                ast.push(AstNode::Comment(value.clone()));
            }

            Token::SceneHead(scene_head) => {
                ast.push(new_scene(scene_head, &tokens, &mut i));
            }

            // New Function or Variable declaration or reference
            Token::VarDeclaration(id) => {
                ast.push(
                    new_variable(
                        *id, 
                        &tokens, 
                        &mut i, 
                        attributes.contains(&Attribute::Exported))
                );
            }

            Token::Export => {
                attributes.push(Attribute::Exported);
            }

            Token::Reference(var_index) => {
                ast.push(
                    create_reference(&tokens, var_index)
                );
            }

            Token::Title => {
                i += 1;
                match &tokens[i] {
                    Token::StringLiteral(value) => {
                        ast.push(AstNode::Title(value.clone()));
                    }
                    _ => {
                        ast.push(AstNode::Error(
                            "Title must have a valid string as a argument".to_string(),
                        ));
                    }
                }
            }

            Token::Date => {
                i += 1;
                match &tokens[i] {
                    Token::StringLiteral(value) => {
                        ast.push(AstNode::Date(value.clone()));
                    }
                    _ => {
                        ast.push(AstNode::Error(
                            "Date must have a valid string as a argument".to_string(),
                        ));
                    }
                }
            }

            Token::Newline | Token::Empty | Token::ModuleStart(_) => {
                // Do nothing for now
            }

            Token::Print => {
                i += 1;
                ast.push(AstNode::Print(Box::new(create_expression(&tokens, &mut i, false))));
            }

            Token::EOF => {
                break;
            }

            // Or stuff that hasn't been implemented yet
            _ => {
                ast.push(AstNode::Error(format!("Compiler Error: Token not recognised by AST parser when creating AST: {:?}", &tokens[i]).to_string()));
            }
        }

        i += 1;
    }

    (ast, i)
}

// CAN RETURN:
// VarDeclaration, Const, Error, Function, Tuple
fn new_variable(name: usize, tokens: &Vec<Token>, i: &mut usize, is_exported: bool) -> AstNode {
    let attribute;

    *i += 1;
    match &tokens[*i] {
        &Token::AssignConstant => {
            attribute = Attribute::Constant;
        }
        &Token::Assign => {
            attribute = Attribute::Mutable;
        }
        &Token::AssignComptime => {
            attribute = Attribute::Comptime;
        }
        &Token::AssignComptimeConstant => {
            attribute = Attribute::ComptimeConstant;
        }

        &Token::Comma => {
            // TO DO: Multiple assignments
            attribute = Attribute::Constant;
        }

        // Uninitialised variable
        &Token::Newline => {
            return AstNode::VarDeclaration(name, Box::new(AstNode::Empty), is_exported);
        }
        _ => {
            return AstNode::Error("Expected ':' or '=' after variable name for initialising. Variable does not yet exsist".to_string());
        }
    }

    // Get assigned values
    // Can also be function args
    *i += 1;

    // Check if array/struct/choice/scene
    match &tokens[*i] {
        Token::OpenScope => {
            match attribute {
                Attribute::Constant => {
                    return AstNode::Struct(
                        name, 
                        Box::new(new_array(tokens, i)), 
                        is_exported
                    )
                }
                Attribute::Mutable => {
                    return AstNode::VarDeclaration(
                        name, 
                        Box::new(new_array(tokens, i)), 
                        is_exported
                    )
                }
                _ => {
                    return AstNode::Error("Invalid assignment declaration for collection - possibly not supported yet?".to_string());
                }
            }
        }
        Token::SceneHead(scene_head) => {
            match attribute {
                Attribute::Constant => {
                    return AstNode::Const(
                        name, 
                        Box::new(new_scene(scene_head, tokens, i)), 
                        is_exported
                    )
                }
                Attribute::Mutable => {
                    return AstNode::VarDeclaration(
                        name, 
                        Box::new(new_scene(scene_head, tokens, i)), 
                        is_exported
                    )
                }
                _ => {
                    return AstNode::Error("Invalid assignment declaration for scene - possibly not supported yet?".to_string());
                }
            }

        }
        _ => {
            *i -= 1;
        }
    }

    let mut data_type = &DataType::Inferred;
    // Can be a collection, expression, literal or empty tuple
    let parsed_expr = create_expression(tokens, i, false);

    // create_expression does not move the token index past the closing token so it is incremented past it here
    *i += 1;

    // Check if the variable is a function, prototype, choice, has a type declaration or an exsisting choice/prototype type
    match &tokens[*i] {
        Token::Arrow => {
            return new_function(name, parsed_expr, tokens, i, is_exported);
        }
        Token::TypeKeyword(type_declaration) => {
            data_type = type_declaration;
        }
        _ => {
            *i -= 1;
        }
    }

    // Check if a type of collection has been created
    // Or whether it is a literal or expression
    // If the expression is an empty expression when the variable is NOT a function, return an error
    match parsed_expr {
        AstNode::Expression(_) => {
            let evaluated_expression = eval_expression(parsed_expr, tokens, data_type);
            return create_var_node(attribute, name, evaluated_expression, is_exported)
        }
        AstNode::Literal(_) => {
            return create_var_node(attribute, name, parsed_expr, is_exported)
        }
        AstNode::Tuple(items) => {
            return AstNode::Tuple(items);
        }
        // AstNode::Collection(items, collection_type) => {
            
        // }
        AstNode::Empty => {
            return AstNode::Error("Invalid expression for variable assignment".to_string());
        }
        AstNode::Error(_) => {
            return AstNode::Error("Invalid expression for variable assignment".to_string());
        }
        _ => {}
    }

    AstNode::VarDeclaration(name, Box::new(parsed_expr), is_exported)
    // AstNode::Error("Invalid variable assignment".to_string())
}

// Called from new_variable
fn new_function(name: usize, args: AstNode, tokens: &Vec<Token>, i: &mut usize, is_exported: bool) -> AstNode {
    let function_body = Vec::new();

    // Check 
    *i += 1;

    if &tokens[*i] != &Token::CloseScope {
        return AstNode::Error("Expected '(' for function args".to_string());
    }

    *i += 1;

    // TODO - Get function body

    AstNode::Function(name.clone(), Box::new(args), function_body, is_exported)
}

fn create_reference(tokens: &Vec<Token>, var_index: &usize) -> AstNode {

    // Should never be out of bounds right?
    match &tokens[var_index + 1] {
        Token::Assign => {
            return AstNode::VarReference(*var_index);
        }
        Token::AssignConstant => {
            return AstNode::ConstReference(*var_index);
        }
        _ => {
            return AstNode::Error("Expected variable or reference after '&'".to_string());
        }
    }
}

fn create_var_node(attribute: Attribute, var_name: usize, var_value: AstNode, is_exported: bool) -> AstNode {
    match attribute {
        Attribute::Constant => {
            return AstNode::Const(var_name, Box::new(var_value), is_exported);
        }
        Attribute::Mutable => {
            return AstNode::VarDeclaration(var_name, Box::new(var_value), is_exported);
        }
        _ => {
            return AstNode::Error("Invalid assignment declaration - possibly not supported yet?".to_string());
        }
    }
}
/*
match &tokens[*i] {
    // Infer type (CONSTANT VARIABLE)
    Token::Initialise => {}

    // Infer type (MUTABLE VARIABLE)
    Token::Assign => {
        var_is_const = false;
    }

    // Explicit Type Declarations
    Token::TypeInt => {
        type_declaration = DataType::Int;
    }
    Token::TypeFloat => {
        type_declaration = DataType::Float;
    }
    Token::TypeString => {
        type_declaration = DataType::String;
    }
    Token::TypeRune => {
        type_declaration = DataType::Rune;
    }

    // Function with implicit return type
    Token::OpenParenthesis => return new_function(tokens, i),

    _ => {
        return AstNode::Error(
            "Expected either type definition or another ':' or '=' for initialising"
                .to_string(),
        )
    }
}
*/
