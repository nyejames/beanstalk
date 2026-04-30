// Targeted #[allow(...)] attributes are used where needed, each with a justification
// comment. Avoid blanket crate-level allowances.

mod compiler_tests {
    #[cfg(test)]
    mod frontend_pipeline_tests;
    pub(crate) mod integration_test_runner; // For running all integration tests and report back the results

    #[cfg(test)]
    pub mod test_support;
}
pub mod build_system;
mod compiler_frontend;
mod libraries;

mod backends {
    pub(crate) mod error_types;
    pub(crate) mod external_package_validation;
    pub(crate) mod js;
    pub(crate) mod wasm;
}

pub mod projects {
    pub mod check;
    pub mod cli;
    pub mod dev_server;
    pub(crate) mod html_project;
    // Kept intentionally in pre-alpha as the future CLI entrypoint for interactive
    // template experimentation. This remains outside the default command surface.
    pub(crate) mod repl;
    pub(crate) mod routing;
    pub mod settings;
}
