//! # WIR Construction Module
//!
//! This module transforms the Abstract Syntax Tree (AST) into WASM Intermediate
//! Representation (WIR) optimized for WASM generation. The WIR provides a simplified,
//! place-based representation that enables efficient borrow checking and direct
//! WASM lowering.

// Re-export all WIR components from sibling modules
pub use crate::compiler::wir::wir_nodes::*;

// Import context types from context module
use crate::compiler::wir::context::WirTransformContext;

// Import statement functions from statements module

// Core compiler imports - consolidated for clarity
use crate::compiler::borrow_checker::borrow_checker::run_unified_borrow_checking;
use crate::compiler::borrow_checker::extract::BorrowFactExtractor;
use crate::compiler::{
    compiler_errors::CompileError,
};
// Error handling macros - grouped for maintainability
use crate::compiler::datatypes::Ownership;
use crate::compiler::parsers::expressions::expression::ExpressionKind;
use crate::{ir_log, wir_log, return_compiler_error};
use crate::compiler::parsers::ast_nodes::AstNode;
use crate::compiler::parsers::tokenizer::tokens::TextLocation;

/// Main entry point: Transform AST to WIR with borrow checking
///
/// This is the primary function for converting a complete AST into WIR representation.
/// It orchestrates the entire transformation process including AST-to-WIR conversion
/// and integrated borrow checking to ensure memory safety.
///
/// # Parameters
///
/// - `ast`: Complete AST block representing a Beanstalk program or module
///
/// # Returns
///
/// - `Ok(WIR)`: Complete WIR with all functions borrow-checked and ready for WASM lowering
/// - `Err(CompileError)`: Transformation or borrow checking error with source location
///
/// # Transformation Process
///
/// 1. **Context Initialization**: Create transformation context with empty state
/// 2. **AST Processing**: Transform each AST node to WIR statements and functions
/// 3. **Borrow Checking**: Run Polonius-style borrow checking on all WIR functions
/// 4. **Validation**: Ensure all memory access patterns are safe
///
/// # Memory Safety
///
/// The returned WIR is guaranteed to be memory-safe:
/// - All borrows are validated against Beanstalk's borrowing rules
/// - Move semantics are properly tracked and enforced
/// - No use-after-move or borrow conflicts exist
///
/// # WASM Readiness
///
/// The WIR is optimized for direct WASM lowering:
/// - All places map to WASM locals or linear memory locations
/// - All operations correspond to WASM instruction sequences
/// - Function calls are prepared for WASM function tables
pub fn ast_to_wir(ast: Vec<AstNode>) -> Result<WIR, CompileError> {
    let mut context = WirTransformContext::new();
    let mut wir = WIR::new();

    // Separate function definitions from other top-level statements
    let mut functions = Vec::new();
    let mut other_statements = Vec::new();

    for node in ast {
        match &node.kind {
            crate::compiler::parsers::ast_nodes::NodeKind::Function(name, signature, body) => {
                // Transform function definition and add to WIR
                let wir_function = create_wir_function_from_ast(name, signature, body, &mut context)?;
                
                // Check if this is an entry point function and add export
                if name == "_start" {
                    wir.exports.insert("_start".to_string(), crate::compiler::wir::wir_nodes::Export {
                        name: "_start".to_string(),
                        kind: crate::compiler::wir::wir_nodes::ExportKind::Function,
                        index: wir.functions.len() as u32, // Function index in the WIR
                    });
                    wir_log!("Added export for entry point function '{}'", name);
                }
                
                functions.push(wir_function);
            }
            _ => {
                other_statements.push(node);
            }
        }
    }

    // Add all functions to the WIR
    for function in functions {
        wir.add_function(function);
    }

    // Create a main function for any remaining top-level statements
    if !other_statements.is_empty() {
        let main_function = create_main_function_from_ast(&other_statements, &mut context)?;
        wir.add_function(main_function);
    }

    // Run borrow checking on the WIR
    run_borrow_checking_on_wir(&mut wir)?;

    Ok(wir)
}

/// Create a main function containing all top-level AST statements
fn create_main_function_from_ast(
    ast: &Vec<AstNode>,
    context: &mut WirTransformContext,
) -> Result<WirFunction, CompileError> {
    use crate::compiler::wir::wir_nodes::{Terminator, WirBlock, WirFunction};

    wir_log!(
        "create_main_function_from_ast called with {} AST nodes",
        ast.len()
    );

    // Create the main function
    let mut main_function = WirFunction::new(
        0, // function ID
        "main".to_string(),
        vec![], // no parameters
        vec![], // no return types for now
        vec![], // no return args for now
    );

    // Create a single basic block for all statements
    let mut main_block = WirBlock::new(0);
    let mut statements = Vec::new();

    // Transform each AST node to WIR statements
    for node in ast {
        wir_log!("Processing AST node: {:?}", node.kind);
        let node_statements = transform_ast_node_to_wir(node, context)?;
        wir_log!(
            "Generated {} WIR statements for node {:?}",
            node_statements.len(),
            node.kind
        );
        statements.extend(node_statements);
    }

    // Add all statements to the block
    main_block.statements = statements;

    // Add a return terminator
    main_block.terminator = Terminator::Return { values: vec![] };

    // Add the block to the function
    main_function.blocks = vec![main_block];

    Ok(main_function)
}

/// Transform a single AST node to WIR statements
fn transform_ast_node_to_wir(
    node: &AstNode,
    context: &mut WirTransformContext,
) -> Result<Vec<Statement>, CompileError> {
    use crate::compiler::parsers::ast_nodes::NodeKind;
    use crate::compiler::wir::wir_nodes::{BorrowKind, Constant, Operand, Rvalue, Statement};

    match &node.kind {
        NodeKind::VariableDeclaration(arg) => {
            // Create a place for the variable
            let var_place = context.create_place_for_variable(arg.name.clone())?;

            // For now, create a simple assignment
            // This is a minimal implementation - full expression handling will be added later
            let rvalue = match &arg.value.kind {
                ExpressionKind::StringSlice(s) => {
                    // Create a string constant
                    Rvalue::Use(Operand::Constant(Constant::String(s.clone())))
                }
                ExpressionKind::Int(i) => Rvalue::Use(Operand::Constant(Constant::I32(*i as i32))),
                ExpressionKind::Reference(var_ref) => {
                    // This is a reference to another variable - create a borrow
                    let source_place = context.get_place_for_variable(var_ref)?;

                    // Check the ownership to determine borrow kind
                    let borrow_kind = match &arg.value.ownership {
                        Ownership::MutableReference => {
                            wir_log!(
                                "Creating mutable borrow for declaration '{}' with MutableReference ownership",
                                arg.name
                            );
                            BorrowKind::Mut
                        }
                        _ => {
                            wir_log!(
                                "Creating shared borrow for declaration '{}' with {:?} ownership",
                                arg.name,
                                arg.value.ownership
                            );
                            BorrowKind::Shared
                        }
                    };

                    Rvalue::Ref {
                        place: source_place,
                        borrow_kind,
                    }
                }
                _ => {
                    // For other expression types, create a placeholder
                    Rvalue::Use(Operand::Constant(Constant::I32(0)))
                }
            };

            Ok(vec![Statement::Assign {
                place: var_place,
                rvalue,
            }])
        }

        NodeKind::Mutation(var_name, expression, is_mutable) => {
            // Get the existing place for the variable
            let var_place = context.get_place_for_variable(var_name)?;

            // Debug logging
            wir_log!(
                "Processing mutation for '{}', is_mutable: {}",
                var_name,
                is_mutable
            );
            wir_log!("Expression kind: {:?}", expression.kind);

            // Handle assignment based on mutability flag
            let rvalue = match &expression.kind {
                ExpressionKind::Reference(var_ref) => {
                    let source_place = context.get_place_for_variable(var_ref)?;
                    // Use the is_mutable flag to determine borrow kind
                    let borrow_kind = if *is_mutable {
                        wir_log!(
                            "Creating mutable borrow for '{}' ~= '{}'",
                            var_name,
                            var_ref
                        );
                        BorrowKind::Mut // x ~= y (mutable assignment)
                    } else {
                        wir_log!("Creating shared borrow for '{}' = '{}'", var_name, var_ref);
                        BorrowKind::Shared // x = y (shared assignment)
                    };

                    Rvalue::Ref {
                        place: source_place,
                        borrow_kind,
                    }
                }
                _ => {
                    wir_log!(
                        "Non-reference expression for '{}', expression kind: {:?}",
                        var_name,
                        expression.kind
                    );
                    // For other types, create a simple use
                    Rvalue::Use(Operand::Constant(Constant::I32(0)))
                }
            };

            Ok(vec![Statement::Assign {
                place: var_place,
                rvalue,
            }])
        }

        NodeKind::Expression(expr) => {
            // Handle standalone expressions
            match &expr.kind {
                ExpressionKind::Template(_template) => {
                    // For templates, we might need to create temporary variables
                    // For now, just create a no-op
                    Ok(vec![])
                }
                _ => {
                    // For other expressions, create a no-op for now
                    Ok(vec![])
                }
            }
        }

        NodeKind::Function(name, signature, body) => {
            // Transform function definition to WIR function
            transform_function_node(name, signature, body, context)
        }

        _ => {
            // For other node types, delegate to the statements module
            // This ensures all node types are properly handled
            crate::compiler::wir::statements::transform_ast_node_to_wir(node, context)
        }
    }
}

/// Create a WIR function from AST function definition
///
/// This function converts an AST function definition into a complete WIR function.
/// It handles parameter conversion, return type mapping, and function body transformation.
///
/// # Parameters
///
/// - `name`: Function name
/// - `signature`: Function signature with parameters and return types
/// - `body`: Function body as AST nodes
/// - `context`: WIR transformation context
///
/// # Returns
///
/// - `Ok(WirFunction)`: Complete WIR function ready for borrow checking
/// - `Err(CompileError)`: Transformation error
fn create_wir_function_from_ast(
    name: &str,
    signature: &crate::compiler::parsers::statements::functions::FunctionSignature,
    body: &[AstNode],
    context: &mut WirTransformContext,
) -> Result<crate::compiler::wir::wir_nodes::WirFunction, CompileError> {
    use crate::compiler::wir::wir_nodes::{Terminator, WirBlock, WirFunction};

    wir_log!("Creating WIR function '{}' with {} body nodes", name, body.len());

    // Check if this is an entry point function and validate signature
    let is_entry_point = name == "_start";
    if is_entry_point {
        wir_log!("Function '{}' detected as entry point", name);
        
        // Validate entry point signature - should have no parameters and no returns
        if !signature.parameters.is_empty() {
            return_compiler_error!(
                "Entry point function '{}' should not have parameters, found {} parameters",
                name,
                signature.parameters.len()
            );
        }
        
        if !signature.returns.is_empty() {
            return_compiler_error!(
                "Entry point function '{}' should not have return values, found {} return values",
                name,
                signature.returns.len()
            );
        }
    }

    // Convert AST signature to WIR signature
    let mut param_places = Vec::new();
    let mut return_types = Vec::new();

    // Process parameters
    for (param_index, param) in signature.parameters.iter().enumerate() {
        // Create a place for each parameter
        let param_place = context.create_place_for_parameter(
            param.name.clone(),
            param_index as u32,
            &param.value.data_type,
        )?;
        param_places.push(param_place);
    }

    // Process return types
    for return_arg in &signature.returns {
        let wasm_type = convert_datatype_to_wasm_type(&return_arg.value.data_type)?;
        return_types.push(wasm_type);
    }

    // Create the WIR function
    let function_id = context.get_next_function_id();
    let mut wir_function = WirFunction::new(
        function_id,
        name.to_string(),
        param_places,
        return_types,
        signature.returns.clone(),
    );

    // Transform function body
    let body_statements = transform_function_body(body, context)?;

    // Create a single basic block for the function body
    let mut main_block = WirBlock::new(0);
    main_block.statements = body_statements;

    // Add return terminator
    main_block.terminator = if signature.returns.is_empty() {
        Terminator::Return { values: vec![] }
    } else {
        // For now, return default values - proper return handling will be added later
        Terminator::Return { values: vec![] }
    };

    // Add the block to the function
    wir_function.add_block(main_block);

    wir_log!("Successfully created WIR function '{}'", name);

    Ok(wir_function)
}

/// Transform a function AST node to WIR statements (legacy compatibility)
///
/// This function is kept for compatibility with the existing transform_ast_node_to_wir
/// function. It delegates to create_wir_function_from_ast but doesn't return the function
/// since it can't be added to the WIR from this context.
///
/// # Parameters
///
/// - `name`: Function name
/// - `signature`: Function signature with parameters and return types
/// - `body`: Function body as AST nodes
/// - `context`: WIR transformation context
///
/// # Returns
///
/// - `Ok(Vec<Statement>)`: Empty vector (function handling is done at higher level)
/// - `Err(CompileError)`: Transformation error
fn transform_function_node(
    name: &str,
    _signature: &crate::compiler::parsers::statements::functions::FunctionSignature,
    _body: &[AstNode],
    _context: &mut WirTransformContext,
) -> Result<Vec<Statement>, CompileError> {
    wir_log!("Function '{}' encountered in statement context - this should be handled at module level", name);
    
    // For now, return empty statements since functions should be handled at the module level
    // In a complete implementation, this might create a function reference or similar
    Ok(vec![])
}

/// Transform function body AST nodes to WIR statements
///
/// Processes each AST node in the function body and converts them to WIR statements.
/// This handles variable declarations, expressions, and other statements within function scope.
/// It creates a new scope for the function body to properly handle local variables.
///
/// # Parameters
///
/// - `body`: Function body as AST nodes
/// - `context`: WIR transformation context
///
/// # Returns
///
/// - `Ok(Vec<Statement>)`: WIR statements for the function body
/// - `Err(CompileError)`: Transformation error
fn transform_function_body(
    body: &[AstNode],
    context: &mut WirTransformContext,
) -> Result<Vec<Statement>, CompileError> {
    let mut statements = Vec::new();

    // Enter a new scope for the function body
    context.enter_scope();

    for (_node_index, node) in body.iter().enumerate() {
        wir_log!("Processing function body node {}: {:?}", _node_index, node.kind);
        
        let node_statements = transform_ast_node_to_wir(node, context)?;
        wir_log!(
            "Generated {} WIR statements for function body node {}",
            node_statements.len(),
            _node_index
        );
        statements.extend(node_statements);
    }

    // Exit the function body scope
    context.exit_scope();

    Ok(statements)
}

/// Convert DataType to WasmType
///
/// Maps Beanstalk data types to WASM types for function signatures.
///
/// # Parameters
///
/// - `data_type`: Beanstalk data type
///
/// # Returns
///
/// - `Ok(WasmType)`: Corresponding WASM type
/// - `Err(CompileError)`: Unsupported type error
fn convert_datatype_to_wasm_type(
    data_type: &crate::compiler::datatypes::DataType,
) -> Result<crate::compiler::wir::place::WasmType, CompileError> {
    use crate::compiler::datatypes::DataType;
    use crate::compiler::wir::place::WasmType;

    match data_type {
        DataType::Int => Ok(WasmType::I32),
        DataType::Float => Ok(WasmType::F32),
        DataType::Bool => Ok(WasmType::I32), // Booleans are represented as i32 in WASM
        DataType::String => Ok(WasmType::I32), // String references are i32 pointers
        _ => {
            return_compiler_error!(
                "DataType to WasmType conversion not yet implemented for {:?}",
                data_type
            );
        }
    }
}

/// Run borrow checking on all functions in the WIR
///
/// Performs Polonius-style borrow checking on every function in the WIR to ensure
/// memory safety. This includes fact extraction, constraint solving, and error
/// reporting for any borrow checking violations.
///
/// # Parameters
///
/// - `wir`: Mutable reference to WIR containing all functions to check
///
/// # Returns
///
/// - `Ok(())`: All functions pass borrow checking
/// - `Err(CompileError)`: Borrow checking error with detailed diagnostics
///
/// # Borrow Checking Process
///
/// For each function:
/// 1. **Event Generation**: Regenerate events for all statements and terminators
/// 2. **Fact Extraction**: Extract Polonius facts (loans, uses, moves, kills)
/// 3. **Constraint Solving**: Run unified borrow checking algorithm
/// 4. **Error Reporting**: Generate detailed error messages for violations
///
/// # Error Types Detected
///
/// - **Borrow Conflicts**: Mutable borrow while shared borrows exist
/// - **Use After Move**: Accessing moved variables
/// - **Multiple Mutable Borrows**: More than one mutable borrow of the same data
/// - **Lifetime Violations**: Borrows outliving their borrowed data
///
/// # Integration with WASM
///
/// Borrow checking results inform WASM generation:
/// - ARC insertion points for shared ownership
/// - Move vs. copy decisions for value transfers
/// - Memory layout optimization based on lifetime analysis
fn run_borrow_checking_on_wir(wir: &mut WIR) -> Result<(), CompileError> {
    for function in &mut wir.functions {
        // Ensure events are generated for all statements and terminators
        regenerate_events_for_function(function);

        // Extract borrow facts from the function
        let mut extractor = BorrowFactExtractor::new();
        extractor.extract_function(function).map_err(|e| {
            CompileError::compiler_error(&format!(
                "Failed to extract borrow facts for function '{}': {}",
                function.name, e
            ))
        })?;

        // Update the function's events with the loans that were created
        extractor.update_function_events(function);

        // Run unified borrow checking
        let borrow_results = run_unified_borrow_checking(function, &extractor).map_err(|e| {
            CompileError::compiler_error(&format!(
                "Borrow checking failed for function '{}': {}",
                function.name, e
            ))
        })?;

        // Handle borrow checking errors with proper diagnostics
        if !borrow_results.errors.is_empty() {
            let first_error = &borrow_results.errors[0];
            let detailed_message = format!(
                "Borrow checking error in function '{}': {}.",
                function.name, first_error.message
            );

            let error_location = if first_error.primary_location != TextLocation::default() {
                first_error.primary_location.clone()
            } else {
                TextLocation::default()
            };

            return Err(CompileError::new_rule_error(
                detailed_message,
                error_location,
            ));
        }

        ir_log!(
            "Borrow checking completed successfully for function '{}'",
            function.name
        );
    }

    Ok(())
}

/// Regenerate events for all statements and terminators in a function
///
/// Creates fresh program points and events for every statement and terminator
/// in a WIR function. This is necessary for borrow checking as events track
/// all memory operations (reads, writes, moves, borrows) at specific program points.
///
/// # Parameters
///
/// - `function`: Mutable reference to WIR function to process
///
/// # Event Generation Process
///
/// 1. **Clear Existing Events**: Remove any previously generated events
/// 2. **Statement Events**: Generate events for each statement in each block
/// 3. **Terminator Events**: Generate events for block terminators
/// 4. **Program Points**: Assign unique program points for precise tracking
///
/// # Program Point Assignment
///
/// - **Statements**: `block_id * 1000 + statement_index`
/// - **Terminators**: `block_id * 1000 + 999`
///
/// This ensures unique, ordered program points for precise borrow analysis.
///
/// # Event Types Generated
///
/// - **Use Events**: Variable reads and borrows
/// - **Move Events**: Ownership transfers
/// - **Loan Events**: Borrow creation and invalidation
/// - **Kill Events**: End of variable lifetimes
fn regenerate_events_for_function(function: &mut WirFunction) {
    function.events.clear();

    let mut all_events = Vec::new();

    for block in &function.blocks {
        for (stmt_index, statement) in block.statements.iter().enumerate() {
            let program_point = ProgramPoint::new(block.id * 1000 + stmt_index as u32);
            let events = statement.generate_events_at_program_point(program_point);
            all_events.push((program_point, events));
        }

        let terminator_point = ProgramPoint::new(block.id * 1000 + 999);
        let terminator_events = block
            .terminator
            .generate_events_at_program_point(terminator_point);
        all_events.push((terminator_point, terminator_events));
    }

    for (program_point, events) in all_events {
        function.store_events(program_point, events);
    }
}

// Function definitions will be handled later when we understand the AST structure better
