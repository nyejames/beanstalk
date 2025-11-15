//! # Statement Transformation Module
//!
//! This module contains functions for converting AST statements to WIR statements.
//! It handles control flow transformations (if/else, loops), manages function calls
//! and host function integration, and processes variable declarations and mutations.

// Import context types from context module
use crate::compiler::wir::context::WirTransformContext;

// Import expression functions from expressions module
use crate::compiler::wir::expressions::expression_to_rvalue_with_context;

// Import WIR types
use crate::compiler::wir::wir_nodes::{BorrowKind, Constant, Operand, Rvalue, Statement};

// Core compiler imports
use crate::compiler::parsers::expressions::expression::ExpressionKind;
use crate::compiler::parsers::tokenizer::tokens::TextLocation;
use crate::compiler::{
    compiler_errors::CompileError,
    parsers::{
        ast_nodes::{Arg, AstNode, NodeKind},
        expressions::expression::Expression,
    },
};
// Error handling macros
use crate::return_wir_transformation_error;

/// Transform a single AST node to WIR statements
///
/// This is the main dispatch function for converting AST nodes to WIR statements.
/// It handles all statement types including declarations, mutations, function calls,
/// control flow, and expressions used as statements.
///
/// # Parameters
///
/// - `node`: AST node to transform
/// - `context`: Transformation context for variable management and place allocation
///
/// # Returns
///
/// - `Ok(Vec<Statement>)`: WIR statements representing the AST node
/// - `Err(CompileError)`: Transformation error with source location
///
/// # Supported AST Node Types
///
/// - **Declarations**: Variable declarations with initialization
/// - **Mutations**: Variable assignments and updates
/// - **Function Calls**: Regular function calls and host function calls
/// - **Control Flow**: If statements, loops, and other control structures
/// - **Expressions**: Function definitions and other expression statements
///
/// # Error Handling
///
/// Unsupported node types return compiler errors indicating the missing
/// implementation. This helps track which language features still need
/// WIR transformation support.
pub fn transform_ast_node_to_wir(
    node: &AstNode,
    context: &mut WirTransformContext,
    string_table: &mut crate::compiler::string_interning::StringTable,
) -> Result<Vec<Statement>, CompileError> {
    match &node.kind {
        NodeKind::VariableDeclaration(arg) => {
            let var_name = string_table.resolve(arg.id).to_string();
            ast_declaration_to_wir(&var_name, &arg.value, &node.location, context, string_table)
        }
        NodeKind::Mutation(name, value, is_mutable) => {
            let var_name = string_table.resolve(*name).to_string();
            ast_mutation_to_wir(&var_name, value, *is_mutable, &node.location, context, string_table)
        }
        NodeKind::FunctionCall(name, args, _, _) => {
            let func_name = string_table.resolve(*name).to_string();
            ast_function_call_to_wir(&func_name, args, &node.location, context, string_table)
        }
        NodeKind::HostFunctionCall(name, args, _, module, function, _) => {
            let func_name = string_table.resolve(*name).to_string();
            let module_name = string_table.resolve(*module).to_string();
            let function_name = string_table.resolve(*function).to_string();
            ast_host_function_call_to_wir(&func_name, args, &module_name, &function_name, &node.location, context, string_table)
        }
        NodeKind::Print(expr) => {
            // Transform Print node to a host function call to the print function
            // Print is a built-in host function provided by the runtime
            ast_print_to_wir(expr, &node.location, context, string_table)
        }
        NodeKind::If(condition, then_block, else_block) => {
            ast_if_statement_to_wir(condition, then_block, else_block, &node.location, context, string_table)
        }
        NodeKind::Expression(expr) => {
            // Handle standalone expressions (like function definitions)
            match &expr.kind {
                ExpressionKind::Function(args, body) => {
                    ast_function_definition_to_wir(&args.parameters, body, &node.location, context, string_table)
                }
                _ => {
                    // For other expressions, convert to assignment to temporary
                    let (statements, _rvalue) =
                        expression_to_rvalue_with_context(expr, &node.location, context, string_table)?;
                    Ok(statements)
                }
            }
        }
        NodeKind::Return(return_values) => {
            ast_return_to_wir(return_values, &node.location, context, string_table)
        }
        _ => {
            let node_type_str: &'static str = Box::leak(format!("{:?}", node.kind).into_boxed_str());
            let error_location = node.location.clone().to_error_location(string_table);
            return_wir_transformation_error!(
                format!("AST node type {:?} not yet implemented in WIR transformation", node.kind),
                error_location, {
                    FoundType => node_type_str,
                    CompilationStage => "WIR Generation",
                    PrimarySuggestion => "This language feature needs WIR lowering support to be added to the compiler",
                }
            );
        }
    }
}


/// Transform AST declaration to WIR statements
///
/// # Performance Optimizations
///
/// - Pre-allocates statement vector with estimated capacity
/// - Avoids unnecessary string allocation by using string references
/// - Reduces clone operations where possible
fn ast_declaration_to_wir(
    name: &str,
    value: &Expression,
    location: &TextLocation,
    context: &mut WirTransformContext,
    string_table: &mut crate::compiler::string_interning::StringTable,
) -> Result<Vec<Statement>, CompileError> {
    // Pre-allocate with capacity for expression statements + assignment (typically 2-4 statements)
    let mut statements = Vec::with_capacity(4);

    // Convert the value expression to an rvalue
    let (expr_statements, rvalue) = expression_to_rvalue_with_context(value, location, context, string_table)?;
    statements.extend(expr_statements);

    // Create a place for the variable
    let place = context.get_place_manager().allocate_local(&value.data_type);

    // Register the variable in the context (avoid unnecessary string allocation)
    context.register_variable(name.to_owned(), place.clone());

    // Create assignment statement
    statements.push(Statement::Assign { place, rvalue });

    Ok(statements)
}

/// Transform AST mutation to WIR statements
///
/// # Performance Optimization
///
/// Pre-allocates statement vector with estimated capacity to reduce reallocations.
fn ast_mutation_to_wir(
    name: &str,
    value: &Expression,
    is_mutable: bool,
    location: &TextLocation,
    context: &mut WirTransformContext,
    string_table: &mut crate::compiler::string_interning::StringTable,
) -> Result<Vec<Statement>, CompileError> {
    // Pre-allocate with capacity for expression statements + assignment (typically 2-4 statements)
    let mut statements = Vec::with_capacity(4);

    // Look up the variable
    let place = context
        .lookup_variable(name)
        .ok_or_else(|| {
            let error_location = location.clone().to_error_location(string_table);
            let name_static: &'static str = Box::leak(name.to_string().into_boxed_str());
            CompileError {
                msg: format!("Undefined variable '{}' in mutation", name),
                location: error_location,
                error_type: crate::compiler::compiler_errors::ErrorType::WirTransformation,
                metadata: {
                    let mut map = std::collections::HashMap::new();
                    map.insert(crate::compiler::compiler_errors::ErrorMetaDataKey::VariableName, name_static);
                    map.insert(crate::compiler::compiler_errors::ErrorMetaDataKey::CompilationStage, "WIR Transformation");
                    map.insert(crate::compiler::compiler_errors::ErrorMetaDataKey::PrimarySuggestion, "Ensure the variable is declared before attempting to mutate it");
                    map
                },
            }
        })?
        .clone();

    // Handle mutable assignments (~=) vs regular assignments (=)
    let rvalue = if is_mutable {
        // For mutable assignments (~=), check if the value is a variable reference
        // If so, create a mutable borrow; otherwise, use the expression value
        match &value.kind {
            crate::compiler::parsers::expressions::expression::ExpressionKind::Reference(
                var_name,
            ) => {
                // This is a mutable borrow: x ~= y
                let resolved_var_name = string_table.resolve(*var_name);
                let source_place = context
                    .lookup_variable(resolved_var_name)
                    .ok_or_else(|| {
                        let error_location = location.clone().to_error_location(string_table);
                        let var_name_static: &'static str = Box::leak(resolved_var_name.to_string().into_boxed_str());
                        CompileError {
                            msg: format!("Undefined variable '{}' in mutable borrow", resolved_var_name),
                            location: error_location,
                            error_type: crate::compiler::compiler_errors::ErrorType::WirTransformation,
                            metadata: {
                                let mut map = std::collections::HashMap::new();
                                map.insert(crate::compiler::compiler_errors::ErrorMetaDataKey::VariableName, var_name_static);
                                map.insert(crate::compiler::compiler_errors::ErrorMetaDataKey::CompilationStage, "WIR Transformation");
                                map.insert(crate::compiler::compiler_errors::ErrorMetaDataKey::PrimarySuggestion, "Ensure the variable is declared before creating a mutable borrow");
                                map
                            },
                        }
                    })?
                    .clone();

                Rvalue::Ref {
                    place: source_place,
                    borrow_kind: BorrowKind::Mut,
                }
            }
            _ => {
                // For non-reference expressions, convert normally
                let (expr_statements, rvalue) =
                    expression_to_rvalue_with_context(value, location, context, string_table)?;
                statements.extend(expr_statements);
                rvalue
            }
        }
    } else {
        // For regular assignments (=), check if the value is a variable reference
        // If so, create a shared borrow; otherwise, use the expression value
        match &value.kind {
            crate::compiler::parsers::expressions::expression::ExpressionKind::Reference(
                var_name,
            ) => {
                // This is a shared borrow: x = y
                let resolved_var_name = string_table.resolve(*var_name);
                let source_place = context
                    .lookup_variable(resolved_var_name)
                    .ok_or_else(|| {
                        let error_location = location.clone().to_error_location(string_table);
                        let var_name_static: &'static str = Box::leak(resolved_var_name.to_string().into_boxed_str());
                        CompileError {
                            msg: format!("Undefined variable '{}' in shared borrow", resolved_var_name),
                            location: error_location,
                            error_type: crate::compiler::compiler_errors::ErrorType::WirTransformation,
                            metadata: {
                                let mut map = std::collections::HashMap::new();
                                map.insert(crate::compiler::compiler_errors::ErrorMetaDataKey::VariableName, var_name_static);
                                map.insert(crate::compiler::compiler_errors::ErrorMetaDataKey::CompilationStage, "WIR Transformation");
                                map.insert(crate::compiler::compiler_errors::ErrorMetaDataKey::PrimarySuggestion, "Ensure the variable is declared before creating a shared borrow");
                                map
                            },
                        }
                    })?
                    .clone();

                Rvalue::Ref {
                    place: source_place,
                    borrow_kind: BorrowKind::Shared,
                }
            }
            _ => {
                // For non-reference expressions, convert normally
                let (expr_statements, rvalue) =
                    expression_to_rvalue_with_context(value, location, context, string_table)?;
                statements.extend(expr_statements);
                rvalue
            }
        }
    };

    // Create assignment statement
    statements.push(Statement::Assign { place, rvalue });

    Ok(statements)
}

/// Transform AST function call to WIR statements
///
/// # Performance Optimization
///
/// Pre-allocates vectors with estimated capacity based on argument count.
fn ast_function_call_to_wir(
    name: &str,
    args: &[Expression],
    location: &TextLocation,
    context: &mut WirTransformContext,
    string_table: &mut crate::compiler::string_interning::StringTable,
) -> Result<Vec<Statement>, CompileError> {
    // Pre-allocate with capacity for arg processing + call (typically 2 statements per arg + 1)
    let mut statements = Vec::with_capacity(args.len() * 2 + 1);

    // Convert arguments to operands
    let mut arg_operands = Vec::with_capacity(args.len());
    for arg in args {
        let (arg_statements, rvalue) = expression_to_rvalue_with_context(arg, location, context, string_table)?;
        statements.extend(arg_statements);

        // Create temporary for the argument result
        let temp_place = context.create_temporary_place(&arg.data_type);
        statements.push(Statement::Assign {
            place: temp_place.clone(),
            rvalue,
        });
        arg_operands.push(Operand::Copy(temp_place));
    }

    // Create function call statement
    // TODO: Properly resolve function name to operand
    let interned_name = string_table.intern(name);
    let func_operand = Operand::Constant(crate::compiler::wir::wir_nodes::Constant::String(
        interned_name,
    ));
    statements.push(Statement::Call {
        func: func_operand,
        args: arg_operands,
        destination: None, // TODO: Handle return values
    });

    Ok(statements)
}

/// Transform AST print statement to WIR statements
fn ast_print_to_wir(
    expr: &Expression,
    location: &TextLocation,
    context: &mut WirTransformContext,
    string_table: &mut crate::compiler::string_interning::StringTable,
) -> Result<Vec<Statement>, CompileError> {
    // Print is a host function call to the print function
    // Convert the expression to a single-element argument list
    let args = vec![expr.clone()];
    
    // Call the host function transformation with print-specific parameters
    ast_host_function_call_to_wir(
        "print",
        &args,
        "beanstalk_io",
        "print",
        location,
        context,
        string_table,
    )
}

/// Transform AST return statement to WIR statements
///
/// Converts a return statement with return values into WIR statements that prepare
/// the return values. For now, this is a simplified implementation that just evaluates
/// the return expressions. Proper return handling with terminators will be added later.
///
/// # Parameters
///
/// - `return_values`: Expressions to return from the function
/// - `location`: Source location for error reporting
/// - `context`: Transformation context for variable management
///
/// # Returns
///
/// - `Ok(Vec<Statement>)`: WIR statements that evaluate return values
/// - `Err(CompileError)`: Transformation error with source location
///
/// # TODO
///
/// - Add proper Return terminator support
/// - Handle early returns (breaking out of current block)
/// - Integrate with control flow analysis
fn ast_return_to_wir(
    return_values: &[Expression],
    location: &TextLocation,
    context: &mut WirTransformContext,
    string_table: &mut crate::compiler::string_interning::StringTable,
) -> Result<Vec<Statement>, CompileError> {
    let mut statements = Vec::with_capacity(return_values.len() * 2);

    // For now, just evaluate the return expressions
    // Proper return handling with terminators will be added in a future task
    for return_expr in return_values {
        let (expr_statements, rvalue) =
            expression_to_rvalue_with_context(return_expr, location, context, string_table)?;
        statements.extend(expr_statements);

        // Create a temporary place for the return value
        let return_place = context.create_temporary_place(&return_expr.data_type);
        statements.push(Statement::Assign {
            place: return_place,
            rvalue,
        });
    }

    // TODO: Add proper Return terminator handling
    // For now, we just evaluate the expressions and let the function builder
    // add a default return terminator at the end of the function
    Ok(statements)
}

/// Transform AST host function call to WIR statements
///
/// # Performance Optimization
///
/// Pre-allocates vectors with estimated capacity based on argument count.
fn ast_host_function_call_to_wir(
    name: &str,
    args: &[Expression],
    module: &str,
    function: &str,
    location: &TextLocation,
    context: &mut WirTransformContext,
    string_table: &mut crate::compiler::string_interning::StringTable,
) -> Result<Vec<Statement>, CompileError> {
    // Pre-allocate with capacity for arg processing + call (typically 2 statements per arg + 1)
    let mut statements = Vec::with_capacity(args.len() * 2 + 1);

    // Convert arguments to operands
    let mut arg_operands = Vec::with_capacity(args.len());
    for arg in args {
        // Handle string literals as constants for WASIX fd_write calls
        match &arg.kind {
            crate::compiler::parsers::expressions::expression::ExpressionKind::StringSlice(s) => {
                // Create a constant operand directly for string literals
                // s is already an InternedString from the AST
                arg_operands.push(Operand::Constant(Constant::String(*s)));
            }
            _ => {
                // For other expression types, use the general approach
                let (arg_statements, rvalue) =
                    expression_to_rvalue_with_context(arg, location, context, string_table)?;
                statements.extend(arg_statements);

                // Create temporary for the argument result
                let temp_place = context.create_temporary_place(&arg.data_type);
                statements.push(Statement::Assign {
                    place: temp_place.clone(),
                    rvalue,
                });
                arg_operands.push(Operand::Copy(temp_place));
            }
        }
    }

    // Look up the host function definition from the builtin registry
    let interned_name = string_table.intern(name);
    let host_function = match crate::compiler::host_functions::registry::create_builtin_registry(string_table) {
        Ok(registry) => {
            match registry.get_function(&interned_name) {
                Some(func_def) => func_def.clone(),
                None => {
                    // If not found in builtin registry, create a basic definition
                    // This maintains backward compatibility for functions not yet in the registry
                    crate::compiler::host_functions::registry::HostFunctionDef::new(
                        name,
                        vec![], // Empty parameters for now
                        vec![], // Empty return types for now
                        module,
                        function,
                        &format!("Host function call to {}.{}", module, function),
                        string_table,
                    )
                }
            }
        }
        Err(_) => {
            // If registry creation fails, create a basic definition
            crate::compiler::host_functions::registry::HostFunctionDef::new(
                name,
                vec![], // Empty parameters for now
                vec![], // Empty return types for now
                module,
                function,
                &format!("Host function call to {}.{}", module, function),
                string_table,
            )
        }
    };

    // Add to context imports
    context.add_host_import(host_function.clone());

    // Create host call statement
    statements.push(Statement::HostCall {
        function: host_function,
        args: arg_operands,
        destination: None, // TODO: Handle return values
    });

    Ok(statements)
}

/// Transform AST if statement to WIR statements
///
/// Converts an if statement into proper WIR block-based control flow with terminators.
/// This creates a structured control flow that maps directly to WASM's if/else instructions.
///
/// # Control Flow Structure
///
/// The if statement is transformed into the following block structure:
/// ```
/// [condition evaluation statements]
/// Terminator::If {
///     condition: <condition_operand>,
///     then_block: <then_block_id>,
///     else_block: <else_block_id>,
/// }
/// ```
///
/// # Parameters
///
/// - `condition`: Boolean expression to evaluate
/// - `then_block`: Statements to execute if condition is true
/// - `else_block`: Optional statements to execute if condition is false
/// - `location`: Source location for error reporting
/// - `context`: Transformation context for variable management
///
/// # Returns
///
/// - `Ok(Vec<Statement>)`: Statements for condition evaluation (blocks handled separately)
/// - `Err(CompileError)`: Transformation error
///
/// # Note
///
/// This function returns the condition evaluation statements. The actual block structure
/// with terminators needs to be handled at the function level where blocks can be created.
/// For now, we use the simplified Statement::Conditional approach until proper block
/// management is implemented in the function transformation.
fn ast_if_statement_to_wir(
    condition: &Expression,
    then_block: &Vec<AstNode>,
    else_block: &Option<Vec<AstNode>>,
    location: &TextLocation,
    context: &mut WirTransformContext,
    string_table: &mut crate::compiler::string_interning::StringTable,
) -> Result<Vec<Statement>, CompileError> {
    // Pre-allocate with capacity for condition + blocks (estimate based on block sizes)
    let estimated_capacity = 2 + then_block.len() + else_block.as_ref().map_or(0, |b| b.len());
    let mut statements = Vec::with_capacity(estimated_capacity);

    // Convert condition to operand
    let (cond_statements, cond_rvalue) =
        expression_to_rvalue_with_context(condition, location, context, string_table)?;
    statements.extend(cond_statements);

    // Create temporary for the condition result
    let cond_place = context.create_temporary_place(&condition.data_type);
    statements.push(Statement::Assign {
        place: cond_place.clone(),
        rvalue: cond_rvalue,
    });

    let cond_operand = Operand::Copy(cond_place);

    // Enter new scope for then block
    context.enter_scope();
    
    // Transform then block
    let mut then_statements = Vec::new();
    for node in then_block {
        let node_statements = transform_ast_node_to_wir(node, context, string_table)?;
        then_statements.extend(node_statements);
    }
    
    // Exit then block scope
    context.exit_scope();

    // Transform else block if present
    let mut else_statements = Vec::new();
    if let Some(else_nodes) = else_block {
        // Enter new scope for else block
        context.enter_scope();
        
        for node in else_nodes {
            let node_statements = transform_ast_node_to_wir(node, context, string_table)?;
            else_statements.extend(node_statements);
        }
        
        // Exit else block scope
        context.exit_scope();
    }

    // Create conditional execution with proper scoping
    // This uses Statement::Conditional which internally manages the control flow
    // In a future enhancement, this could be converted to use Terminator::If with proper blocks
    statements.push(Statement::Conditional {
        condition: cond_operand,
        then_statements,
        else_statements,
    });

    Ok(statements)
}

/// Transform AST function definition to WIR statements
fn ast_function_definition_to_wir(
    args: &[Arg],
    body: &[AstNode],
    _location: &TextLocation,
    context: &mut WirTransformContext,
    string_table: &mut crate::compiler::string_interning::StringTable,
) -> Result<Vec<Statement>, CompileError> {
    let mut statements = Vec::new();

    // Enter new scope for function
    context.enter_scope();

    // Register function parameters
    for arg in args {
        let param_place = context
            .get_place_manager()
            .allocate_local(&arg.value.data_type);
        let param_name = string_table.resolve(arg.id);
        context.register_variable(param_name.to_string(), param_place);
    }

    // Transform function body
    for node in body {
        let node_statements = transform_ast_node_to_wir(node, context, string_table)?;
        statements.extend(node_statements);
    }

    // Exit function scope
    context.exit_scope();

    Ok(statements)
}
