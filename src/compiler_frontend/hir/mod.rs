//! High-level IR modules and lowering support.
//!
//! WHAT: defines HIR nodes/types plus AST-to-HIR lowering and validation helpers.

pub(crate) mod hir_builder;
pub(crate) mod hir_datatypes;
pub(crate) mod hir_nodes;
pub(crate) mod hir_side_table;

// Private parts of the hir lowering
pub(crate) mod hir_display;
mod hir_expression;
mod hir_statement;
mod hir_structs;
mod hir_validation;

#[cfg(test)]
mod tests;
