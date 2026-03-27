//! HIR lowering test modules and shared harness utilities.
//!
//! WHAT: groups the HIR test suites and exposes common resolver helpers for them.
//! WHY: HIR tests share path-resolution setup and should discover one another through a single module entry.

pub(crate) use crate::compiler_frontend::test_support::test_project_path_resolver;

mod hir_expression_lowering_tests;
mod hir_function_origin_tests;
mod hir_statement_lowering_tests;
mod hir_validation_tests;
