//! Tests for shared routing policy parsing.

use super::{HtmlRoutingConfig, PageUrlStyle, parse_html_routing_config};
use crate::projects::settings::Config;
use std::path::PathBuf;

#[test]
fn defaults_are_applied_when_settings_are_missing() {
    let config = Config::new(PathBuf::from("project"));
    assert_eq!(
        parse_html_routing_config(&config).expect("defaults should parse"),
        HtmlRoutingConfig::default()
    );
}

#[test]
fn parser_accepts_valid_overrides() {
    let mut config = Config::new(PathBuf::from("project"));
    config.settings.insert(
        String::from("page_url_style"),
        String::from("no_trailing_slash"),
    );
    config
        .settings
        .insert(String::from("redirect_index_html"), String::from("false"));

    let parsed = parse_html_routing_config(&config).expect("valid settings should parse");
    assert_eq!(parsed.page_url_style, PageUrlStyle::NoTrailingSlash);
    assert!(!parsed.redirect_index_html);
}

#[test]
fn parser_rejects_invalid_page_url_style() {
    let mut config = Config::new(PathBuf::from("project"));
    config
        .settings
        .insert(String::from("page_url_style"), String::from("slashy"));

    let error = parse_html_routing_config(&config).expect_err("invalid value should fail");
    assert_eq!(
        error.error_type,
        crate::compiler_frontend::compiler_errors::ErrorType::Config
    );
    assert!(error.msg.contains("#page_url_style"));
}

#[test]
fn parser_rejects_invalid_redirect_index_html() {
    let mut config = Config::new(PathBuf::from("project"));
    config
        .settings
        .insert(String::from("redirect_index_html"), String::from("yes"));

    let error = parse_html_routing_config(&config).expect_err("invalid value should fail");
    assert_eq!(
        error.error_type,
        crate::compiler_frontend::compiler_errors::ErrorType::Config
    );
    assert!(error.msg.contains("#redirect_index_html"));
}
