//! # WIR (WASM Intermediate Representation) Module
//!
//! This module contains the WASM-targeted intermediate representation used for
//! borrow checking and direct WASM lowering in the Beanstalk compiler.
//!
//! ## Architecture Overview
//!
//! The WIR serves two primary purposes:
//! 1. **Precise Borrow Checking**: Enables Polonius-style lifetime analysis with place-based tracking
//! 2. **Direct WASM Lowering**: Each WIR statement maps to ≤3 WASM instructions for efficient code generation
//!
//! ## Module Organization
//!
//! ### Core Transformation Pipeline
//! - [`build_wir`]: Main entry point and orchestration functions for AST-to-WIR transformation
//! - [`context`]: Context management, variable scoping, and place allocation during transformation
//! - [`expressions`]: Expression transformation from AST to WIR rvalues and operands
//! - [`statements`]: Statement transformation including control flow and function calls
//! - [`templates`]: Template transformation and string processing for Beanstalk's template system
//! - [`utilities`]: Utility functions for type checking, conversions, and common operations
//!
//! ### Core WIR Infrastructure
//! - [`wir`]: Core WIR data structures and types
//! - [`wir_nodes`]: WIR node definitions (statements, terminators, operands)
//! - [`place`]: Place abstraction for memory locations and WASM-aware memory management
//!
//! ### Borrow Checking Infrastructure
//! - [`extract`]: Fact extraction for Polonius-style borrow checking
//! - [`borrow_checker`]: Borrow checking implementation with lifetime analysis
//!
//! ## Design Principles
//!
//! ### WASM-First Design
//! The WIR is specifically designed for efficient WASM generation:
//! - **Direct Instruction Mapping**: WIR operations correspond directly to WASM instruction sequences
//! - **WASM Type Alignment**: All operands use WASM value types (i32, i64, f32, f64)
//! - **Structured Control Flow**: WIR blocks map directly to WASM's structured control flow
//! - **Linear Memory Integration**: Place analysis designed for WASM's linear memory model
//!
//! ### Borrow Checking Integration
//! The WIR structure is optimized for precise lifetime analysis:
//! - **Statement-Level Precision**: Every program point tracked for borrow analysis
//! - **Field-Sensitive Places**: Struct fields and array indices tracked separately
//! - **Three-Address Form**: Clear operand reads/writes for precise borrow tracking
//! - **Polonius Fact Generation**: Direct integration with Polonius-style analysis
//!
//! ### No Optimization Passes
//! The WIR explicitly avoids complex optimization passes:
//! - **Constant Folding**: Handled at AST stage before WIR generation
//! - **External Optimization**: Advanced optimizations left to external WASM tools
//! - **Minimal Passes**: Only borrow checking and direct WASM lowering
//!
//! ## Usage Example
//!
//! ```rust
//! use crate::compiler::wir::build_wir::ast_to_wir;
//! use crate::compiler::parsers::build_ast::AstBlock;
//!
//! // Transform AST to WIR with borrow checking
//! let wir = ast_to_wir(ast_block)?;
//!
//! // WIR is now ready for direct WASM lowering
//! ```
//!
//! ## Error Handling
//!
//! The WIR transformation uses the unified error system from `compiler_errors.rs`:
//!
//! ### WIR Transformation Errors
//!
//! Use `return_wir_transformation_error!` for transformation failures:
//!
//! ```rust
//! // With metadata
//! return_wir_transformation_error!(
//!     format!("Cannot transform expression type {:?}", expr_type),
//!     error_location,
//!     {
//!         CompilationStage => "WIR Transformation",
//!         PrimarySuggestion => "This is a compiler bug - please report it"
//!     }
//! );
//!
//! // Simple version
//! return_wir_transformation_error!(
//!     "Unsupported AST node type",
//!     error_location
//! );
//! ```
//!
//! ### ErrorLocation Conversion
//!
//! WIR modules use `TextLocation` which must be converted to `ErrorLocation`:
//!
//! ```rust
//! // Convert TextLocation to ErrorLocation
//! let error_location = text_location.to_error_location(string_table);
//!
//! // Use in error creation
//! return_wir_transformation_error!(message, error_location, { metadata });
//! ```
//!
//! ### Error Handling Pattern
//!
//! All WIR transformation functions return `Result<T, CompileError>`:
//!
//! ```rust
//! pub fn transform_ast_node_to_wir(
//!     node: &AstNode,
//!     context: &mut WirContext,
//! ) -> Result<Vec<Statement>, CompileError> {
//!     match node.node_type {
//!         AstNodeType::Supported => {
//!             // Transform successfully
//!             Ok(statements)
//!         }
//!         AstNodeType::Unsupported => {
//!             // Return error with helpful message
//!             return_wir_transformation_error!(
//!                 format!("Unsupported node type: {:?}", node.node_type),
//!                 node.location.to_error_location(context.string_table),
//!                 {
//!                     CompilationStage => "WIR Transformation",
//!                     PrimarySuggestion => "This feature is not yet implemented"
//!                 }
//!             )
//!         }
//!     }
//! }
//! ```
//!
//! ## Memory Safety Guarantees
//!
//! The WIR ensures memory safety through:
//! - **Compile-time borrow checking**: All memory access patterns validated before WASM generation
//! - **Place-based analysis**: Precise tracking of all memory locations and their lifetimes
//! - **Move semantics**: Ownership transfer tracked to prevent use-after-move errors
//! - **Field-sensitive borrowing**: Disjoint field access allowed while maintaining safety

// Core WIR modules
pub mod build_wir;
pub mod place;
pub mod wir_nodes;

// New modular organization
pub mod context;
pub mod expressions;
pub mod statements;
pub mod templates;
pub mod utilities;

// Re-export main public API to maintain compatibility
// External code imports directly from submodules, so no re-exports needed here
