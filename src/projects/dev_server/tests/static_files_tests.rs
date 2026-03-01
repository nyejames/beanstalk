//! Tests for static path resolution, content-type mapping, and HTML snippet injection.

use super::{ResolvePathError, content_type_for_path, inject_dev_client, resolve_request_path};
use std::path::Path;

#[test]
fn injection_happens_before_closing_body_once() {
    let html = "<html><body><h1>Hello</h1></body></html>";
    let injected = inject_dev_client(html);
    assert!(injected.contains("EventSource('/__beanstalk/events')"));
    assert_eq!(
        injected
            .matches("EventSource('/__beanstalk/events')")
            .count(),
        1
    );
    assert!(injected.find("</body>").expect("should contain body close") > 0);

    let reinjected = inject_dev_client(&injected);
    assert_eq!(
        reinjected
            .matches("EventSource('/__beanstalk/events')")
            .count(),
        1,
        "snippet should not be injected twice"
    );
}

#[test]
fn content_type_map_covers_common_extensions() {
    assert_eq!(
        content_type_for_path(Path::new("index.html")),
        "text/html; charset=utf-8"
    );
    assert_eq!(
        content_type_for_path(Path::new("bundle.js")),
        "application/javascript; charset=utf-8"
    );
    assert_eq!(content_type_for_path(Path::new("image.png")), "image/png");
}

#[test]
fn resolve_path_rejects_traversal() {
    let output_dir = Path::new("/tmp/project/dev");
    let err =
        resolve_request_path("/../secret.txt", output_dir, None).expect_err("traversal fails");
    assert_eq!(err, ResolvePathError::InvalidPath);
}

#[test]
fn root_uses_entry_page_when_available() {
    let output_dir = Path::new("/tmp/project/dev");
    let resolved = resolve_request_path("/", output_dir, Some(Path::new("index.html")))
        .expect("root should resolve to entry page");
    assert_eq!(resolved, output_dir.join("index.html"));
}

#[test]
fn nested_routes_resolve_under_output_dir() {
    let output_dir = Path::new("/tmp/project/dev");
    let resolved = resolve_request_path("/docs/basics.html", output_dir, None)
        .expect("nested route should resolve");
    assert_eq!(resolved, output_dir.join("docs/basics.html"));
}
