//! Dev server v2 entry point and orchestration.
//!
//! This module validates CLI input, runs the initial dev build, starts the watcher/build loop,
//! and serves HTTP/SSE traffic for hot reload in phase-1 single-file mode.

mod build_loop;
mod error_page;
mod http;
mod sse;
mod state;
mod static_files;
mod watch;

use crate::build_system::build::ProjectBuilder;
use crate::compiler_frontend::Flag;
use crate::compiler_frontend::basic_utility_functions::check_if_valid_path;
use crate::compiler_frontend::compiler_errors::{CompilerError, CompilerMessages, ErrorType};
use crate::projects::dev_server::build_loop::{ProjectBuildExecutor, dev_server_error_messages};
use crate::projects::dev_server::state::DevServerState;
use saying::say;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

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

pub fn run_dev_server(
    builder: Box<dyn ProjectBuilder + Send>,
    entry_path: &str,
    flags: &[Flag],
    options: DevServerOptions,
) -> Result<(), CompilerMessages> {
    let entry_file = validate_dev_entry_path(entry_path)?;
    let watch_root = entry_file
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    let output_dir = watch_root.join("dev");

    let state = Arc::new(DevServerState::new(output_dir.clone()));
    let mut executor = ProjectBuildExecutor::new(builder);

    let initial_build_report =
        build_loop::run_single_build_cycle(&state, &mut executor, &entry_file, flags);
    if initial_build_report.build_ok {
        say!(
            Green "Initial dev build succeeded. Reload broadcast to ",
            Green initial_build_report.clients_notified,
            Green " clients."
        );
    } else {
        say!(
            Yellow "Initial dev build failed. Reload broadcast to ",
            Yellow initial_build_report.clients_notified,
            Yellow " clients."
        );
    }

    let bind_addr = format!("{}:{}", options.host, options.port);
    let listener = TcpListener::bind(&bind_addr).map_err(|error| {
        dev_server_error_messages(
            &entry_file,
            format!("Failed to start dev server on {bind_addr}: {error}"),
        )
    })?;

    let host_display = if options.host == "127.0.0.1" {
        "localhost"
    } else {
        options.host.as_str()
    };
    say!(Bold "Dev server listening at:");
    say!(
        Green "http://",
        Green host_display,
        Green ":",
        Green options.port
    );

    let watch_state = Arc::clone(&state);
    let watch_executor = Box::new(executor) as Box<dyn build_loop::DevBuildExecutor>;
    let watch_entry_file = entry_file.clone();
    let watch_flags = flags.to_vec();
    let watch_root_clone = watch_root.clone();
    let poll_interval = Duration::from_millis(options.poll_interval_ms);

    // Watch/rebuild runs independently from request handling so SSE clients do not block rebuilds.
    thread::spawn(move || {
        build_loop::run_watch_build_loop(
            watch_state,
            watch_executor,
            watch_entry_file,
            watch_flags,
            watch_root_clone,
            poll_interval,
        );
    });

    // The server keeps the accept loop simple: each connection is handled on a small worker thread.
    for stream_result in listener.incoming() {
        match stream_result {
            Ok(stream) => {
                let state = Arc::clone(&state);
                thread::spawn(move || {
                    if let Err(error) = http::handle_connection(stream, state) {
                        say!(
                            Yellow "Dev server request handling warning: ",
                            Yellow error.to_string()
                        );
                    }
                });
            }
            Err(error) => {
                say!(
                    Yellow "Dev server connection accept warning: ",
                    Yellow error.to_string()
                );
            }
        }
    }

    Ok(())
}

fn validate_dev_entry_path(entry_path: &str) -> Result<PathBuf, CompilerMessages> {
    let resolved_path = if entry_path.trim().is_empty() {
        std::env::current_dir().map_err(|error| {
            dev_server_error_messages(
                Path::new("."),
                format!("Failed to resolve current directory: {error}"),
            )
        })?
    } else {
        check_if_valid_path(entry_path).map_err(|error| CompilerMessages {
            errors: vec![error.with_error_type(ErrorType::DevServer)],
            warnings: Vec::new(),
        })?
    };

    if resolved_path.is_dir() {
        return Err(dev_server_error_messages(
            &resolved_path,
            "Project directory mode is deferred in Dev Server v2 phase 1. \
             Please run `bst dev <file.bst>` for now.",
        ));
    }

    if !resolved_path.is_file() {
        return Err(dev_server_error_messages(
            &resolved_path,
            "Dev server entry path must resolve to a .bst file.",
        ));
    }

    let is_beanstalk_file = resolved_path
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext == "bst");
    if !is_beanstalk_file {
        return Err(dev_server_error_messages(
            &resolved_path,
            "Dev server currently only supports .bst file entries.",
        ));
    }

    match resolved_path.canonicalize() {
        Ok(canonical_path) => Ok(canonical_path),
        Err(error) => Err(CompilerMessages {
            errors: vec![
                CompilerError::file_error(
                    &resolved_path,
                    format!("Failed to canonicalize dev entry path: {error}"),
                )
                .with_error_type(ErrorType::DevServer),
            ],
            warnings: Vec::new(),
        }),
    }
}

#[cfg(test)]
#[path = "tests/mod_tests.rs"]
mod tests;
