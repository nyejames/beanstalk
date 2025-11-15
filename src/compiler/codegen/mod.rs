//! # WASM Code Generation Module
//!
//! This module handles the final stage of compilation: generating WASM bytecode
//! from the WIR (WASM Intermediate Representation).
//!
//! ## Architecture
//!
//! The codegen module performs direct WIR-to-WASM lowering:
//! - [`build_wasm`]: Main WASM module construction and validation
//! - [`wasm_encoding`]: WASM bytecode encoding and instruction generation
//! - [`wat_to_wasm`]: WAT (WebAssembly Text) to WASM binary conversion
//!
//! ## Error Handling
//!
//! The codegen module uses the unified error system from `compiler_errors.rs`:
//!
//! ### WASM Generation Errors
//!
//! Use `return_wasm_generation_error!` for codegen failures:
//!
//! ```rust
//! // With metadata
//! return_wasm_generation_error!(
//!     format!("Failed to generate WASM export for function '{}'", func_name),
//!     error_location,
//!     {
//!         CompilationStage => "WASM Generation",
//!         PrimarySuggestion => "This is a compiler bug - please report it"
//!     }
//! );
//!
//! // Simple version
//! return_wasm_generation_error!(
//!     "WASM validation failed",
//!     error_location
//! );
//! ```
//!
//! ### ErrorLocation Conversion
//!
//! Codegen modules use `TextLocation` which must be converted to `ErrorLocation`:
//!
//! ```rust
//! // Convert TextLocation to ErrorLocation
//! let error_location = text_location.to_error_location(string_table);
//!
//! // Use in error creation
//! return_wasm_generation_error!(message, error_location, { metadata });
//! ```
//!
//! ### Error Handling Pattern
//!
//! All codegen functions return `Result<T, CompileError>`:
//!
//! ```rust
//! pub fn generate_wasm_function(
//!     wir_function: &WirFunction,
//!     context: &CodegenContext,
//! ) -> Result<WasmFunction, CompileError> {
//!     // Validate WIR before generation
//!     if !is_valid_for_wasm(wir_function) {
//!         return_wasm_generation_error!(
//!             format!("Function '{}' cannot be lowered to WASM", wir_function.name),
//!             wir_function.location.to_error_location(context.string_table),
//!             {
//!                 CompilationStage => "WASM Generation",
//!                 PrimarySuggestion => "Check WIR validation rules"
//!             }
//!         );
//!     }
//!
//!     // Generate WASM instructions
//!     Ok(wasm_function)
//! }
//! ```
//!
//! ## Design Principles
//!
//! ### Direct WIR Lowering
//! Each WIR statement maps to ≤3 WASM instructions for efficient code generation.
//! No complex optimization passes - external WASM tools handle advanced optimizations.
//!
//! ### Memory Safety Preservation
//! The codegen preserves all memory safety guarantees from borrow checking:
//! - Lifetime-informed ARC placement
//! - Precise memory layout based on WIR place analysis
//! - Drop elaboration based on WIR lifetime information

pub mod build_wasm;
pub mod wasm_encoding;
pub mod wat_to_wasm;