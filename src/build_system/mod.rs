//! Build-system entry modules.
//!
//! This layer owns project discovery, configuration parsing, backend orchestration, and output
//! writing above the shared compiler frontend.

pub(crate) mod build;
pub(crate) mod create_project_modules;
pub(crate) mod project_config;
