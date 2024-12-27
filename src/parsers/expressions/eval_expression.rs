use super::constant_folding::{logical_constant_fold, math_constant_fold};
use crate::{bs_types::DataType, parsers::ast_nodes::AstNode, CompileError, Token};
use crate::parsers::ast_nodes::{NodeInfo, Value};

// This function will turn a series of ast nodes into a Value enum
// A Value enum can also be a runtime expression which contains a series of nodes
// It will fold constants (not working yet) down to a single Value if possible
pub fn evaluate_expression(
    expr: AstNode,
    type_declaration: &DataType,
    ast: &Vec<AstNode>,
) -> Result<Value, CompileError> {
    let mut current_type = type_declaration.to_owned();
    let mut simplified_expression: Vec<AstNode> = Vec::new();
    let line_number ;

    // SHUNTING YARD ALGORITHM
    let mut output_stack: Vec<AstNode> = Vec::new();
    let mut operators_stack: Vec<AstNode> = Vec::new();
    match expr {
        AstNode::Expression(e, line) => {
            line_number = line.to_owned();

            for ref node in e {
                match node {
                    AstNode::Expression(nested_e, nested_line_number) => {
                        simplified_expression.push(AstNode::Literal(evaluate_expression(
                            AstNode::Expression(nested_e.to_owned(), nested_line_number.to_owned()),
                            type_declaration,
                            ast,
                        )?, nested_line_number.to_owned()));
                    }

                    AstNode::Literal(value, _) => {
                        if current_type == DataType::CoerceToString || current_type == DataType::String {
                            simplified_expression.push(
                                node.to_owned()
                            );
                        } else {
                            output_stack.push(node.to_owned());
                        }

                        if current_type == DataType::Inferred {
                            current_type = value.get_type();
                        }
                    },

                    AstNode::FunctionCall(..) => {
                        // TODO
                    }

                    AstNode::BinaryOperator(op, precedence, _) => {
                        // If the current type is a string or scene, add operator is assumed.
                        if current_type == DataType::String || current_type == DataType::Scene {
                            if op != &Token::Add {
                                return Err( CompileError {
                                    msg: "Can only use the '+' operator to manipulate strings or scenes inside expressions".to_string(),
                                    line_number: line,
                                });
                            }
                            simplified_expression.push(node.to_owned());
                            continue;
                        }

                        if current_type == DataType::CoerceToString {
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

                    _ => {
                        return Err( CompileError {
                            msg: "unsupported AST node found in expression".to_string(),
                            line_number: line_number.to_owned(),
                        });
                    }
                }
            }
        }

        // Don't evaluate tuples
        // Each element inside should already be evaluated
        AstNode::Tuple(e, _) => {
            return Ok(Value::Tuple(e.to_owned()));
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
        return Ok(simplified_expression[0].get_value());
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
    if current_type == DataType::Scene {
        return concat_scene(&mut simplified_expression, line_number);
    }

    // STRING EXPRESSIONS
    // If the expression is a constant string, combine and return a string
    if current_type == DataType::String {
        return concat_strings(&mut simplified_expression, line_number);
    }

    // Scene Head Coerce to String
    if current_type == DataType::CoerceToString {
        // TODO - line number
        return Ok(Value::Runtime(simplified_expression, current_type));
    }

    // MATHS EXPRESSIONS
    // Push everything into the stack, is now in RPN notation
    for operator in operators_stack {
        output_stack.push(operator);
    }

    // Evaluate all constants in the maths expression
    math_constant_fold(output_stack, current_type)
}

// TODO - needs to check what can be concatenated at compile time
// Everything else should be left for runtime
fn concat_scene(simplified_expression: &mut Vec<AstNode>, line_number: u32) -> Result<Value, CompileError> {
    let mut nodes = Vec::new();
    let mut tags = Vec::new();
    let mut styles = Vec::new();
    let mut actions = Vec::new();

    for node in simplified_expression {
        match node.get_value() {
            Value::Scene(ref mut vec1, ref mut vec2, ref mut vec3, ref mut vec4) => {
                nodes.append(vec1);
                tags.append(vec2);
                styles.append(vec3);
                actions.append(vec4);
            },
            _ => {
                return Err(CompileError {
                    msg: "Non-scene value found in scene expression (you can only concatenate scenes with other scenes)".to_string(),
                    line_number,
                });
            }
        }
    }

    Ok(Value::Scene(nodes, tags, styles, actions))
}

// TODO - needs to check what can be concatenated at compile time
// Everything else should be left for runtime
fn concat_strings(simplified_expression: &mut Vec<AstNode>, line_number: u32) -> Result<Value, CompileError> {
    let mut new_string = String::new();
    let mut previous_node_is_plus = false;

    for node in simplified_expression {
        match node.get_value() {
            Value::String(ref string) => {
                if previous_node_is_plus || new_string.is_empty() {
                    new_string.push_str(string);
                    previous_node_is_plus = false;
                } else {
                    // Syntax error, must have a + operator between strings when concatenating
                    return Err(CompileError {
                        msg: "Syntax Error: Must have a + operator between strings when concatenating".to_string(),
                        line_number: line_number.to_owned(),
                    });
                }
            }

            // TODO: - does there need to be runtime stuff here for strings?
            Value::Runtime(_, _) => {
                return Err(CompileError {
                    msg: "Compiler Bug: Runtime expressions not supported yet in string expression (concat strings - eval expression)".to_string(),
                    line_number: line_number.to_owned(),
                });
            }

            _ => {
                return Err(CompileError {
                    msg: "Compiler Bug: Non-string (or runtime string expression) used in string expression (concat strings - eval expression)".to_string(),
                    line_number: line_number.to_owned(),
                });
            }
        }
    }

    Ok(Value::String(new_string))
}
