//! Static file and HTML injection helpers for the dev server.
//!
//! This module resolves safe output-relative paths, maps content types, and injects the tiny
//! EventSource client into HTML responses.

use crate::projects::dev_server::error_page::DEV_CLIENT_MARKER;
use std::path::{Component, Path, PathBuf};

#[derive(Debug, PartialEq, Eq)]
pub enum ResolvePathError {
    InvalidPath,
    MissingEntryPage,
}

pub fn dev_client_snippet() -> String {
    format!(
        "\n{DEV_CLIENT_MARKER}\n<script>\n  (() => {{\n    const source = new EventSource('/__beanstalk/events');\n    source.addEventListener('reload', () => window.location.reload());\n  }})();\n</script>\n"
    )
}

pub fn inject_dev_client(html: &str) -> String {
    if html.contains(DEV_CLIENT_MARKER) {
        return html.to_owned();
    }

    let snippet = dev_client_snippet();
    if let Some(body_index) = html.rfind("</body>") {
        let mut injected = String::with_capacity(html.len() + snippet.len());
        injected.push_str(&html[..body_index]);
        injected.push_str(&snippet);
        injected.push_str(&html[body_index..]);
        injected
    } else {
        let mut injected = String::with_capacity(html.len() + snippet.len());
        injected.push_str(html);
        injected.push_str(&snippet);
        injected
    }
}

pub fn content_type_for_path(path: &Path) -> &'static str {
    match path.extension().and_then(|ext| ext.to_str()) {
        Some("html") | Some("htm") => "text/html; charset=utf-8",
        Some("js") => "application/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("json") => "application/json; charset=utf-8",
        Some("wasm") => "application/wasm",
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("svg") => "image/svg+xml",
        Some("ico") => "image/x-icon",
        Some("txt") => "text/plain; charset=utf-8",
        Some("webmanifest") => "application/manifest+json; charset=utf-8",
        _ => "application/octet-stream",
    }
}

pub fn is_html_content_type(content_type: &str) -> bool {
    content_type.starts_with("text/html")
}

pub fn entry_route(entry_page_rel: &Path) -> String {
    let normalized = entry_page_rel.to_string_lossy().replace('\\', "/");
    format!("/{normalized}")
}

pub fn resolve_request_path(
    request_path: &str,
    output_dir: &Path,
    entry_page_rel: Option<&Path>,
) -> Result<PathBuf, ResolvePathError> {
    if request_path == "/" {
        let Some(entry_page) = entry_page_rel else {
            return Err(ResolvePathError::MissingEntryPage);
        };
        return resolve_relative_path(entry_page, output_dir);
    }

    let path_without_slash = request_path.trim_start_matches('/');
    if path_without_slash.is_empty() {
        return Err(ResolvePathError::InvalidPath);
    }

    resolve_relative_path(Path::new(path_without_slash), output_dir)
}

fn resolve_relative_path(
    relative_path: &Path,
    output_dir: &Path,
) -> Result<PathBuf, ResolvePathError> {
    let mut sanitized = PathBuf::new();
    // Reject any non-normal components so HTTP paths cannot escape the dev output root.
    for component in relative_path.components() {
        match component {
            Component::Normal(part) => sanitized.push(part),
            _ => return Err(ResolvePathError::InvalidPath),
        }
    }

    if sanitized.as_os_str().is_empty() {
        return Err(ResolvePathError::InvalidPath);
    }

    let full_path = output_dir.join(&sanitized);
    if !full_path.starts_with(output_dir) {
        return Err(ResolvePathError::InvalidPath);
    }

    Ok(full_path)
}

#[cfg(test)]
#[path = "tests/static_files_tests.rs"]
mod tests;
