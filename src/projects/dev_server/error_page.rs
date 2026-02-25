//! Error page rendering for dev-server build/runtime failures.
//!
//! Compiler diagnostics are converted into escaped HTML so failed builds still render safely and
//! remain connected to hot reload via the injected SSE client snippet.

use crate::compiler_frontend::compiler_errors::{CompilerMessages, error_type_to_str};
use std::fmt::Write;
use std::time::{SystemTime, UNIX_EPOCH};

pub const DEV_CLIENT_MARKER: &str = "<!-- beanstalk-dev-client -->";

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
        if !error.location.scope.as_os_str().is_empty() {
            let _ = writeln!(output, "  at: {}", error.location.scope.display());
        }
        if error.location.start_pos.line_number > 0 {
            let _ = writeln!(
                output,
                "  line: {}, col: {}",
                error.location.start_pos.line_number + 1,
                error.location.start_pos.char_column + 1
            );
        }
    }

    for warning in &messages.warnings {
        let _ = writeln!(output, "[WARNING] {}", warning.msg);
        if !warning.location.scope.as_os_str().is_empty() {
            let _ = writeln!(output, "  at: {}", warning.location.scope.display());
        }
    }

    output
}

pub fn render_compiler_error_page(messages: &CompilerMessages, build_version: u64) -> String {
    let details = format_compiler_messages(messages);
    render_runtime_error_page("Build Failed", &details, build_version)
}

pub fn render_runtime_error_page(title: &str, details: &str, build_version: u64) -> String {
    let escaped_title = escape_html(title);
    let escaped_details = escape_html(details);
    let timestamp = current_timestamp_unix_seconds();

    format!(
        r#"<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1" />
  <title>Beanstalk Dev Server Error</title>
  <style>
    :root {{
      color-scheme: light;
      --bg: #f7f7f9;
      --card: #ffffff;
      --fg: #1d1d21;
      --accent: #9c0f2b;
      --muted: #64646e;
      --border: #d8d8df;
    }}
    body {{
      margin: 0;
      background: var(--bg);
      color: var(--fg);
      font-family: Menlo, Monaco, Consolas, "Liberation Mono", monospace;
    }}
    main {{
      max-width: 980px;
      margin: 2.25rem auto;
      padding: 0 1rem;
    }}
    .card {{
      background: var(--card);
      border: 1px solid var(--border);
      border-radius: 10px;
      box-shadow: 0 6px 30px rgba(0, 0, 0, 0.05);
      overflow: hidden;
    }}
    header {{
      padding: 1rem 1.2rem;
      border-bottom: 1px solid var(--border);
      background: #fff;
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
      background: #fcfcfd;
    }}
    pre {{
      margin: 0;
      padding: 1.2rem;
      overflow-x: auto;
      line-height: 1.45;
      font-size: 0.92rem;
    }}
  </style>
</head>
<body>
  <main>
    <section class="card">
      <header><h1>{escaped_title}</h1></header>
      <div class="meta">Build Version: {build_version} | Timestamp (unix): {timestamp}</div>
      <pre>{escaped_details}</pre>
    </section>
  </main>
  {DEV_CLIENT_MARKER}
  <script>
    (() => {{
      const source = new EventSource('/__beanstalk/events');
      source.addEventListener('reload', () => window.location.reload());
    }})();
  </script>
</body>
</html>"#
    )
}

#[cfg(test)]
#[path = "tests/error_page_tests.rs"]
mod tests;
