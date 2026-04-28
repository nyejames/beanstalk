//! High-level IR modules and lowering support.
//!
//! WHAT: defines HIR nodes/types plus AST-to-HIR lowering and validation helpers.

pub(crate) mod blocks;
pub(crate) mod constants;
pub(crate) mod expression_rewrite;
pub(crate) mod expressions;
pub(crate) mod functions;
pub(crate) mod ids;
pub(crate) mod module;
pub(crate) mod operators;
pub(crate) mod patterns;
pub(crate) mod places;
pub(crate) mod regions;
pub(crate) mod statements;
pub(crate) mod structs;
pub(crate) mod terminators;

pub(crate) mod hir_builder;
pub(crate) mod hir_datatypes;
pub(crate) mod hir_side_table;

// Private parts of the hir lowering
pub(crate) mod hir_display;
mod hir_expression;
mod hir_statement;
mod hir_structs;
mod hir_validation;
pub(crate) mod utils;

#[cfg(test)]
mod tests;
