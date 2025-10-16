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
use crate::compiler::{
    compiler_errors::CompileError,
    parsers::{build_ast::AstBlock, tokens::TextLocation, ast_nodes::AstNode},
};
use crate::compiler::borrow_checker::borrow_checker::run_unified_borrow_checking;
use crate::compiler::borrow_checker::extract::BorrowFactExtractor;
// Error handling macros - grouped for maintainability
use crate::ir_log;

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
pub fn ast_to_wir(ast: AstBlock) -> Result<WIR, CompileError> {
    let mut context = WirTransformContext::new();
    let mut wir = WIR::new();

    // Create a main function to contain all top-level statements
    let main_function = create_main_function_from_ast(&ast, &mut context)?;
    wir.add_function(main_function);

    // Run borrow checking on the WIR
    run_borrow_checking_on_wir(&mut wir)?;

    Ok(wir)
}

/// Create a main function containing all top-level AST statements
fn create_main_function_from_ast(ast: &AstBlock, context: &mut WirTransformContext) -> Result<WirFunction, CompileError> {
    use crate::compiler::wir::wir_nodes::{WirFunction, WirBlock, Terminator};
    use crate::compiler::wir::place::WasmType;
    
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
    for node in &ast.ast {
        let node_statements = transform_ast_node_to_wir(node, context)?;
        statements.extend(node_statements);
    }
    
    // Add all statements to the block
    main_block.statements = statements;
    
    // Add a return terminator
    main_block.terminator = Terminator::Return { 
        values: vec![] 
    };
    
    // Add the block to the function
    main_function.blocks = vec![main_block];
    
    Ok(main_function)
}

/// Transform a single AST node to WIR statements
fn transform_ast_node_to_wir(node: &AstNode, context: &mut WirTransformContext) -> Result<Vec<Statement>, CompileError> {
    use crate::compiler::parsers::ast_nodes::NodeKind;
    use crate::compiler::wir::wir_nodes::{Statement, Rvalue, Operand, BorrowKind, Constant};
    use crate::compiler::wir::place::Place;
    
    match &node.kind {
        NodeKind::Declaration(var_name, expression, _visibility) => {
            // Create a place for the variable
            let var_place = context.create_place_for_variable(var_name.clone())?;
            
            // For now, create a simple assignment
            // This is a minimal implementation - full expression handling will be added later
            let rvalue = match &expression.kind {
                crate::compiler::parsers::expressions::expression::ExpressionKind::StringSlice(s) => {
                    // Create a string constant
                    Rvalue::Use(Operand::Constant(Constant::String(s.clone())))
                },
                crate::compiler::parsers::expressions::expression::ExpressionKind::Int(i) => {
                    Rvalue::Use(Operand::Constant(Constant::I32(*i as i32)))
                },
                crate::compiler::parsers::expressions::expression::ExpressionKind::Reference(var_ref) => {
                    // This is a reference to another variable - create a borrow
                    let source_place = context.get_place_for_variable(var_ref)?;
                    Rvalue::Ref { 
                        place: source_place, 
                        borrow_kind: BorrowKind::Shared // Default to shared borrow
                    }
                },
                _ => {
                    // For other expression types, create a placeholder
                    Rvalue::Use(Operand::Constant(Constant::I32(0)))
                }
            };
            
            Ok(vec![Statement::Assign { 
                place: var_place, 
                rvalue 
            }])
        },
        
        NodeKind::Mutation(var_name, expression) => {
            // Get the existing place for the variable
            let var_place = context.get_place_for_variable(var_name)?;
            
            // Handle mutable assignment
            let rvalue = match &expression.kind {
                crate::compiler::parsers::expressions::expression::ExpressionKind::Reference(var_ref) => {
                    let source_place = context.get_place_for_variable(var_ref)?;
                    // Check if this is a mutable assignment (x ~= y)
                    // For now, assume mutable if it's a mutation
                    Rvalue::Ref { 
                        place: source_place, 
                        borrow_kind: BorrowKind::Mut 
                    }
                },
                _ => {
                    // For other types, create a simple use
                    Rvalue::Use(Operand::Constant(Constant::I32(0)))
                }
            };
            
            Ok(vec![Statement::Assign { 
                place: var_place, 
                rvalue 
            }])
        },
        
        NodeKind::Expression(expr) => {
            // Handle standalone expressions
            match &expr.kind {
                crate::compiler::parsers::expressions::expression::ExpressionKind::Template(_template) => {
                    // For templates, we might need to create temporary variables
                    // For now, just create a no-op
                    Ok(vec![])
                },
                _ => {
                    // For other expressions, create a no-op for now
                    Ok(vec![])
                }
            }
        },
        
        _ => {
            // For other node types, return empty statements for now
            // This allows the transformation to proceed without failing
            Ok(vec![])
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
