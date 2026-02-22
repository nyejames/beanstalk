// This is early prototype code, so ignore placeholder unused stuff for now
#![allow(unused)]

// CURRENT REFACTOR
pub(crate) mod hir_builder;
pub(crate) mod hir_datatypes;
pub(crate) mod hir_nodes;

// Private parts of the hir lowering
mod hir_display;
mod hir_expression;
mod hir_statement;
mod hir_structs;
mod hir_validation;

#[cfg(test)]
mod tests;
