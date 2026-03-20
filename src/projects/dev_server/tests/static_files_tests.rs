//! Tests for static path resolution, content-type mapping, and HTML snippet injection.

use super::{
    ResolvedRequest, ResolvedRequestKind, content_type_for_path, inject_dev_client, resolve_request,
};
use crate::projects::routing::{HtmlRoutingConfig, PageUrlStyle};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

fn temp_dir(prefix: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .expect("time should be after unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("beanstalk_dev_server_static_{prefix}_{unique}"))
}

fn routing(page_url_style: PageUrlStyle, redirect_index_html: bool) -> HtmlRoutingConfig {
    HtmlRoutingConfig {
        page_url_style,
        redirect_index_html,
    }
}

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
    let resolved = resolve_request(
        "/../secret.txt",
        None,
        output_dir,
        Some(Path::new("index.html")),
        HtmlRoutingConfig::default(),
    );
    assert_eq!(resolved, ResolvedRequest::InvalidPath);
}

#[test]
fn root_uses_entry_page_when_available() {
    let root = temp_dir("root_page");
    let output_dir = root.join("dev");
    fs::create_dir_all(&output_dir).expect("should create output dir");
    fs::write(output_dir.join("index.html"), "<h1>home</h1>").expect("should write root page");

    let resolved = resolve_request(
        "/",
        None,
        &output_dir,
        Some(Path::new("index.html")),
        HtmlRoutingConfig::default(),
    );

    assert_eq!(
        resolved,
        ResolvedRequest::File {
            path: output_dir.join("index.html"),
            kind: ResolvedRequestKind::PageHtml,
        }
    );

    fs::remove_dir_all(&root).expect("should remove temp dir");
}

#[test]
fn trailing_slash_mode_redirects_non_canonical_page_forms() {
    let root = temp_dir("trailing_slash");
    let output_dir = root.join("dev");
    fs::create_dir_all(output_dir.join("about")).expect("should create about dir");
    fs::write(output_dir.join("about/index.html"), "<h1>about</h1>").expect("should write page");

    let cfg = routing(PageUrlStyle::TrailingSlash, true);

    assert_eq!(
        resolve_request(
            "/about",
            None,
            &output_dir,
            Some(Path::new("index.html")),
            cfg
        ),
        ResolvedRequest::Redirect {
            location: String::from("/about/"),
        }
    );
    assert_eq!(
        resolve_request(
            "/about/index.html",
            Some("x=1"),
            &output_dir,
            Some(Path::new("index.html")),
            cfg
        ),
        ResolvedRequest::Redirect {
            location: String::from("/about/?x=1"),
        }
    );
    assert_eq!(
        resolve_request(
            "/about/",
            None,
            &output_dir,
            Some(Path::new("index.html")),
            cfg
        ),
        ResolvedRequest::File {
            path: output_dir.join("about/index.html"),
            kind: ResolvedRequestKind::PageHtml,
        }
    );

    fs::remove_dir_all(&root).expect("should remove temp dir");
}

#[test]
fn no_trailing_slash_mode_redirects_trailing_page_form() {
    let root = temp_dir("no_trailing_slash");
    let output_dir = root.join("dev");
    fs::create_dir_all(output_dir.join("about")).expect("should create about dir");
    fs::write(output_dir.join("about/index.html"), "<h1>about</h1>").expect("should write page");

    let cfg = routing(PageUrlStyle::NoTrailingSlash, true);

    assert_eq!(
        resolve_request(
            "/about/",
            None,
            &output_dir,
            Some(Path::new("index.html")),
            cfg
        ),
        ResolvedRequest::Redirect {
            location: String::from("/about"),
        }
    );
    assert_eq!(
        resolve_request(
            "/about",
            None,
            &output_dir,
            Some(Path::new("index.html")),
            cfg
        ),
        ResolvedRequest::File {
            path: output_dir.join("about/index.html"),
            kind: ResolvedRequestKind::PageHtml,
        }
    );
    assert_eq!(
        resolve_request(
            "/about/index.html",
            None,
            &output_dir,
            Some(Path::new("index.html")),
            cfg
        ),
        ResolvedRequest::Redirect {
            location: String::from("/about"),
        }
    );

    fs::remove_dir_all(&root).expect("should remove temp dir");
}

#[test]
fn ignore_mode_serves_both_slash_forms_but_can_still_redirect_index_alias() {
    let root = temp_dir("ignore_mode");
    let output_dir = root.join("dev");
    fs::create_dir_all(output_dir.join("about")).expect("should create about dir");
    fs::write(output_dir.join("about/index.html"), "<h1>about</h1>").expect("should write page");

    let cfg = routing(PageUrlStyle::Ignore, true);

    assert_eq!(
        resolve_request(
            "/about",
            None,
            &output_dir,
            Some(Path::new("index.html")),
            cfg
        ),
        ResolvedRequest::File {
            path: output_dir.join("about/index.html"),
            kind: ResolvedRequestKind::PageHtml,
        }
    );
    assert_eq!(
        resolve_request(
            "/about/",
            None,
            &output_dir,
            Some(Path::new("index.html")),
            cfg
        ),
        ResolvedRequest::File {
            path: output_dir.join("about/index.html"),
            kind: ResolvedRequestKind::PageHtml,
        }
    );
    assert_eq!(
        resolve_request(
            "/about/index.html",
            None,
            &output_dir,
            Some(Path::new("index.html")),
            cfg
        ),
        ResolvedRequest::Redirect {
            location: String::from("/about/"),
        }
    );

    fs::remove_dir_all(&root).expect("should remove temp dir");
}

#[test]
fn exact_assets_are_served_without_page_canonicalization() {
    let root = temp_dir("exact_assets");
    let output_dir = root.join("dev");
    fs::create_dir_all(output_dir.join("images")).expect("should create images dir");
    fs::write(output_dir.join("app.js"), "console.log('ok');").expect("should write js");
    fs::write(output_dir.join("images/logo.png"), [0x89, b'P', b'N', b'G'])
        .expect("should write png");
    fs::write(output_dir.join("CNAME"), "example.com").expect("should write extensionless file");

    let cfg = HtmlRoutingConfig::default();

    assert_eq!(
        resolve_request(
            "/app.js",
            None,
            &output_dir,
            Some(Path::new("index.html")),
            cfg
        ),
        ResolvedRequest::File {
            path: output_dir.join("app.js"),
            kind: ResolvedRequestKind::Asset,
        }
    );
    assert_eq!(
        resolve_request(
            "/app.js/",
            None,
            &output_dir,
            Some(Path::new("index.html")),
            cfg
        ),
        ResolvedRequest::NotFound
    );
    assert_eq!(
        resolve_request(
            "/images/logo",
            None,
            &output_dir,
            Some(Path::new("index.html")),
            cfg
        ),
        ResolvedRequest::NotFound
    );
    assert_eq!(
        resolve_request(
            "/CNAME",
            None,
            &output_dir,
            Some(Path::new("index.html")),
            cfg
        ),
        ResolvedRequest::File {
            path: output_dir.join("CNAME"),
            kind: ResolvedRequestKind::Asset,
        }
    );

    fs::remove_dir_all(&root).expect("should remove temp dir");
}
