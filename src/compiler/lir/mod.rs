//! Low-Level Intermediate Representation (LIR) Module
//!
//! This module contains the LIR data structures and the HIR to LIR lowering pass.
//! LIR is a WASM-shaped representation that can be directly emitted as WebAssembly.
//!
//! ## Module Structure
//!
//! - `nodes` - LIR data structures (LirModule, LirFunction, LirInst, etc.)
//! - `context` - Lowering context and local allocation
//! - `types` - Struct layout computation and type conversion
//! - `expressions` - Expression lowering (literals, binary/unary ops)
//! - `memory` - Memory operations (field access, collection access)
//! - `ownership` - Ownership tagging and drop operations
//! - `functions` - Function call lowering and parameter handling
//! - `control_flow` - Control flow lowering (if, match, loop, etc.)
//! - `statements` - Statement and definition lowering
//! - `lower` - Main lowering entry point
//! - `errors` - Error helper methods
//! - `display` - LIR pretty-printing

pub mod nodes;

mod build_lir;
mod context;
mod control_flow;
mod display;
mod expressions;
mod functions;
mod memory;
mod ownership;
mod statements;
mod types;

// Re-export the main public interface
pub use build_lir::lower_hir_to_lir;
pub use context::{LocalAllocator, LoopContext, LoweringContext};
pub use display::display_lir;
pub use types::{
    FieldLayout, StructLayout, alignment_of_lir_type, build_struct_layout, calculate_struct_size,
    compute_field_offsets, hir_expr_to_lir_type, size_of_lir_type,
};
