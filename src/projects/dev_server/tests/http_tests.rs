//! Tests for dev-server HTTP routing during successful and failed builds.

use super::{PreparedResponse, prepare_static_response, should_serve_failed_build_html};
use crate::projects::dev_server::state::BuildState;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

fn temp_dir(prefix: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .expect("time should be after unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("beanstalk_dev_server_http_{prefix}_{unique}"))
}

fn configure_failed_build_state(
    output_dir: &Path,
    last_error_html: &str,
    entry_page_rel: Option<PathBuf>,
) -> BuildState {
    let mut build_state = BuildState::new(output_dir.to_path_buf());
    build_state.last_build_ok = false;
    build_state.last_error_html = Some(last_error_html.to_owned());
    build_state.last_build_version = 9;
    build_state.entry_page_rel = entry_page_rel;
    build_state
}

#[test]
fn failed_build_html_helper_targets_root_and_nested_html_only() {
    let mut build_state = BuildState::new(PathBuf::from("dev"));
    build_state.last_build_ok = false;

    assert!(should_serve_failed_build_html("/", None, &build_state));
    assert!(should_serve_failed_build_html(
        "/docs/basics.html",
        Some(Path::new("dev/docs/basics.html")),
        &build_state
    ));
    assert!(!should_serve_failed_build_html(
        "/styles/site.css",
        Some(Path::new("dev/styles/site.css")),
        &build_state
    ));
    assert!(!should_serve_failed_build_html(
        "/docs/basics.html",
        None,
        &build_state
    ));
}

#[test]
fn nested_html_request_uses_stored_error_page_during_failed_build() {
    let root = temp_dir("nested_html");
    let output_dir = root.join("dev");
    fs::create_dir_all(output_dir.join("docs")).expect("should create docs output dir");
    fs::write(
        output_dir.join("docs/basics.html"),
        "<html><body>stale success</body></html>",
    )
    .expect("should write stale html");

    let error_html = "<html><body>compiler exploded</body></html>";
    let build_state =
        configure_failed_build_state(&output_dir, error_html, Some(PathBuf::from("index.html")));

    match prepare_static_response("/docs/basics.html", &build_state) {
        PreparedResponse::Text {
            status_line,
            content_type,
            body,
        } => {
            assert_eq!(status_line, "200 OK");
            assert_eq!(content_type, "text/html; charset=utf-8");
            assert!(body.contains("compiler exploded"));
            assert!(!body.contains("stale success"));
        }
        PreparedResponse::File { .. } => {
            panic!("nested html route should render stored error page")
        }
    }

    fs::remove_dir_all(&root).expect("should remove temp dir");
}

#[test]
fn failed_build_keeps_css_js_and_image_assets_reachable() {
    let root = temp_dir("assets");
    let output_dir = root.join("dev");
    fs::create_dir_all(output_dir.join("styles")).expect("should create styles dir");
    fs::create_dir_all(output_dir.join("scripts")).expect("should create scripts dir");
    fs::create_dir_all(output_dir.join("images")).expect("should create images dir");
    fs::write(output_dir.join("styles/site.css"), "body { color: red; }")
        .expect("should write css");
    fs::write(output_dir.join("scripts/app.js"), "console.log('hello');").expect("should write js");
    fs::write(output_dir.join("images/icon.png"), [0x89, b'P', b'N', b'G'])
        .expect("should write png");

    let build_state = configure_failed_build_state(
        &output_dir,
        "<html><body>error</body></html>",
        Some(PathBuf::from("index.html")),
    );

    match prepare_static_response("/styles/site.css", &build_state) {
        PreparedResponse::File { path, content_type } => {
            assert_eq!(content_type, "text/css; charset=utf-8");
            assert_eq!(
                fs::read_to_string(path).expect("css file should be readable"),
                "body { color: red; }"
            );
        }
        PreparedResponse::Text { .. } => panic!("css request should keep serving the asset"),
    }

    match prepare_static_response("/scripts/app.js", &build_state) {
        PreparedResponse::File { path, content_type } => {
            assert_eq!(content_type, "application/javascript; charset=utf-8");
            assert_eq!(
                fs::read_to_string(path).expect("js file should be readable"),
                "console.log('hello');"
            );
        }
        PreparedResponse::Text { .. } => panic!("js request should keep serving the asset"),
    }

    match prepare_static_response("/images/icon.png", &build_state) {
        PreparedResponse::File { path, content_type } => {
            assert_eq!(content_type, "image/png");
            assert_eq!(
                fs::read(path).expect("png file should be readable"),
                vec![0x89, b'P', b'N', b'G']
            );
        }
        PreparedResponse::Text { .. } => panic!("image request should keep serving the asset"),
    }

    fs::remove_dir_all(&root).expect("should remove temp dir");
}

#[test]
fn failed_build_traversal_request_still_returns_not_found() {
    let root = temp_dir("traversal");
    let output_dir = root.join("dev");
    fs::create_dir_all(&output_dir).expect("should create output dir");
    let build_state = configure_failed_build_state(
        &output_dir,
        "<html><body>error</body></html>",
        Some(PathBuf::from("index.html")),
    );

    match prepare_static_response("/../secret.txt", &build_state) {
        PreparedResponse::Text {
            status_line,
            content_type,
            body,
        } => {
            assert_eq!(status_line, "404 NOT FOUND");
            assert_eq!(content_type, "text/plain; charset=utf-8");
            assert_eq!(body, "Not Found");
        }
        PreparedResponse::File { .. } => panic!("traversal request should return not found"),
    }

    fs::remove_dir_all(&root).expect("should remove temp dir");
}

#[test]
fn root_request_uses_failed_build_error_page_without_entry_page() {
    let root = temp_dir("root_error");
    let output_dir = root.join("dev");
    fs::create_dir_all(&output_dir).expect("should create output dir");
    let build_state =
        configure_failed_build_state(&output_dir, "<html><body>root error</body></html>", None);

    match prepare_static_response("/", &build_state) {
        PreparedResponse::Text {
            status_line,
            content_type,
            body,
        } => {
            assert_eq!(status_line, "200 OK");
            assert_eq!(content_type, "text/html; charset=utf-8");
            assert!(body.contains("root error"));
        }
        PreparedResponse::File { .. } => panic!("root request should render the failed-build page"),
    }

    fs::remove_dir_all(&root).expect("should remove temp dir");
}
