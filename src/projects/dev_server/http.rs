//! Minimal HTTP routing for the std-only dev server.
//!
//! Routes SSE and ping endpoints, serves static files from the dev output directory, and falls
//! back to a generated error page when the latest build failed.

use crate::projects::dev_server::error_page::render_runtime_error_page;
use crate::projects::dev_server::sse;
use crate::projects::dev_server::state::{BuildState, DevServerState};
use crate::projects::dev_server::static_files::{self, ResolvePathError};
use std::fs::File;
use std::io::{self, BufRead, BufReader, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub fn handle_connection(mut stream: TcpStream, state: Arc<DevServerState>) -> io::Result<()> {
    let Some(request) = parse_request(&stream)? else {
        return Ok(());
    };

    if request.method != "GET" {
        return send_text_response(
            &mut stream,
            "405 METHOD NOT ALLOWED",
            "text/plain; charset=utf-8",
            "Method Not Allowed",
        );
    }

    let request_path = strip_query_string(&request.path);
    match request_path {
        "/__beanstalk/events" => sse::handle_sse_connection(stream, state),
        "/__beanstalk/ping" => {
            send_text_response(&mut stream, "200 OK", "text/plain; charset=utf-8", "ok")
        }
        _ => serve_static_request(&mut stream, request_path, &state),
    }
}

struct HttpRequest {
    method: String,
    path: String,
}

enum PreparedResponse {
    Text {
        status_line: &'static str,
        content_type: &'static str,
        body: String,
    },
    File {
        path: PathBuf,
        content_type: &'static str,
    },
}

impl PreparedResponse {
    fn text(
        status_line: &'static str,
        content_type: &'static str,
        body: impl Into<String>,
    ) -> Self {
        Self::Text {
            status_line,
            content_type,
            body: body.into(),
        }
    }
}

fn parse_request(stream: &TcpStream) -> io::Result<Option<HttpRequest>> {
    let mut reader = BufReader::new(stream.try_clone()?);

    let mut request_line = String::new();
    if reader.read_line(&mut request_line)? == 0 {
        return Ok(None);
    }

    // Discard headers; v2 only needs method/path routing.
    loop {
        let mut header_line = String::new();
        let bytes_read = reader.read_line(&mut header_line)?;
        if bytes_read == 0 || header_line == "\r\n" {
            break;
        }
    }

    let mut parts = request_line.split_whitespace();
    let Some(method) = parts.next() else {
        return Ok(None);
    };
    let Some(path) = parts.next() else {
        return Ok(None);
    };

    Ok(Some(HttpRequest {
        method: method.to_owned(),
        path: path.to_owned(),
    }))
}

fn strip_query_string(path: &str) -> &str {
    path.split('?').next().unwrap_or(path)
}

fn serve_static_request(
    stream: &mut TcpStream,
    request_path: &str,
    state: &Arc<DevServerState>,
) -> io::Result<()> {
    let build_state = state
        .build_state
        .lock()
        .map_err(|_| io::Error::other("build state lock was poisoned"))?
        .clone();

    match prepare_static_response(request_path, &build_state) {
        PreparedResponse::Text {
            status_line,
            content_type,
            body,
        } => send_text_response(stream, status_line, content_type, &body),
        PreparedResponse::File { path, content_type } => {
            stream_file_response(stream, &path, content_type)
        }
    }
}

fn prepare_static_response(request_path: &str, build_state: &BuildState) -> PreparedResponse {
    let resolved_path = match static_files::resolve_request_path(
        request_path,
        &build_state.output_dir,
        build_state.entry_page_rel.as_deref(),
    ) {
        Ok(path) => Some(path),
        Err(ResolvePathError::MissingEntryPage) if request_path == "/" => None,
        Err(ResolvePathError::InvalidPath) | Err(ResolvePathError::MissingEntryPage) => {
            return PreparedResponse::text(
                "404 NOT FOUND",
                "text/plain; charset=utf-8",
                "Not Found",
            );
        }
    };

    // Failed builds should replace stale HTML pages with diagnostics while still allowing
    // supporting assets to load so the browser can keep the last successful shell intact.
    if should_serve_failed_build_html(request_path, resolved_path.as_deref(), build_state) {
        let error_page = build_state.last_error_html.clone().unwrap_or_else(|| {
            render_runtime_error_page(
                "Build Failed",
                "The latest build failed, but no diagnostics were stored.",
                build_state.last_build_version,
            )
        });

        return PreparedResponse::text("200 OK", "text/html; charset=utf-8", error_page);
    }

    let resolved_path = match resolved_path {
        Some(path) => path,
        None => {
            let error_page = render_runtime_error_page(
                "Missing Entry Page",
                "Build did not produce a HTML entry page for '/'.",
                build_state.last_build_version,
            );
            return PreparedResponse::text("200 OK", "text/html; charset=utf-8", error_page);
        }
    };

    if !resolved_path.exists() || !resolved_path.is_file() {
        return PreparedResponse::text("404 NOT FOUND", "text/plain; charset=utf-8", "Not Found");
    }

    let content_type = static_files::content_type_for_path(&resolved_path);
    if static_files::is_html_content_type(content_type) {
        let html = match std::fs::read_to_string(&resolved_path) {
            Ok(contents) => contents,
            Err(error) => {
                return PreparedResponse::text(
                    "500 INTERNAL SERVER ERROR",
                    "text/plain; charset=utf-8",
                    format!("Failed to read HTML file: {error}"),
                );
            }
        };

        let injected_html = static_files::inject_dev_client(&html);
        return PreparedResponse::text("200 OK", content_type, injected_html);
    }

    PreparedResponse::File {
        path: resolved_path,
        content_type,
    }
}

fn should_serve_failed_build_html(
    request_path: &str,
    resolved_path: Option<&Path>,
    build_state: &BuildState,
) -> bool {
    if build_state.last_build_ok {
        return false;
    }

    if request_path == "/" {
        return true;
    }

    let Some(resolved_path) = resolved_path else {
        return false;
    };

    let content_type = static_files::content_type_for_path(resolved_path);
    static_files::is_html_content_type(content_type)
}

fn stream_file_response(
    stream: &mut TcpStream,
    file_path: &Path,
    content_type: &str,
) -> io::Result<()> {
    let file = File::open(file_path)?;
    let content_length = file.metadata()?.len();
    send_response_headers(stream, "200 OK", content_type, content_length)?;

    let mut reader = BufReader::new(file);
    io::copy(&mut reader, stream)?;
    stream.flush()
}

fn send_text_response(
    stream: &mut TcpStream,
    status_line: &str,
    content_type: &str,
    body: &str,
) -> io::Result<()> {
    send_response_bytes(stream, status_line, content_type, body.as_bytes())
}

fn send_response_bytes(
    stream: &mut TcpStream,
    status_line: &str,
    content_type: &str,
    body: &[u8],
) -> io::Result<()> {
    send_response_headers(stream, status_line, content_type, body.len() as u64)?;
    stream.write_all(body)?;
    stream.flush()
}

fn send_response_headers(
    stream: &mut TcpStream,
    status_line: &str,
    content_type: &str,
    content_length: u64,
) -> io::Result<()> {
    let headers = format!(
        "HTTP/1.1 {status_line}\r\nContent-Type: {content_type}\r\nContent-Length: {content_length}\r\nConnection: close\r\n\r\n"
    );
    stream.write_all(headers.as_bytes())
}

#[cfg(test)]
#[path = "tests/http_tests.rs"]
mod tests;
