// While many parts of the compiler are in heavy development,
// there are lots of placeholders and code that will possibly be used, but isn't atm.
// #![allow(dead_code, unused_macros, unused_variables)]

mod compiler_tests {
    pub(crate) mod integration_test_runner; // For running all integration tests and report back the results
}
pub mod build_system;
mod compiler_frontend;

mod backends {
    pub(crate) mod js;
    pub(crate) mod wasm;
}

pub mod projects {
    pub mod cli;
    pub mod dev_server;
    pub(crate) mod html_project;
    pub(crate) mod path_resolution;
    pub(crate) mod repl;
    pub(crate) mod routing;
    pub mod settings;
}
