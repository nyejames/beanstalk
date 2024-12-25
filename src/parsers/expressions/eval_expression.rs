use super::constant_folding::{logical_constant_fold, math_constant_fold};
use crate::{bs_types::DataType, parsers::ast_nodes::{AstNode, Arg}, CompileError, Token};
use colour::red_ln;

// This function takes in an Expression node or Collection of expressions that has a Vec of Nodes to evaluate
// And evaluates everything possible at compile time (Constant Folding)
// If it returns a literal, then everything was evaluated at compile time
// Otherwise it will return an EvaluatedExpression, which has a strict type and will be evaluated at runtime
pub fn evaluate_expression(
    expr: AstNode,
    type_declaration: &DataType,
    ast: &Vec<AstNode>,
) -> Result<AstNode, CompileError> {
    let mut current_type = type_declaration.to_owned();
    let mut simplified_expression = Vec::new();
    let mut runtime_nodes: usize = 0;

    // SHUNTING YARD ALGORITHM
    let mut output_stack: Vec<AstNode> = Vec::new();
    let mut operators_stack: Vec<AstNode> = Vec::new();
    match expr {
        AstNode::Expression(e, line_number) => {
            for ref node in e {
                match node {
                    AstNode::Expression(nested_e, nested_line_number) => {
                        simplified_expression.push(evaluate_expression(
                            AstNode::Expression(nested_e.to_owned(), nested_line_number.to_owned()),
                            type_declaration,
                            ast,
                        )?);
                    }
                    AstNode::Literal(token, line_number) => match token {
                        Token::FloatLiteral(value) => {
                            if current_type == DataType::CoerseToString {
                                simplified_expression.push(
                                    AstNode::Literal(Token::StringLiteral(value.to_string()), *line_number)
                                );
                                
                                continue;
                            }
                            
                            output_stack.push(node.to_owned());
                            if current_type == DataType::Inferred {
                                current_type = DataType::Float;
                            }
                        }
                        
                        Token::IntLiteral(value) => {
                            if current_type == DataType::CoerseToString {
                                simplified_expression.push(
                                    AstNode::Literal(Token::StringLiteral(value.to_string()), *line_number)
                                );

                                continue;
                            }
                            
                            output_stack.push(node.to_owned());
                            if current_type == DataType::Inferred {
                                current_type = DataType::Int;
                            }
                        }
                        
                        Token::StringLiteral(_) => {
                            simplified_expression.push(node.to_owned());
                            if current_type == DataType::Inferred {
                                current_type = DataType::String;
                            }
                        }
                        
                        Token::BoolLiteral(_) => {
                            output_stack.push(node.to_owned());
                            if current_type == DataType::Inferred {
                                current_type = DataType::Bool;
                            }
                        }
                        
                        _ => {
                            return Err( CompileError {
                                msg: "unsupported literal type found in expression".to_string(),
                                line_number: line_number.to_owned(),
                            });
                        }
                    },

                    AstNode::ConstReference(_, data_type, line_number) => {
                        if current_type == DataType::Inferred {
                            current_type = data_type.to_owned();
                        }

                        match current_type {
                            DataType::Float | DataType::Int | DataType::Bool => {
                                output_stack.push(node.to_owned());
                            }
                            DataType::String | DataType::CoerseToString => {
                                simplified_expression.push(node.to_owned());
                            }
                            _ => {
                                return Err( CompileError {
                                    msg: format!(
                                        "unsupported data type for constants in expressions: {:?}",
                                        current_type
                                    ),
                                    line_number: *line_number,
                                });
                            }
                        }
                    }

                    AstNode::FunctionCall(..) => {
                        // TODO
                    }

                    AstNode::VarReference(_, data_type, line_number)
                    | AstNode::TupleAccess(_, _, data_type, line_number)
                    | AstNode::CollectionAccess(_, _, data_type, line_number) => {
                        if current_type == DataType::Inferred {
                            current_type = data_type.to_owned();
                        }

                        match current_type {
                            DataType::Float | DataType::Int | DataType::Bool => {
                                output_stack.push(node.to_owned());
                            }
                            DataType::String | DataType::CoerseToString => {
                                simplified_expression.push(node.to_owned());
                            }
                            _ => {
                                return Err( CompileError {
                                    msg: format!(
                                        "unsupported data type for variables in expressions: {:?}",
                                        current_type
                                    ),
                                    line_number: *line_number,
                                });
                            }
                        }

                        runtime_nodes += 1;
                    }

                    AstNode::BinaryOperator(op, precedence, _) => {
                        // If the current type is a string or scene, add operator is assumed.
                        if current_type == DataType::String || current_type == DataType::Scene {
                            if op != &Token::Add {
                                return Err( CompileError {
                                    msg: "Can only use the '+' operator to manipulate strings or scenes inside expressions".to_string(),
                                    line_number: line_number.to_owned(),
                                });
                            }
                            simplified_expression.push(node.to_owned());
                            continue;
                        }

                        if current_type == DataType::CoerseToString {
                            simplified_expression.push(node.to_owned());
                        }

                        if current_type == DataType::Bool {
                            if *op != Token::Or || *op != Token::And {
                                return Err( CompileError {
                                    msg: "Can only use 'or' and 'and' operators with booleans"
                                        .to_string(),
                                    line_number: line_number.to_owned(),
                                });
                            }
                            operators_stack.push(node.to_owned());
                        }

                        if operators_stack.last().is_some_and(|x| match x {
                            AstNode::BinaryOperator(_, p, _) => p >= &precedence,
                            _ => false,
                        }) {
                            output_stack.push(operators_stack.pop().unwrap());
                        }

                        operators_stack.push(node.to_owned());
                    }

                    AstNode::Scene(..) => {
                        if current_type == DataType::Inferred {
                            current_type = DataType::Scene;
                        }

                        if current_type != DataType::Scene {
                            return Err( CompileError {
                                msg: "Scene used in non-scene expression".to_string(),
                                line_number: line_number.to_owned(),
                            });
                        }
                        output_stack.push(node.to_owned());
                    }

                    _ => {
                        return Err( CompileError {
                            msg: "unsupported AST node found in expression".to_string(),
                            line_number: line_number.to_owned(),
                        });
                    }
                }
            }
        }

        AstNode::Tuple(e, line_number) => {
            let mut simplified_references = Vec::new();
            for arg in e {
                match arg.value {
                    AstNode::Expression(e, line_number) => {
                        let simplified_ref = Arg {
                            value: evaluate_expression(
                                AstNode::Expression(e.to_owned(), line_number.to_owned()),
                                type_declaration,
                                ast,
                            )?,
                            data_type: arg.data_type,
                            name: arg.name,
                        };
                        simplified_references.push(simplified_ref)
                    },
                    _ => {
                        simplified_references.push(arg);
                    }
                }
            }

            return Ok(AstNode::Tuple(
                simplified_references,
                line_number
            ));
        }
        _ => {
            return Err( CompileError {
                msg: format!("Compiler Bug: No Expression to Evaluate - eval expression passed wrong AST node: {:?}", expr),
                line_number: 0,
            });
        }
    }

    // If nothing to evaluate at compile time, just one value, return that value
    if simplified_expression.len() == 1 {
        return Ok(simplified_expression[0].clone());
    }

    // LOGICAL EXPRESSIONS
    if current_type == DataType::Bool {
        for operator in operators_stack {
            output_stack.push(operator);
        }

        return logical_constant_fold(output_stack, current_type);
    }

    // SCENE EXPRESSIONS
    // If constant scene expression, combine the scenes together and return the new scene
    if current_type == DataType::Scene && runtime_nodes == 0 {
        return Ok(concat_scene(&mut simplified_expression));
    }

    // STRING EXPRESSIONS
    // If the expression is a constant string, combine and return a string
    if current_type == DataType::String && runtime_nodes == 0 {
        return Ok(concat_strings(&mut simplified_expression));
    }

    // Scene Head Coerse to String
    if current_type == DataType::CoerseToString {
        // TODO - line number
        return Ok(AstNode::RuntimeExpression(simplified_expression, current_type, 0));
    }

    // MATHS EXPRESSIONS
    // Push everything into the stack, is now in RPN notation
    for operator in operators_stack {
        output_stack.push(operator);
    }

    // Evaluate all constants in the maths expression
    math_constant_fold(output_stack, current_type)
}

fn concat_scene(simplified_expression: &mut Vec<AstNode>) -> AstNode {
    let mut new_scene: AstNode = AstNode::Scene(Vec::new(), Vec::new(), Vec::new(), Vec::new(), 0);

    for node in simplified_expression {
        match node {
            AstNode::Scene(vec1, vec2, vec3, vec4, line_number) => match new_scene {
                AstNode::Scene(ref mut v1, ref mut v2, ref mut v3, ref mut v4, ref mut new_line_number) => {
                    v1.append(vec1);
                    v2.append(vec2);
                    v3.append(vec3);
                    v4.append(vec4);
                    *new_line_number = line_number.to_owned();
                }
                _ => {
                    return AstNode::Error(
                        "Compiler Bug: Cannot evaluate scene expression at compile time. Compiler should be creating a runtime scene expression".to_string(),
                        0,
                    );
                }
            },
            _ => {
                return AstNode::Error(
                    "Compiler Bug: Cannot evaluate scene expression at compile time. Compiler should be creating a runtime scene expression".to_string(),
                    0,
                );
            }
        }
    }

    new_scene
}

// Concat strings at COMPILE TIME ONLY
fn concat_strings(simplified_expression: &mut Vec<AstNode>) -> AstNode {
    let mut new_string = String::new();
    let mut previous_node_is_plus = false;
    let mut first_line_number = 0;

    for node in simplified_expression {
        match node {
            AstNode::Literal(Token::StringLiteral(string), line_number) => {
                if new_string.is_empty() {
                    first_line_number = line_number.to_owned();
                }

                if previous_node_is_plus || new_string.is_empty() {
                    new_string.push_str(string);
                    previous_node_is_plus = false;
                } else {
                    // Syntax error, must have a + operator between strings when concatinating
                    red_ln!(
                        "Syntax Error: Must have a + operator between strings when concatinating"
                    );
                }
            }
            AstNode::BinaryOperator(..) => {
                // Should always be a plus operator, this is enforced in the eval_expression function
                previous_node_is_plus = true;
            }
            _ => {
                return AstNode::Error(
                    "Compiler Bug: Cannot evaluate string expression at compile time. Compiler should be creating a runtime string expression".to_string(),
                    0,
                );
            }
        }
    }

    AstNode::Literal(Token::StringLiteral(new_string), first_line_number)
}
