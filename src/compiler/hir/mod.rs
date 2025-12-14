//! High-Level Intermediate Representation (HIR)
//!
//! HIR is Beanstalk's semantic IR designed for borrow checking, move analysis,
//! and structured lowering to WebAssembly. It preserves high-level control flow
//! while eliminating syntactic sugar and providing a place-based memory model.
//!
//! Key design principles:
//! - Structured control flow for CFG-based analysis
//! - Place-based memory model for precise borrow tracking  
//! - No nested expressions - all computation linearized into statements
//! - Borrow intent, not ownership outcome (determined by the borrow checker)
//! - Language-shaped, not Wasm-shaped (deferred to LIR)

pub mod builder;
pub mod nodes;
pub mod place;

// Re-export main types for convenience
pub use builder::HirBuilder;
pub use nodes::{BinOp, BorrowKind, HirExpr, HirExprKind, HirKind, HirNode, HirNodeId, UnaryOp};
pub use place::{IndexKind, Place, PlaceRoot, Projection};
