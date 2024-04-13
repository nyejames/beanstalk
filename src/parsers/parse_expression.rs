use super::ast::AstNode;
use crate::{bs_types::DataType, Token};

// Creates an expression node from a list of tokens
// Will eventually also evaluate the expression at compile time to simplify the AST
pub fn create_expression(tokens: &Vec<Token>, i: &mut usize) -> AstNode {
    let mut expression = Vec::new();

    // Check if value is wrapped in brackets and move on until first value is found
    let mut bracket_nesting: i32 = 0;
    while &tokens[*i] == &Token::OpenParenthesis {
        bracket_nesting += 1;
        *i += 1;
    }

    // Find the end of the expression and check if it is assigned a data type at the end
    let mut expression_end = *i;
    if bracket_nesting > 0 {
        // Find the last closing bracket and end expression there
        let mut total_open_brackets = bracket_nesting;
        while &expression_end < &tokens.len() {
            if &tokens[expression_end] == &Token::OpenParenthesis {
                total_open_brackets += 1;
            } else if &tokens[expression_end] == &Token::CloseParenthesis {
                if total_open_brackets < 1 {
                    break;
                }
                total_open_brackets -= 1;
            }

            expression_end += 1;
        }
    } else {
        // Find the next newline, comma or final closing bracket and end expression there
        while &expression_end < &tokens.len() {
            match &tokens[expression_end] {
                Token::Newline | Token::Comma | Token::SceneClose(_) | Token::CloseParenthesis => {
                    break;
                }
                _ => {
                    expression_end += 1;
                }
            }
        }
    }

    // Get the data type of the expression if there is one after the expression
    let mut data_type = &DataType::Inferred;
    if expression_end + 1 < tokens.len() {
        match &tokens[expression_end + 1] {
            Token::TypeKeyword(type_keyword) => data_type = &type_keyword,
            _ => {}
        };
    }

    // Loop through the expression and create the AST nodes
    // Figure out the type from the data
    // If the type does not match the assigned datatype then throw an error
    while let Some(token) = tokens.get(*i) {
        match token {
            // Conditions that close the expression
            Token::Newline => {
                if bracket_nesting == 0 {
                    break;
                }
            }
            Token::EOF | Token::Comma | Token::CloseCollection | Token::SceneClose(_) => {
                if bracket_nesting == 0 {
                    break;
                }
                return AstNode::Error(
                    "Not enough closing parenthesis for expression. Need more ')'!".to_string(),
                );
            }
            Token::CloseParenthesis => {
                if bracket_nesting > 1 {
                    bracket_nesting -= 1;
                } else {
                    *i += 1;
                    break;
                }
            }

            // Check if name is a reference to another variable or function call
            Token::Variable(_) => {
                expression.push(AstNode::Error("NOT IMPLIMENTED YET - GETTING VARIABLE. Variable reference not defined. Maybe you're using a variable that has not yet been declared?".to_string()));
            }

            // Check if is a literal
            Token::IntLiteral(int) => {
                if data_type == &DataType::Inferred {
                    data_type = &DataType::Int;
                }

                if data_type != &DataType::Int {
                    return AstNode::Error("Error Mixing types. You must explicitly convert types to use them in the same expression".to_string());
                }

                expression.push(AstNode::Literal(Token::IntLiteral(*int)));
            }
            Token::StringLiteral(string) => {
                if data_type == &DataType::Inferred {
                    data_type = &DataType::String;
                }

                if data_type != &DataType::String {
                    return AstNode::Error("Error Mixing types. You must explicitly convert types to use them in the same expression".to_string());
                }

                expression.push(AstNode::Literal(Token::StringLiteral(string.clone())));
            }
            Token::FloatLiteral(float) => {
                if data_type == &DataType::Inferred {
                    data_type = &DataType::Float;
                }

                if data_type != &DataType::Float {
                    return AstNode::Error("Error Mixing types. You must explicitly convert types to use them in the same expression".to_string());
                }

                expression.push(AstNode::Literal(Token::FloatLiteral(*float)));
            }

            // OPERATORS
            // Assign precedence

            // UNARY OPERATORS
            Token::Negative => {
                expression.push(AstNode::UnaryOperator(Token::Negative, 10));
            }
            Token::Exponent => {
                expression.push(AstNode::UnaryOperator(Token::Exponent, 8));
            }

            // BINARY OPERATORS
            Token::Add => {
                expression.push(AstNode::BinaryOperator(Token::Add, 6));
            }
            Token::Subtract => {
                expression.push(AstNode::BinaryOperator(Token::Subtract, 6));
            }
            Token::Multiply => {
                expression.push(AstNode::BinaryOperator(Token::Multiply, 7));
            }
            Token::Divide => {
                expression.push(AstNode::BinaryOperator(Token::Divide, 7));
            }
            Token::AddAssign => {
                expression.push(AstNode::BinaryOperator(Token::AddAssign, 6));
            }
            Token::SubtractAssign => {
                expression.push(AstNode::BinaryOperator(Token::SubtractAssign, 6));
            }
            Token::Equal => {
                expression.push(AstNode::BinaryOperator(Token::Equal, 5));
            }
            Token::LessThan => {
                expression.push(AstNode::BinaryOperator(Token::LessThan, 5));
            }
            Token::LessThanOrEqual => {
                expression.push(AstNode::BinaryOperator(Token::LessThanOrEqual, 5));
            }
            Token::GreaterThan => {
                expression.push(AstNode::BinaryOperator(Token::GreaterThan, 5));
            }
            Token::GreaterThanOrEqual => {
                expression.push(AstNode::BinaryOperator(Token::GreaterThanOrEqual, 5));
            }
            Token::Modulus => {
                expression.push(AstNode::BinaryOperator(Token::Modulus, 7));
            }
            Token::Remainder => {
                expression.push(AstNode::BinaryOperator(Token::Remainder, 7));
            }
            Token::Root => {
                expression.push(AstNode::BinaryOperator(Token::Root, 8));
            }
            Token::ExponentAssign => {
                expression.push(AstNode::BinaryOperator(Token::ExponentAssign, 8));
            }
            Token::MultiplyAssign => {
                expression.push(AstNode::BinaryOperator(Token::MultiplyAssign, 7));
            }
            Token::DivideAssign => {
                expression.push(AstNode::BinaryOperator(Token::DivideAssign, 7));
            }
            Token::ModulusAssign => {
                expression.push(AstNode::BinaryOperator(Token::ModulusAssign, 7));
            }
            Token::RootAssign => {
                expression.push(AstNode::BinaryOperator(Token::RootAssign, 8));
            }
            Token::RemainderAssign => {
                expression.push(AstNode::BinaryOperator(Token::RemainderAssign, 7));
            }

            // LOGICAL OPERATORS
            Token::And => {
                expression.push(AstNode::BinaryOperator(Token::And, 4));
            }
            Token::Or => {
                expression.push(AstNode::BinaryOperator(Token::Or, 3));
            }

            _ => {
                expression.push(AstNode::Error(
                    "Invalid Expression, must be assigned wih a valid datatype".to_string(),
                ));
            }
        }

        *i += 1;
    }

    // TO DO: Evaluate the expression at compile time and return the result
    // THIS WILL BE DONE IN EVAL_EXPRESSION FUNCTION
    AstNode::Expression(expression, data_type.clone())
}

// This function takes in an Expression node that has a Vec of Nodes to evaluate
// And evaluates everything possible at compile time (Constant Folding)
// If it returns a literal, then everything was evaluated at compile time
// Otherwise it will return an expression, which will need runtime evaluation
pub fn _eval_expression(expr: AstNode, tokens: &Vec<Token>) -> AstNode {
    let mut result_type = DataType::Inferred;

    let mut constants_queue: Vec<AstNode> = Vec::new();
    let mut operator_stack: Vec<AstNode> = Vec::new();

    match expr {
        AstNode::Expression(e, data_type) => {
            for node in e {
                match node {
                    AstNode::Literal(v) => {
                        constants_queue.push(AstNode::Literal(v));
                    }
                    AstNode::BinaryOperator(op, precedence) => {
                        if constants_queue.is_empty() { return AstNode::Error("Not enough operands for binary operator".to_string()); }
                        
                        if operator_stack.is_empty() {
                            operator_stack.push(AstNode::BinaryOperator(op, precedence));
                        } else {
                            let top_op = operator_stack.pop().unwrap();
                            match top_op {
                                AstNode::BinaryOperator(top_op, top_precedence) => {
                                    if top_precedence > precedence {
                                        operator_stack.push(AstNode::BinaryOperator(top_op, top_precedence));
                                        operator_stack.push(AstNode::BinaryOperator(op, precedence));
                                    } else {
                                        operator_stack.push(AstNode::BinaryOperator(top_op, top_precedence));
                                        operator_stack.push(AstNode::BinaryOperator(op, precedence));
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                    AstNode::ConstReference(value) => {
                        // Get the value of the constant and push it to the constants queue
                        match &tokens[value] {
                            Token::IntLiteral(int) => {
                                if data_type == DataType::Inferred {result_type = DataType::Int;}
                                else if data_type != DataType::Int {
                                    return AstNode::Error("Error Mixing types. You must explicitly convert types to use them in the same expression".to_string());
                                }
                                constants_queue.push(AstNode::Literal(Token::IntLiteral(*int)));
                            }
                            Token::FloatLiteral(float) => {
                                if data_type == DataType::Inferred {result_type = DataType::Int;}
                                else if data_type != DataType::Int {
                                    return AstNode::Error("Error Mixing types. You must explicitly convert types to use them in the same expression".to_string());
                                }
                                constants_queue.push(AstNode::Literal(Token::FloatLiteral(*float)));
                            }
                            _ => {
                                return AstNode::Error("Invalid Constant Reference".to_string());
                            }
                        }
                    }

                    _=> {}
                }
            }
        }
        _ => {
            return AstNode::Error("No Expression to Evaluate".to_string());
        }
    }

    let simplified_expression = Vec::new();

    AstNode::Expression(simplified_expression, result_type)
}

/*
while there are tokens to be read:
    read a token
    if the token is:

    - a number:
        put it into the output queue


    - a function:
        push it onto the operator stack


    - an operator o1:
        while (
            there is an operator o2 at the top of the operator stack which is not a left parenthesis,
            and (o2 has greater precedence than o1 or (o1 and o2 have the same precedence and o1 is left-associative))
        ):
            pop o2 from the operator stack into the output queue
        push o1 onto the operator stack


    - a ",":
        while the operator at the top of the operator stack is not a left parenthesis:
             pop the operator from the operator stack into the output queue


    - a left parenthesis (i.e. "("):
        push it onto the operator stack


    - a right parenthesis (i.e. ")"):
        while the operator at the top of the operator stack is not a left parenthesis:
            {assert the operator stack is not empty}
            /* If the stack runs out without finding a left parenthesis, then there are mismatched parentheses. */
            pop the operator from the operator stack into the output queue
        {assert there is a left parenthesis at the top of the operator stack}
        pop the left parenthesis from the operator stack and discard it
        if there is a function token at the top of the operator stack, then:
            pop the function from the operator stack into the output queue

            After the while loop, pop the remaining items from the operator stack into the output queue.

while there are tokens on the operator stack:
If the operator token on the top of the stack is a parenthesis, then there are mismatched parentheses.
    {assert the operator on top of the stack is not a (left) parenthesis}
    pop the operator from the operator stack onto the output queue



*/
