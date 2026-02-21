// While many parts of the compiler are in heavy development,
// there are lots of placeholders and code that will possibly be used, but isn't atm.
#![allow(dead_code, unused_macros, unused_variables)]
#![warn(rust_2018_idioms, unreachable_pub)]
mod compiler_tests {
    #[cfg(test)]
    pub(crate) mod hir_expression_lowering_tests;
    pub(crate) mod integration_test_runner; // For running all integration tests and report back the results
}
pub mod build_system;
mod compiler_frontend;

mod backends {
    pub(crate) mod function_registry;
    // pub(crate) mod js;
    // pub mod lir;
    // pub(crate) mod wasm;
}

pub mod projects {
    pub mod cli;
    pub(crate) mod html_project;
    pub(crate) mod repl;
    pub mod settings;
}
