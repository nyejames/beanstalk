use crate::compiler::borrow_checker::borrow_checker::UnifiedBorrowChecker;
use crate::compiler::borrow_checker::extract::BorrowFactExtractor;
use crate::compiler::parsers::ast_nodes::AstNode;
use crate::compiler::{
    compiler_errors::CompileError,
    wir::build_wir::{WIR, ast_to_wir},
    wir::wir_nodes::WirFunction,
};
use crate::{borrow_log, wir_log};

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
pub fn borrow_check_pipeline(ast: Vec<AstNode>, string_table: &mut crate::compiler::string_interning::StringTable) -> Result<WIR, Vec<CompileError>> {
    // Step 1: Lower AST to simplified WIR
    let wir = match ast_to_wir(ast, string_table) {
        Ok(wir) => wir,
        Err(e) => return Err(vec![e]),
    };

    // Step 2: Run state-aware borrow checking on all functions
    let mut all_errors = Vec::new();

    wir_log!("WIR has {} functions", wir.functions.len());

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
    // Debug: Log that borrow checking is being called
    borrow_log!("Running borrow checker on function with ID {}", function.id);
    borrow_log!("Function has {} blocks", function.blocks.len());
    borrow_log!("Function has {} events", function.events.len());
    borrow_log!("Function has {} loans", function.loans.len());
    // Step 1: Extract loans and build gen/kill sets with state mapping
    let (fact_extractor, state_mapping) = BorrowFactExtractor::from_function_with_states(function)
        .map_err(|e| {
            vec![CompileError::compiler_error(&format!(
                "Failed to extract borrow facts with states for function {}: {}",
                function.id, e
            ))]
        })?;

    // Step 2: Run state-aware unified borrow checking
    let loan_count = fact_extractor.get_loan_count();
    let mut checker =
        UnifiedBorrowChecker::new_with_function_id(loan_count, function.id);

    let borrow_results = checker
        .check_function_with_states(function, &fact_extractor, &state_mapping)
        .map_err(|e| {
            vec![CompileError::compiler_error(&format!(
                "State-aware borrow checking failed for function {}: {}",
                function.id, e
            ))]
        })?;

    // Step 3: Return borrow errors if any were detected
    if !borrow_results.errors.is_empty() {
        return Err(borrow_results.errors);
    }

    // Log successful borrow checking for debugging
    #[cfg(feature = "verbose_codegen_logging")]
    println!(
        "State-aware borrow checking completed successfully for function {}",
        function.id
    );

    Ok(())
}


