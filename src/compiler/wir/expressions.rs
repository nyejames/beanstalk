//! # Expression Transformation Module
//!
//! This module contains functions for converting AST expressions to WIR rvalues
//! and operands. It handles runtime expression evaluation with RPN, manages
//! expression stack and temporary variables, and performs type inference for
//! binary operations.

// Import context types from context module
use crate::compiler::wir::context::WirTransformContext;

// Import WIR types
use crate::compiler::wir::place::Place;
use crate::compiler::wir::wir_nodes::{BinOp, Constant, Operand, Rvalue, Statement};

// Core compiler imports
use crate::compiler::{
    compiler_errors::CompileError,
    datatypes::DataType,
    parsers::{
        ast_nodes::{AstNode, NodeKind},
        expressions::expression::{Expression, ExpressionKind},
    },
};
use crate::compiler::parsers::tokenizer::tokens::TextLocation;
// Error handling macros
use crate::{return_compiler_error, return_wir_transformation_error};

/// Convert an AST expression to a WIR rvalue with supporting statements
///
/// This is the core expression transformation function that handles all expression
/// types and converts them to WIR rvalues. It may generate supporting statements
/// for complex expressions that require temporary variables or multiple operations.
///
/// # Parameters
///
/// - `expression`: AST expression to convert
/// - `location`: Source location for error reporting
/// - `context`: Transformation context for variable lookup and temporary allocation
///
/// # Returns
///
/// - `Ok((statements, rvalue))`: Supporting statements and the resulting rvalue
/// - `Err(CompileError)`: Transformation error with source location
///
/// # Expression Types Handled
///
/// - **Literals**: Int, Float, Bool, String constants
/// - **Variables**: Variable references with proper place lookup
/// - **Templates**: Beanstalk template expressions
/// - **Runtime Expressions**: Complex expressions requiring RPN evaluation
///
/// # Note
///
/// The returned statements must be executed before using the rvalue to ensure
/// all temporary variables and intermediate results are properly computed.
pub fn expression_to_rvalue_with_context(
    expression: &Expression,
    location: &TextLocation,
    context: &mut WirTransformContext,
    string_table: &mut crate::compiler::string_interning::StringTable,
) -> Result<(Vec<Statement>, Rvalue), CompileError> {
    match &expression.kind {
        ExpressionKind::Int(value) => Ok((
            vec![],
            Rvalue::Use(Operand::Constant(Constant::I32(*value as i32))),
        )),
        ExpressionKind::Float(value) => Ok((
            vec![],
            Rvalue::Use(Operand::Constant(Constant::F32(*value as f32))),
        )),
        ExpressionKind::Bool(value) => Ok((
            vec![],
            Rvalue::Use(Operand::Constant(Constant::Bool(*value))),
        )),
        ExpressionKind::StringSlice(value) => {
            // value is already an InternedString from the AST
            Ok((
                vec![],
                Rvalue::Use(Operand::Constant(Constant::String(*value))),
            ))
        },
        ExpressionKind::Reference(name) => {
            let var_name = string_table.resolve(*name);
            let variable_place = context
                .lookup_variable(var_name)
                .ok_or_else(|| {
                    CompileError::new_rule_error(
                        format!("Undefined variable '{}'", name),
                        location.clone(),
                    )
                })?
                .clone();
            Ok((vec![], Rvalue::Use(Operand::Copy(variable_place))))
        }
        ExpressionKind::Template(template) => {
            // Use template transformation functions from templates module
            crate::compiler::wir::templates::transform_template_to_rvalue(
                template, location, context,
            )
        }
        ExpressionKind::Runtime(rpn_nodes) => {
            // Handle runtime expressions (RPN evaluation)
            evaluate_rpn_to_wir_statements(rpn_nodes, location, context, string_table)
        }
        _ => {
            return_wir_transformation_error!(
                location.clone(),
                "Expression kind {:?} not yet implemented in WIR transformation at {}:{}. This expression type needs to be added to the WIR lowering implementation.",
                expression.kind,
                location.start_pos.line_number,
                location.start_pos.char_column
            );
        }
    }
}

/// Convert an AST expression to a WIR operand with supporting statements
///
/// Similar to `expression_to_rvalue_with_context` but ensures the result is an
/// operand that can be used directly in other WIR constructs. For complex rvalues
/// that cannot be used as operands, this creates a temporary variable.
///
/// # Parameters
///
/// - `expression`: AST expression to convert
/// - `location`: Source location for error reporting  
/// - `context`: Transformation context for variable lookup and temporary allocation
///
/// # Returns
///
/// - `Ok((statements, operand))`: Supporting statements and the resulting operand
/// - `Err(CompileError)`: Transformation error with source location
///
/// # Operand Creation
///
/// - **Simple rvalues**: `Rvalue::Use(operand)` returns the operand directly
/// - **Complex rvalues**: Creates a temporary variable, assigns the rvalue to it,
///   and returns an operand that copies from the temporary
///
/// This ensures all results can be used as operands in function calls, binary
/// operations, and other contexts that require operands rather than rvalues.
pub fn expression_to_operand_with_context(
    expression: &Expression,
    location: &TextLocation,
    context: &mut WirTransformContext,
    string_table: &mut crate::compiler::string_interning::StringTable,
) -> Result<(Vec<Statement>, Operand), CompileError> {
    let (statements, rvalue) = expression_to_rvalue_with_context(expression, location, context, string_table)?;

    match rvalue {
        Rvalue::Use(operand) => Ok((statements, operand)),
        _ => {
            // For complex rvalues, create a temporary and return its operand
            let temp_place = context.create_temporary_place(&expression.data_type);
            let assign_statement = Statement::Assign {
                place: temp_place.clone(),
                rvalue,
            };
            let mut all_statements = statements;
            all_statements.push(assign_statement);
            Ok((all_statements, Operand::Copy(temp_place)))
        }
    }
}


/// Evaluate RPN (Reverse Polish Notation) expression to WIR statements
///
/// Processes a runtime expression that has been converted to RPN form during AST
/// construction. Uses a stack-based evaluation approach to handle complex expressions
/// with proper operator precedence and associativity.
///
/// # Parameters
///
/// - `rpn_nodes`: AST nodes in RPN order (operands followed by operators)
/// - `location`: Source location for error reporting
/// - `context`: Transformation context for temporary allocation
///
/// # Returns
///
/// - `Ok((statements, rvalue))`: Statements to evaluate the expression and final result
/// - `Err(CompileError)`: Evaluation error with source location
///
/// # RPN Evaluation Process
///
/// 1. **Operands**: Pushed onto evaluation stack
/// 2. **Operators**: Pop required operands, create binary operation, push result
/// 3. **Final Result**: Single operand remaining on stack becomes the rvalue
///
/// # Example
///
/// ```beanstalk
/// x + 2 * y  // Becomes RPN: [x, 2, y, *, +]
/// ```
///
/// Evaluation:
/// 1. Push x, push 2, push y
/// 2. Pop y and 2, multiply, push result
/// 3. Pop result and x, add, push final result
///
/// # Validation
///
/// - RPN nodes array must not be empty
/// - Stack must have exactly one operand at the end
/// - Binary operators must have at least 2 operands on stack
pub fn evaluate_rpn_to_wir_statements(
    rpn_nodes: &[AstNode],
    location: &TextLocation,
    context: &mut WirTransformContext,
    string_table: &mut crate::compiler::string_interning::StringTable,
) -> Result<(Vec<Statement>, Rvalue), CompileError> {
    // Validate input
    if rpn_nodes.is_empty() {
        return_compiler_error!(
            "Empty RPN expression at {}:{}. This indicates a bug in the AST-to-RPN conversion.",
            location.start_pos.line_number,
            location.start_pos.char_column
        );
    }
    
    // Pre-allocate vectors with estimated capacity to reduce reallocations
    // Assume ~1 statement per node and stack depth of ~nodes/2
    let mut statements = Vec::with_capacity(rpn_nodes.len());
    let mut operand_stack: Vec<Operand> = Vec::with_capacity(rpn_nodes.len() / 2 + 1);

    for node in rpn_nodes {
        match &node.kind {
            NodeKind::Expression(expr) => {
                // Convert expression to operand and push to stack
                let (expr_statements, operand) =
                    expression_to_operand_with_context(expr, &node.location, context, string_table)?;
                statements.extend(expr_statements);
                operand_stack.push(operand);
            }
            NodeKind::Operator(op) => {
                // Process binary operator
                if operand_stack.len() < 2 {
                    return_compiler_error!(
                        "Insufficient operands for binary operator at {}:{}. RPN evaluation stack has {} operands but needs 2. This indicates a bug in the AST-to-RPN conversion.",
                        node.location.start_pos.line_number,
                        node.location.start_pos.char_column,
                        operand_stack.len()
                    );
                }

                let rhs = operand_stack.pop().unwrap();
                let lhs = operand_stack.pop().unwrap();

                // Infer result type
                let lhs_type = operand_to_datatype(&lhs, context)?;
                let rhs_type = operand_to_datatype(&rhs, context)?;
                let result_type = infer_binary_operation_result_type(&lhs_type, &rhs_type, op)?;

                // Create temporary for result
                let result_place = context.create_temporary_place(&result_type);

                // Check if this is string concatenation
                use crate::compiler::parsers::expressions::expression::Operator;
                let rvalue = if matches!(op, Operator::Add) && 
                              (lhs_type == DataType::String || rhs_type == DataType::String) {
                    // String concatenation
                    Rvalue::StringConcat(lhs, rhs)
                } else {
                    // Regular binary operation
                    let wir_op = ast_operator_to_wir_binop(op)?;
                    Rvalue::BinaryOp(wir_op, lhs, rhs)
                };

                // Create operation statement
                statements.push(Statement::Assign {
                    place: result_place.clone(),
                    rvalue,
                });

                // Push result operand to stack
                operand_stack.push(Operand::Copy(result_place));
            }
            _ => {
                return_compiler_error!(
                    "Unexpected node type in RPN expression: {:?}. RPN expressions should only contain Expression and Operator nodes. This indicates a bug in the AST-to-RPN conversion.",
                    node.kind
                );
            }
        }
    }

    // The final result should be the only operand left on the stack
    if operand_stack.len() != 1 {
        return_compiler_error!(
            "Invalid RPN expression: expected 1 result operand on stack, got {}. This indicates a bug in the RPN evaluation or AST-to-RPN conversion.",
            operand_stack.len()
        );
    }

    let result_operand = operand_stack.pop().unwrap();
    Ok((statements, Rvalue::Use(result_operand)))
}

/// Convert operand to its data type
fn operand_to_datatype(
    operand: &Operand,
    _context: &WirTransformContext,
) -> Result<DataType, CompileError> {
    match operand {
        Operand::Copy(place) | Operand::Move(place) => {
            // Extract type from place - this is a simplified implementation
            match place {
                Place::Local { wasm_type, .. } => {
                    // Convert WasmType to DataType
                    match wasm_type {
                        crate::compiler::wir::place::WasmType::I32 => Ok(DataType::Int),
                        crate::compiler::wir::place::WasmType::F32 => Ok(DataType::Float),
                        crate::compiler::wir::place::WasmType::I64 => Ok(DataType::Int),
                        crate::compiler::wir::place::WasmType::F64 => Ok(DataType::Float),
                        crate::compiler::wir::place::WasmType::ExternRef => Ok(DataType::String), // External references default to string
                        crate::compiler::wir::place::WasmType::FuncRef => Ok(DataType::Int), // Function references default to int
                    }
                }
                Place::Global { wasm_type, .. } => {
                    // Convert WasmType to DataType
                    match wasm_type {
                        crate::compiler::wir::place::WasmType::I32 => Ok(DataType::Int),
                        crate::compiler::wir::place::WasmType::F32 => Ok(DataType::Float),
                        crate::compiler::wir::place::WasmType::I64 => Ok(DataType::Int),
                        crate::compiler::wir::place::WasmType::F64 => Ok(DataType::Float),
                        crate::compiler::wir::place::WasmType::ExternRef => Ok(DataType::String), // External references default to string
                        crate::compiler::wir::place::WasmType::FuncRef => Ok(DataType::Int), // Function references default to int
                    }
                }
                _ => Ok(DataType::Int), // Default fallback
            }
        }
        Operand::Constant(constant) => {
            match constant {
                Constant::I32(_) => Ok(DataType::Int),
                Constant::F32(_) => Ok(DataType::Float),
                Constant::Bool(_) => Ok(DataType::Bool),
                Constant::String(_) => Ok(DataType::String),
                _ => Ok(DataType::Int), // Default fallback
            }
        }
        Operand::FunctionRef(_) => Ok(DataType::Int), // Function references default to int
        Operand::GlobalRef(_) => Ok(DataType::Int),   // Global references default to int
    }
}

/// Convert AST operator to WIR binary operation
///
/// Maps Beanstalk AST operators to WIR binary operations that correspond to WASM instructions.
/// This includes arithmetic, comparison, and logical operators.
///
/// # Supported Operators
///
/// - **Arithmetic**: Add, Subtract, Multiply, Divide, Modulus
/// - **Comparison**: Equality, LessThan, LessThanOrEqual, GreaterThan, GreaterThanOrEqual
/// - **Logical**: And, Or (implemented as short-circuiting control flow at higher level)
/// - **Bitwise**: Not (mapped to Ne for boolean negation)
///
/// # Parameters
///
/// - `op`: AST operator to convert
///
/// # Returns
///
/// - `Ok(BinOp)`: Corresponding WIR binary operation
/// - `Err(CompileError)`: Unsupported operator error
fn ast_operator_to_wir_binop(
    op: &crate::compiler::parsers::expressions::expression::Operator,
) -> Result<BinOp, CompileError> {
    use crate::compiler::parsers::expressions::expression::Operator;

    match op {
        // Arithmetic operators
        Operator::Add => Ok(BinOp::Add),
        Operator::Subtract => Ok(BinOp::Sub),
        Operator::Multiply => Ok(BinOp::Mul),
        Operator::Divide => Ok(BinOp::Div),
        Operator::Modulus => Ok(BinOp::Rem),
        
        // Comparison operators (essential for if conditions)
        Operator::Equality => Ok(BinOp::Eq),
        Operator::LessThan => Ok(BinOp::Lt),
        Operator::LessThanOrEqual => Ok(BinOp::Le),
        Operator::GreaterThan => Ok(BinOp::Gt),
        Operator::GreaterThanOrEqual => Ok(BinOp::Ge),
        
        // Logical operators
        Operator::And => Ok(BinOp::And),
        Operator::Or => Ok(BinOp::Or),
        Operator::Not => Ok(BinOp::Ne), // Boolean negation mapped to Ne
        
        // Unsupported operators
        _ => return_wir_transformation_error!(
            TextLocation::default(),
            "Operator {:?} not yet supported in WIR transformation. This operator needs to be added to the WIR binary operation mapping.",
            op
        ),
    }
}

/// Infer the result type of a binary operation
///
/// Determines the result type of a binary operation based on the operand types
/// and the operator. This is essential for creating properly-typed temporary
/// variables during expression evaluation.
///
/// # Type Inference Rules
///
/// - **Arithmetic**: Result type matches operand types (Int + Int = Int, Float + Float = Float)
/// - **Comparison**: Always returns Bool (Int > Int = Bool)
/// - **Logical**: Always returns Bool (Bool and Bool = Bool)
/// - **String Concatenation**: String + String = String
///
/// # Parameters
///
/// - `lhs_type`: Type of left operand
/// - `rhs_type`: Type of right operand
/// - `op`: Binary operator
///
/// # Returns
///
/// - `Ok(DataType)`: Inferred result type
/// - `Err(CompileError)`: Type inference error
fn infer_binary_operation_result_type(
    lhs_type: &DataType,
    rhs_type: &DataType,
    op: &crate::compiler::parsers::expressions::expression::Operator,
) -> Result<DataType, CompileError> {
    use crate::compiler::parsers::expressions::expression::Operator;

    match op {
        // Arithmetic operations preserve the operand type
        Operator::Add | Operator::Subtract | Operator::Multiply | Operator::Divide | Operator::Modulus => {
            // Special case: String concatenation
            if matches!(op, Operator::Add) && (lhs_type == &DataType::String || rhs_type == &DataType::String) {
                return Ok(DataType::String);
            }
            
            // For matching types, preserve the type
            if lhs_type == rhs_type {
                Ok(lhs_type.clone())
            } else {
                // Type mismatch - default to Int for now
                // TODO: Implement proper type coercion rules
                Ok(DataType::Int)
            }
        }
        
        // Comparison operations always return boolean
        Operator::Equality
        | Operator::LessThan
        | Operator::LessThanOrEqual
        | Operator::GreaterThan
        | Operator::GreaterThanOrEqual => {
            Ok(DataType::Bool)
        }
        
        // Logical operations always return boolean
        Operator::And | Operator::Or | Operator::Not => {
            Ok(DataType::Bool)
        }
        
        // Unsupported operators
        _ => return_wir_transformation_error!(
            TextLocation::default(),
            "Operator {:?} not supported for type inference. This operator needs type inference rules to be added.",
            op
        ),
    }
}
