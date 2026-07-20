//! Node-backed rendered-output assertions for HTML integration artifacts.
//!
//! WHAT: extracts emitted scripts, executes them in the minimal Node harness and checks captured
//!       console and fragment output.
//! WHY: runtime semantics belong to one harness so rendered assertions do not inspect generated
//!      JavaScript structure or create a second execution path.

use super::super::{ArtifactKind, FailureKind};
use crate::build_system::build::BuildResult;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

static RENDER_HARNESS_COUNTER: AtomicU64 = AtomicU64::new(0);

pub(super) fn validate_rendered_output(
    build_result: &BuildResult,
    contains: &[String],
    not_contains: &[String],
) -> Option<(String, FailureKind)> {
    let Some(index_html_file) = super::artifacts::find_output_file(build_result, "index.html")
    else {
        return Some((
            "rendered_output assertion requires 'index.html', but it was not produced.".to_string(),
            FailureKind::HarnessFailed,
        ));
    };

    let Some(html) = super::artifacts::output_text_content(index_html_file, ArtifactKind::Html)
    else {
        return Some((
            "rendered_output assertion requires 'index.html' to be an HTML artifact.".to_string(),
            FailureKind::HarnessFailed,
        ));
    };

    let rendered = match execute_html_in_node(html) {
        Ok(output) => output,
        Err(reason) => return Some((reason, FailureKind::HarnessFailed)),
    };

    validate_rendered_output_fragments(&combine_rendered_output(&rendered), contains, not_contains)
}

/// Validates rendered fragments independently of harness execution.
///
/// WHAT: checks required and forbidden fragments against precomputed rendered output.
/// WHY: keeps harness failures separate from semantic mismatch failures and supports focused
///      self-tests without requiring a Node runtime.
pub(super) fn validate_rendered_output_fragments(
    rendered_output: &str,
    contains: &[String],
    not_contains: &[String],
) -> Option<(String, FailureKind)> {
    for required in contains {
        if !rendered_output.contains(required.as_str()) {
            return Some((
                format!(
                    "Rendered output did not contain required fragment '{required}'.\nActual output:\n{rendered_output}"
                ),
                FailureKind::RenderedOutputMismatch,
            ));
        }
    }

    for forbidden in not_contains {
        if rendered_output.contains(forbidden.as_str()) {
            return Some((
                format!(
                    "Rendered output contained forbidden fragment '{forbidden}'.\nActual output:\n{rendered_output}"
                ),
                FailureKind::RenderedOutputMismatch,
            ));
        }
    }

    None
}

struct RenderedOutput {
    io_lines: Vec<String>,
    slot_outputs: Vec<SlotOutput>,
}

struct SlotOutput {
    html: String,
}

fn combine_rendered_output(output: &RenderedOutput) -> String {
    let mut parts = output.io_lines.clone();
    for slot in &output.slot_outputs {
        parts.push(slot.html.clone());
    }
    parts.join("\n")
}

/// Executes the script blocks from compiled HTML through a minimal Node.js harness.
///
/// The harness stubs `document.getElementById` to capture `insertAdjacentHTML` calls, intercepts
/// `console.log` and emits a JSON summary after one microtask tick so runtime assertions can
/// observe batched reactive flushes queued by the page bundle.
fn execute_html_in_node(html: &str) -> Result<RenderedOutput, String> {
    let scripts = extract_script_blocks(html);
    if scripts.is_empty() {
        return Err(
            "rendered_output: no <script> blocks found in 'index.html'. \
             Ensure the fixture produces runtime output."
                .to_string(),
        );
    }

    let harness = build_node_harness(&scripts);

    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let sequence = RENDER_HARNESS_COUNTER.fetch_add(1, Ordering::Relaxed);
    let temp_path = std::env::temp_dir().join(format!(
        "bst_render_harness_{}_{}_{}.js",
        std::process::id(),
        unique,
        sequence
    ));

    std::fs::write(&temp_path, &harness)
        .map_err(|error| format!("rendered_output: failed to write node harness: {error}"))?;

    let output = std::process::Command::new("node")
        .arg(&temp_path)
        .output()
        .map_err(|error| {
            let _ = remove_temp_harness_file_with_retry(&temp_path);
            format!(
                "rendered_output: failed to invoke node: {error}. \
                 Ensure 'node' is on PATH to use rendered_output_contains."
            )
        })?;

    let _ = remove_temp_harness_file_with_retry(&temp_path);

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "rendered_output: node harness execution failed:\n{stderr}"
        ));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_harness_output(stdout.trim())
}

/// Best-effort cleanup for temporary Node harness files.
///
/// WHAT: retries removal briefly to tolerate Windows file-sharing race windows after process exit.
/// WHY: cleanup races must not surface as semantic rendered-output mismatches.
fn remove_temp_harness_file_with_retry(path: &Path) -> Result<(), std::io::Error> {
    const MAX_ATTEMPTS: usize = 6;
    const BASE_RETRY_DELAY_MS: u64 = 8;

    let mut last_error = None;
    for attempt in 0..MAX_ATTEMPTS {
        match std::fs::remove_file(path) {
            Ok(()) => return Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => {
                last_error = Some(error);
                if attempt + 1 < MAX_ATTEMPTS {
                    std::thread::sleep(Duration::from_millis(
                        BASE_RETRY_DELAY_MS * (attempt as u64 + 1),
                    ));
                }
            }
        }
    }

    Err(last_error.unwrap_or_else(|| std::io::Error::other("failed to remove file")))
}

fn build_node_harness(scripts: &[String]) -> String {
    let prefix = r#"const __bst_io = [];
const __bst_slots = [];
const __bst_slot_by_id = new Map();
console.log = (...args) => __bst_io.push(args.map(String).join(' '));
function __bst_get_slot(id) {
    if (!__bst_slot_by_id.has(id)) {
        const slot = {
            id,
            innerHTML: "",
            insertAdjacentHTML: (_, html) => {
                const text = String(html);
                slot.innerHTML += text;
                __bst_slots.push({ id, html: text });
            }
        };
        __bst_slot_by_id.set(id, slot);
    }
    return __bst_slot_by_id.get(id);
}
const document = {
    getElementById: __bst_get_slot
};
"#;

    let suffix = r#"
Promise.resolve().then(() => {
    process.stdout.write(JSON.stringify({ io: __bst_io, slots: __bst_slots }) + '\n');
});
"#;

    format!("{prefix}{}\n{suffix}", scripts.join("\n"))
}

/// Extracts the text content between `<script>` and `</script>` tag pairs.
fn extract_script_blocks(html: &str) -> Vec<String> {
    let mut blocks = Vec::new();
    let mut search_from = 0;

    while let Some(open_end) = find_script_open_end(html, search_from) {
        let close_tag = "</script>";
        let Some(close_start) = html[open_end..].find(close_tag) else {
            break;
        };
        let block = &html[open_end..open_end + close_start];
        if !block.trim().is_empty() {
            blocks.push(block.to_owned());
        }
        search_from = open_end + close_start + close_tag.len();
    }

    blocks
}

/// Finds the end position of a `<script>` opening tag starting from `from`.
fn find_script_open_end(html: &str, from: usize) -> Option<usize> {
    let slice = &html[from..];
    let tag_start = slice.find("<script")?;
    let tag_slice = &slice[tag_start..];
    let close_bracket = tag_slice.find('>')?;
    Some(from + tag_start + close_bracket + 1)
}

fn parse_harness_output(json: &str) -> Result<RenderedOutput, String> {
    let value: serde_json::Value = serde_json::from_str(json).map_err(|error| {
        format!("rendered_output: failed to parse node harness JSON output: {error}\nRaw: {json}")
    })?;

    let io_lines = value["io"]
        .as_array()
        .ok_or("rendered_output: 'io' field missing from harness output")?
        .iter()
        .filter_map(|value| value.as_str().map(str::to_owned))
        .collect();

    let slot_outputs = value["slots"]
        .as_array()
        .ok_or("rendered_output: 'slots' field missing from harness output")?
        .iter()
        .filter_map(|value| {
            let html = value["html"].as_str()?.to_owned();
            Some(SlotOutput { html })
        })
        .collect();

    Ok(RenderedOutput {
        io_lines,
        slot_outputs,
    })
}
