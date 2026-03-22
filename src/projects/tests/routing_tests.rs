//! Tests for shared routing policy parsing.

use super::{
    HtmlSiteConfig, PageUrlStyle, parse_html_site_config, prefix_origin, strip_origin_prefix,
};
use crate::projects::settings::Config;
use std::path::PathBuf;

#[test]
fn defaults_are_applied_when_settings_are_missing() {
    let config = Config::new(PathBuf::from("project"));
    assert_eq!(
        parse_html_site_config(&config).expect("defaults should parse"),
        HtmlSiteConfig::default()
    );
}

#[test]
fn parser_accepts_valid_overrides() {
    let mut config = Config::new(PathBuf::from("project"));
    config
        .settings
        .insert(String::from("origin"), String::from("/beanstalk"));
    config.settings.insert(
        String::from("page_url_style"),
        String::from("no_trailing_slash"),
    );
    config
        .settings
        .insert(String::from("redirect_index_html"), String::from("false"));

    let parsed = parse_html_site_config(&config).expect("valid settings should parse");
    assert_eq!(parsed.origin, "/beanstalk");
    assert_eq!(parsed.page_url_style, PageUrlStyle::NoTrailingSlash);
    assert!(!parsed.redirect_index_html);
}

#[test]
fn parser_rejects_invalid_origin() {
    let mut config = Config::new(PathBuf::from("project"));

    // No leading slash
    config
        .settings
        .insert(String::from("origin"), String::from("beanstalk"));
    assert!(parse_html_site_config(&config).is_err());

    // Trailing slash (not root)
    config
        .settings
        .insert(String::from("origin"), String::from("/beanstalk/"));
    assert!(parse_html_site_config(&config).is_err());

    // Empty
    config
        .settings
        .insert(String::from("origin"), String::from(""));
    assert!(parse_html_site_config(&config).is_err());

    // Query string
    config
        .settings
        .insert(String::from("origin"), String::from("/?x=1"));
    assert!(parse_html_site_config(&config).is_err());
}

#[test]
fn parser_rejects_invalid_page_url_style() {
    let mut config = Config::new(PathBuf::from("project"));
    config
        .settings
        .insert(String::from("page_url_style"), String::from("slashy"));

    let error = parse_html_site_config(&config).expect_err("invalid value should fail");
    assert_eq!(
        error.error_type,
        crate::compiler_frontend::compiler_errors::ErrorType::Config
    );
    assert!(error.msg.contains("#page_url_style"));
    
    // Verify the suggestion contains the actual accepted values
    let suggestion = error.metadata.get(
        &crate::compiler_frontend::compiler_errors::ErrorMetaDataKey::PrimarySuggestion
    ).expect("error should have suggestion");
    assert!(suggestion.contains("trailing_slash"), "suggestion should mention 'trailing_slash'");
    assert!(suggestion.contains("no_trailing_slash"), "suggestion should mention 'no_trailing_slash'");
    assert!(suggestion.contains("ignore"), "suggestion should mention 'ignore'");
}

#[test]
fn parser_rejects_invalid_redirect_index_html() {
    let mut config = Config::new(PathBuf::from("project"));
    config
        .settings
        .insert(String::from("redirect_index_html"), String::from("yes"));

    let error = parse_html_site_config(&config).expect_err("invalid value should fail");
    assert_eq!(
        error.error_type,
        crate::compiler_frontend::compiler_errors::ErrorType::Config
    );
    assert!(error.msg.contains("#redirect_index_html"));
}

#[test]
fn parser_uses_precise_location_from_setting_locations() {
    use crate::compiler_frontend::compiler_errors::ErrorLocation;
    
    let mut config = Config::new(PathBuf::from("project"));
    config
        .settings
        .insert(String::from("origin"), String::from("invalid"));
    
    // Store a precise location for the origin key
    let precise_location = ErrorLocation::new(
        PathBuf::from("project/#config.bst"),
        Default::default(),
        Default::default(),
    );
    config.setting_locations.insert(String::from("origin"), precise_location.clone());
    
    let error = parse_html_site_config(&config).expect_err("invalid origin should fail");
    
    // Verify the error uses the precise location from setting_locations
    assert_eq!(error.location.scope, precise_location.scope);
}

#[test]
fn parser_falls_back_to_file_location_when_key_not_in_setting_locations() {
    let mut config = Config::new(PathBuf::from("project"));
    config
        .settings
        .insert(String::from("origin"), String::from("invalid"));
    
    // Don't add the key to setting_locations
    
    let error = parse_html_site_config(&config).expect_err("invalid origin should fail");
    
    // Verify the error falls back to file-level location
    assert_eq!(error.location.scope, PathBuf::from("project/#config.bst"));
}


#[test]
fn prefix_origin_works() {
    assert_eq!(prefix_origin("/", "/docs/"), "/docs/");
    assert_eq!(prefix_origin("/beanstalk", "/docs/"), "/beanstalk/docs/");
    assert_eq!(prefix_origin("/beanstalk", "/"), "/beanstalk/");
}

#[test]
fn strip_origin_prefix_works() {
    assert_eq!(
        strip_origin_prefix("/beanstalk/docs/", "/beanstalk"),
        Some(String::from("/docs/"))
    );
    assert_eq!(
        strip_origin_prefix("/beanstalk/", "/beanstalk"),
        Some(String::from("/"))
    );
    assert_eq!(
        strip_origin_prefix("/beanstalk", "/beanstalk"),
        Some(String::from("/"))
    );
    assert_eq!(strip_origin_prefix("/docs/", "/beanstalk"), None);
    assert_eq!(strip_origin_prefix("/beanstalkish/", "/beanstalk"), None);
}
