//! Error page rendering for dev-server build/runtime failures.
//!
//! Compiler diagnostics are converted into escaped HTML so failed builds still render safely and
//! remain connected to hot reload via the injected SSE client snippet.
use crate::compiler_frontend::basic_utility_functions::file_url_from_path;
use crate::compiler_frontend::compiler_errors::{
    CompilerError, CompilerMessages, ErrorMetaDataKey, error_type_to_str,
};
use crate::compiler_frontend::display_messages::{
    format_error_guidance_lines, relative_display_path_from_root, resolve_source_file_path,
    resolved_display_path,
};
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::string_interning::StringTable;
use crate::projects::dev_server::dev_client::dev_client_snippet;
use std::fmt::Write;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

struct SourcePathLink {
    display_label: String,
    href: String,
}

fn current_timestamp_unix_seconds() -> u64 {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => duration.as_secs(),
        Err(_) => 0,
    }
}

pub fn escape_html(input: &str) -> String {
    let mut escaped = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#39;"),
            _ => escaped.push(ch),
        }
    }
    escaped
}

pub fn format_compiler_messages(messages: &CompilerMessages) -> String {
    let mut output = String::new();

    if messages.errors.is_empty() && messages.warnings.is_empty() {
        output.push_str("No compiler diagnostics available.\n");
        return output;
    }

    for error in &messages.errors {
        let _ = writeln!(
            output,
            "[ERROR][{}] {}",
            error_type_to_str(&error.error_type),
            error.msg
        );
        if !error.location.scope.as_components().is_empty() {
            let _ = writeln!(
                output,
                "  at: {}",
                resolved_display_path(&error.location.scope, &messages.string_table)
            );
        }
        if error.location.start_pos.line_number > 0 {
            let _ = writeln!(
                output,
                "  line: {}, col: {}",
                error.location.start_pos.line_number + 1,
                error.location.start_pos.char_column + 1
            );
        }
        if let Some(stage) = error.metadata.get(&ErrorMetaDataKey::CompilationStage) {
            let _ = writeln!(output, "  stage: {stage}");
        }
        if let Some(help) = error.metadata.get(&ErrorMetaDataKey::PrimarySuggestion) {
            let _ = writeln!(output, "  help: {help}");
        }
        if let Some(alternative) = error.metadata.get(&ErrorMetaDataKey::AlternativeSuggestion) {
            let _ = writeln!(output, "  alternative: {alternative}");
        }
        if let Some(replacement) = error.metadata.get(&ErrorMetaDataKey::SuggestedReplacement) {
            let _ = writeln!(output, "  suggested replacement: {replacement}");
        }
        match (
            error.metadata.get(&ErrorMetaDataKey::SuggestedInsertion),
            error.metadata.get(&ErrorMetaDataKey::SuggestedLocation),
        ) {
            (Some(insertion), Some(location)) => {
                let _ = writeln!(output, "  suggested insertion: '{insertion}' {location}");
            }
            (Some(insertion), None) => {
                let _ = writeln!(output, "  suggested insertion: '{insertion}'");
            }
            (None, Some(location)) => {
                let _ = writeln!(output, "  suggested location: {location}");
            }
            (None, None) => {}
        }
    }

    for warning in &messages.warnings {
        let _ = writeln!(output, "[WARNING] {}", warning.msg);
        if !warning.location.scope.as_components().is_empty() {
            let _ = writeln!(
                output,
                "  at: {}",
                resolved_display_path(&warning.location.scope, &messages.string_table)
            );
        }
    }

    output
}

pub fn render_compiler_error_page(
    messages: &CompilerMessages,
    project_root: &Path,
    origin: &str,
    build_version: u64,
) -> String {
    let diagnostics_html = render_compiler_diagnostics(messages, project_root);
    render_error_page_shell("Build Failed", &diagnostics_html, origin, build_version)
}

pub fn render_runtime_error_page(
    title: &str,
    details: &str,
    origin: &str,
    build_version: u64,
) -> String {
    let escaped_details = escape_html(details);
    let details_html = format!("<pre class=\"msg\">{escaped_details}</pre>");
    render_error_page_shell(title, &details_html, origin, build_version)
}

fn render_error_page_shell(
    title: &str,
    body_html: &str,
    origin: &str,
    build_version: u64,
) -> String {
    let escaped_title = escape_html(title);
    let timestamp = current_timestamp_unix_seconds();
    let dev_client = dev_client_snippet(origin);

    format!(
        r#"<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1" />
  <title>Beanstalk Dev Server Error</title>
  <style>
    :root {{
      color-scheme: dark;
      --bg: #0b0f14;
      --bg-glow: rgba(58, 92, 148, 0.24);
      --card: #131922;
      --panel: #0f141c;
      --fg: #f2f5f8;
      --accent: #ff7a90;
      --muted: #9ba8b8;
      --border: #273142;
      --link: #8bc7ff;
      --warning: #ffd479;
      --shadow: rgba(0, 0, 0, 0.35);
    }}
    body {{
      margin: 0;
      min-height: 100vh;
      background:
        radial-gradient(circle at top, var(--bg-glow), transparent 36%),
        var(--bg);
      color: var(--fg);
      font-family: Menlo, Monaco, Consolas, "Liberation Mono", monospace;
    }}
    main {{
      max-width: 1040px;
      margin: 2.5rem auto;
      padding: 0 1rem;
    }}
    .card {{
      background: var(--card);
      border: 1px solid var(--border);
      border-radius: 14px;
      box-shadow: 0 20px 60px var(--shadow);
      overflow: hidden;
    }}
    header {{
      padding: 1rem 1.2rem 0.95rem;
      border-bottom: 1px solid var(--border);
      background: rgba(255, 255, 255, 0.02);
    }}
    h1 {{
      margin: 0;
      font-size: 1.1rem;
      color: var(--accent);
    }}
    .meta {{
      padding: 0.8rem 1.2rem;
      color: var(--muted);
      font-size: 0.9rem;
      border-bottom: 1px solid var(--border);
      background: rgba(255, 255, 255, 0.02);
    }}
    .msg {{
      margin: 0;
      padding: 1.3rem;
      line-height: 1.45;
      font-size: 0.92rem;
      white-space: pre-wrap;
    }}
    .diagnostics {{
      padding: 1.1rem;
      display: grid;
      gap: 0.9rem;
    }}
    .diagnostic {{
      border: 1px solid var(--border);
      border-radius: 10px;
      background: var(--panel);
      padding: 1rem 1rem 0.95rem;
    }}
    .diagnostic-head {{
      display: flex;
      flex-wrap: wrap;
      gap: 0.55rem;
      align-items: center;
      margin-bottom: 0.7rem;
    }}
    .badge {{
      display: inline-flex;
      align-items: center;
      border-radius: 999px;
      padding: 0.22rem 0.55rem;
      font-size: 0.74rem;
      letter-spacing: 0.04em;
      background: rgba(255, 122, 144, 0.16);
      color: var(--accent);
    }}
    .badge.warning {{
      background: rgba(255, 212, 121, 0.16);
      color: var(--warning);
    }}
    .kind {{
      color: var(--muted);
      font-size: 0.86rem;
    }}
    .diagnostic-message {{
      margin: 0 0 0.8rem;
      line-height: 1.5;
    }}
    .detail-list {{
      margin: 0;
      padding: 0;
      list-style: none;
      display: grid;
      gap: 0.45rem;
    }}
    .detail-list li {{
      color: var(--muted);
      line-height: 1.45;
    }}
    .detail-label {{
      color: var(--fg);
      margin-right: 0.45rem;
    }}
    a {{
      color: var(--link);
      text-decoration: none;
    }}
    a:hover {{
      text-decoration: underline;
    }}
    .empty-state {{
      padding: 1.3rem;
      color: var(--muted);
    }}
  </style>
</head>
<body>
  <main>
    <section class="card">
      <header><h1>{escaped_title}</h1></header>
      <div class="meta">Build Version: {build_version} | Timestamp (unix): {timestamp}</div>
      {body_html}
    </section>
  </main>
  {dev_client}
</body>
</html>"#
    )
}

fn render_compiler_diagnostics(messages: &CompilerMessages, project_root: &Path) -> String {
    let mut diagnostics_html = String::from("<section class=\"diagnostics\">");

    if messages.errors.is_empty() && messages.warnings.is_empty() {
        diagnostics_html.push_str(
            "<div class=\"empty-state\">No compiler diagnostics were available for this failed build.</div>",
        );
        diagnostics_html.push_str("</section>");
        return diagnostics_html;
    }

    for error in &messages.errors {
        diagnostics_html.push_str(&render_diagnostic_card(
            error,
            project_root,
            true,
            &messages.string_table,
        ));
    }

    for warning in &messages.warnings {
        let warning_message = escape_html(&warning.msg);
        let badge_class = "badge warning";
        diagnostics_html.push_str(&format!(
            "<article class=\"diagnostic\"><div class=\"diagnostic-head\"><span class=\"{badge_class}\">WARNING</span></div><p class=\"diagnostic-message\">{warning_message}</p></article>"
        ));
    }

    diagnostics_html.push_str("</section>");
    diagnostics_html
}

fn render_diagnostic_card(
    error: &CompilerError,
    project_root: &Path,
    is_error: bool,
    string_table: &StringTable,
) -> String {
    let mut details = String::from("<ul class=\"detail-list\">");

    if let Some(source_link) =
        resolve_source_path_link(&error.location.scope, project_root, string_table)
    {
        let file_label = escape_html(&source_link.display_label);
        let file_href = escape_html(&source_link.href);
        let _ = write!(
            details,
            "<li><span class=\"detail-label\">File:</span><a href=\"{file_href}\">{file_label}</a></li>"
        );
    }

    if error.location.start_pos.line_number > 0 {
        let line_number = error.location.start_pos.line_number + 1;
        let column_number = error.location.start_pos.char_column + 1;
        let _ = write!(
            details,
            "<li><span class=\"detail-label\">Location:</span>line {line_number}, col {column_number}</li>"
        );
    }

    for guidance_line in format_error_guidance_lines(error) {
        let escaped_guidance = escape_html(&guidance_line);
        let _ = write!(details, "<li>{escaped_guidance}</li>");
    }

    details.push_str("</ul>");

    let severity_badge = if is_error { "ERROR" } else { "WARNING" };
    let badge_class = if is_error { "badge" } else { "badge warning" };
    let error_kind = escape_html(error_type_to_str(&error.error_type));
    let error_message = escape_html(&error.msg);

    format!(
        "<article class=\"diagnostic\"><div class=\"diagnostic-head\"><span class=\"{badge_class}\">{severity_badge}</span><span class=\"kind\">{error_kind}</span></div><p class=\"diagnostic-message\">{error_message}</p>{details}</article>"
    )
}

fn resolve_source_path_link(
    scope: &InternedPath,
    project_root: &Path,
    string_table: &StringTable,
) -> Option<SourcePathLink> {
    if scope.as_components().is_empty() {
        return None;
    }

    // Dev-server pages should mirror terminal diagnostics by linking header-scoped errors back to
    // the original source file while still displaying a project-relative label for quick scanning.
    let resolved_path = resolve_source_file_path(scope, string_table);
    let display_root = match std::fs::canonicalize(project_root) {
        Ok(canonical_root) => canonical_root,
        Err(_) => project_root.to_path_buf(),
    };
    let display_label = relative_display_path_from_root(&resolved_path, &display_root);
    let href = file_url_from_path(&resolved_path, false);

    Some(SourcePathLink {
        display_label,
        href,
    })
}

#[cfg(test)]
#[path = "tests/error_page_tests.rs"]
mod tests;
