use crate::compiler::{compiler_errors::CompileError, mir::build_mir::{ast_to_mir_with_events, run_borrow_checking_on_function, MIR}, parsers::build_ast::AstBlock};

/// WASM-optimized Mid-level Intermediate Representation (MIR) with simplified borrow checking
///
/// This module contains the MIR implementation designed specifically for efficient WASM
/// generation with simple dataflow-based borrow checking using program points and events.


/// Borrow check pipeline entry point function
///
/// This function orchestrates the complete MIR generation and borrow checking pipeline:
/// 1. AST-to-MIR lowering with event generation
/// 2. Control flow graph construction
/// 3. Backward liveness analysis for last-use refinement
/// 4. Forward loan-liveness dataflow analysis
/// 5. Conflict detection and error reporting
/// 6. WASM constraint validation
pub fn borrow_check_pipeline(ast: AstBlock) -> Result<MIR, Vec<CompileError>> {
    // Step 1: Lower AST to MIR with event generation
    let mir = match ast_to_mir_with_events(ast) {
        Ok(mir) => mir,
        Err(e) => return Err(vec![e]),
    };

    // Step 2: Run borrow checking on each function
    let mut all_errors = Vec::new();

    for function in &mir.functions {
        match run_borrow_checking_on_function(function) {
            Ok(_) => {
                // Borrow checking passed for this function
            }
            Err(errors) => {
                all_errors.extend(errors);
            }
        }
    }

    // Step 3: If there are borrow checking errors, return them
    if !all_errors.is_empty() {
        return Err(all_errors);
    }

    // Step 4: Validate WASM constraints
    if let Err(e) = mir.validate_wasm_constraints() {
        let compile_error = CompileError {
            msg: e,
            location: crate::compiler::parsers::tokens::TextLocation::default(),
            error_type: crate::compiler::compiler_errors::ErrorType::Compiler,
            file_path: std::path::PathBuf::new(),
        };
        return Err(vec![compile_error]);
    }

    Ok(mir)
}