//! TIR unit tests.
//!
//! WHAT: exercises the typed IDs, store, summary, converter, and validation types
//! introduced in Phase B0 and Phase B1.
//! WHY: TIR is a new internal representation; keeping its unit tests in a separate
//!      module follows the project rule that tests do not live in production files.

mod converter_tests;
mod fold_parity_tests;
mod ids_tests;
mod store_tests;
mod summary_tests;
mod validation_tests;
