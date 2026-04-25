//! Dev server v2 entry point and orchestration.
//!
//! This module validates CLI input, runs the initial dev build, starts the watcher/build loop,
//! and serves HTTP/SSE traffic for hot reload for both single-file and project-directory builds.

mod build_loop;
mod dev_client;
mod error_page;
mod http;
mod server;
mod sse;
mod state;
mod static_files;
mod watch;

pub use server::run_dev_server;

#[cfg(test)]
pub(crate) use server::{resolve_dev_runtime_paths, validate_dev_entry_path};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DevServerOptions {
    pub host: String,
    pub port: u16,
    pub poll_interval_ms: u64,
}

impl Default for DevServerOptions {
    fn default() -> Self {
        Self {
            host: String::from("127.0.0.1"),
            port: 6342,
            poll_interval_ms: 300,
        }
    }
}

#[cfg(test)]
#[path = "tests/mod_tests.rs"]
mod tests;
