//! Tests for HTML document-shell config parsing.

use super::*;
use crate::compiler_frontend::compiler_errors::{ErrorLocation, ErrorType};
use crate::projects::settings::Config;
use std::path::PathBuf;

fn project_config() -> Config {
    Config::new(PathBuf::from("project"))
}

fn set_setting(config: &mut Config, key: &str, value: &str) {
    config.settings.insert(key.to_owned(), value.to_owned());
}

#[test]
fn defaults_are_applied_when_settings_are_missing() {
    let config = project_config();
    assert_eq!(
        parse_html_document_config(&config).expect("defaults should parse"),
        HtmlDocumentConfig::default()
    );
}

#[test]
fn parser_accepts_valid_overrides() {
    let mut config = project_config();
    set_setting(&mut config, "html_lang", "en-GB");
    set_setting(&mut config, "html_title_prefix", "Docs | ");
    set_setting(&mut config, "html_title_postfix", " | Beanstalk");
    set_setting(&mut config, "html_favicon", "/assets/favicon.ico");
    set_setting(&mut config, "html_inject_charset", "false");
    set_setting(&mut config, "html_inject_viewport", "false");
    set_setting(&mut config, "html_inject_color_scheme", "false");
    set_setting(&mut config, "html_inject_core_css", "false");
    set_setting(&mut config, "html_body_style", "margin: 0;");

    let parsed = parse_html_document_config(&config).expect("valid settings should parse");
    assert_eq!(parsed.lang, "en-GB");
    assert_eq!(parsed.title_prefix, "Docs | ");
    assert_eq!(parsed.title_postfix, " | Beanstalk");
    assert_eq!(parsed.favicon, Some(String::from("/assets/favicon.ico")));
    assert!(!parsed.inject_charset);
    assert!(!parsed.inject_viewport);
    assert!(!parsed.inject_color_scheme);
    assert!(!parsed.inject_core_css);
    assert_eq!(parsed.body_style, "margin: 0;");
}

#[test]
fn parser_rejects_empty_lang() {
    let mut config = project_config();
    set_setting(&mut config, "html_lang", "");

    let error = parse_html_document_config(&config).expect_err("empty lang should fail");
    assert_eq!(error.error_type, ErrorType::Config);
    assert!(error.msg.contains("#html_lang"));
}

#[test]
fn parser_rejects_invalid_bool_values() {
    let mut config = project_config();
    set_setting(&mut config, "html_inject_core_css", "yes");

    let error = parse_html_document_config(&config).expect_err("invalid bool should fail");
    assert_eq!(error.error_type, ErrorType::Config);
    assert!(error.msg.contains("#html_inject_core_css"));
}

#[test]
fn parser_uses_precise_location_from_setting_locations() {
    let mut config = project_config();
    set_setting(&mut config, "html_lang", "");
    let precise_location = ErrorLocation::new(
        PathBuf::from("project/#config.bst"),
        Default::default(),
        Default::default(),
    );
    config
        .setting_locations
        .insert(String::from("html_lang"), precise_location.clone());

    let error = parse_html_document_config(&config).expect_err("invalid lang should fail");
    assert_eq!(error.location.scope, precise_location.scope);
}

#[test]
fn parser_falls_back_to_config_file_location() {
    let mut config = project_config();
    set_setting(&mut config, "html_inject_core_css", "invalid");

    let error = parse_html_document_config(&config).expect_err("invalid bool should fail");
    assert_eq!(error.location.scope, PathBuf::from("project/#config.bst"));
}
