use crate::compiler::{
    compiler_errors::CompileError,
    wir::build_wir::{WIR, ast_to_wir},
    parsers::build_ast::AstBlock,
    wir::{
        extract::BorrowFactExtractor,
        borrow_checker::UnifiedBorrowChecker,
        wir_nodes::{WirFunction, BorrowError},
    },
};

/// WASM Intermediate Representation (WIR) with simplified borrow checking
///
/// This module contains the WIR implementation designed specifically for efficient WASM
/// generation with simple dataflow-based borrow checking using program points and events.

/// Borrow check pipeline entry point function
///
/// This function orchestrates the complete WIR generation and borrow checking pipeline:
/// 1. AST-to-WIR lowering with event generation
/// 2. State-aware borrow checking with hybrid loan-state mapping
/// 3. Error reporting and conversion to compile errors
pub fn borrow_check_pipeline(ast: AstBlock) -> Result<WIR, Vec<CompileError>> {
    // Step 1: Lower AST to simplified WIR
    let wir = match ast_to_wir(ast) {
        Ok(wir) => wir,
        Err(e) => return Err(vec![e]),
    };

    // Step 2: Run state-aware borrow checking on all functions
    let mut all_errors = Vec::new();
    
    for function in &wir.functions {
        match run_state_aware_borrow_checker(function) {
            Ok(_) => continue,
            Err(errors) => all_errors.extend(errors),
        }
    }
    
    if !all_errors.is_empty() {
        return Err(all_errors);
    }

    Ok(wir)
}

/// Run state-aware borrow checking on a single function
///
/// This function orchestrates the hybrid state-loan borrow checking approach:
/// 1. Extract loans and build gen/kill sets using existing infrastructure
/// 2. Map loans to Beanstalk states (Owned/Referenced/Borrowed/Moved)
/// 3. Run unified analysis with state-based conflict detection
/// 4. Convert borrow errors to compile errors with source locations
fn run_state_aware_borrow_checker(function: &WirFunction) -> Result<(), Vec<CompileError>> {
    // Step 1: Extract loans and build gen/kill sets with state mapping
    let (fact_extractor, state_mapping) = BorrowFactExtractor::from_function_with_states(function).map_err(|e| {
        vec![CompileError::compiler_error(&format!(
            "Failed to extract borrow facts with states for function '{}': {}",
            function.name, e
        ))]
    })?;

    // Step 2: Run state-aware unified borrow checking
    let loan_count = fact_extractor.get_loan_count();
    let mut checker = UnifiedBorrowChecker::new_with_function_name(loan_count, function.name.clone());
    
    let borrow_results = checker.check_function_with_states(function, &fact_extractor, &state_mapping).map_err(|e| {
        vec![CompileError::compiler_error(&format!(
            "State-aware borrow checking failed for function '{}': {}",
            function.name, e
        ))]
    })?;

    // Step 3: Convert borrow errors to compile errors
    if !borrow_results.errors.is_empty() {
        let compile_errors = convert_borrow_errors_to_compile_errors(&borrow_results.errors, &function.name);
        return Err(compile_errors);
    }

    // Log successful borrow checking for debugging
    #[cfg(feature = "verbose_codegen_logging")]
    println!("State-aware borrow checking completed successfully for function '{}'", function.name);

    Ok(())
}

/// Convert borrow errors to compile errors with proper error types and locations
///
/// This function transforms borrow checker errors into the compiler's standard
/// error format, preserving source location information and providing helpful
/// error messages that explain Beanstalk's memory model.
fn convert_borrow_errors_to_compile_errors(
    borrow_errors: &[BorrowError],
    function_name: &str,
) -> Vec<CompileError> {
    borrow_errors.iter().map(|borrow_error| {
        // Create detailed error message with function context
        let detailed_message = format!(
            "Borrow checking error in function '{}': {}",
            function_name, borrow_error.message
        );

        // Use the error location if available, otherwise use a default location
        let error_location = if borrow_error.primary_location != crate::compiler::parsers::tokens::TextLocation::default() {
            borrow_error.primary_location.clone()
        } else {
            // TODO: Map program points to source locations for better error reporting
            crate::compiler::parsers::tokens::TextLocation::default()
        };

        // Create compile error with rule error type (user-facing borrow checker error)
        CompileError::new_rule_error(detailed_message, error_location)
    }).collect()
}
