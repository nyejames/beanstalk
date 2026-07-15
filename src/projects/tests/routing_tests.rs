//! Tests for shared routing policy parsing.

use super::{
    HtmlSiteConfig, PageUrlStyle, parse_html_site_config, prefix_origin, strip_origin_prefix,
};
use crate::compiler_frontend::compiler_messages::{DiagnosticPayload, InvalidConfigReason};
use crate::compiler_frontend::symbols::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::projects::settings::Config;
use std::path::PathBuf;

#[test]
fn defaults_are_applied_when_settings_are_missing() {
    let config = Config::new(PathBuf::from("project"));
    let mut string_table = StringTable::new();
    assert_eq!(
        parse_html_site_config(&config, &mut string_table).expect("defaults should parse"),
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

    let mut string_table = StringTable::new();
    let parsed =
        parse_html_site_config(&config, &mut string_table).expect("valid settings should parse");
    assert_eq!(parsed.origin, "/beanstalk");
    assert_eq!(parsed.page_url_style, PageUrlStyle::NoTrailingSlash);
    assert!(!parsed.redirect_index_html);
}

#[test]
fn parser_rejects_invalid_origin() {
    let mut config = Config::new(PathBuf::from("project"));
    let mut string_table = StringTable::new();

    // No leading slash
    config
        .settings
        .insert(String::from("origin"), String::from("beanstalk"));
    assert!(parse_html_site_config(&config, &mut string_table).is_err());

    // Trailing slash (not root)
    config
        .settings
        .insert(String::from("origin"), String::from("/beanstalk/"));
    assert!(parse_html_site_config(&config, &mut string_table).is_err());

    // Empty
    config
        .settings
        .insert(String::from("origin"), String::from(""));
    assert!(parse_html_site_config(&config, &mut string_table).is_err());

    // Query string
    config
        .settings
        .insert(String::from("origin"), String::from("/?x=1"));
    assert!(parse_html_site_config(&config, &mut string_table).is_err());
}

#[test]
fn parser_rejects_invalid_page_url_style() {
    let mut config = Config::new(PathBuf::from("project"));
    config
        .settings
        .insert(String::from("page_url_style"), String::from("slashy"));

    let mut string_table = StringTable::new();
    let error =
        parse_html_site_config(&config, &mut string_table).expect_err("invalid value should fail");
    let diagnostic = error.diagnostic().expect("config error should be typed");
    let DiagnosticPayload::InvalidConfig {
        reason: InvalidConfigReason::InvalidProjectSettingValue { expected, .. },
        ..
    } = &diagnostic.payload
    else {
        panic!("expected invalid project setting diagnostic");
    };
    let expected_values = string_table.resolve(*expected);
    assert!(expected_values.contains("trailing_slash"));
    assert!(expected_values.contains("no_trailing_slash"));
    assert!(expected_values.contains("ignore"));
}

#[test]
fn parser_rejects_invalid_redirect_index_html() {
    let mut config = Config::new(PathBuf::from("project"));
    config
        .settings
        .insert(String::from("redirect_index_html"), String::from("yes"));

    let mut string_table = StringTable::new();
    let error =
        parse_html_site_config(&config, &mut string_table).expect_err("invalid value should fail");
    let diagnostic = error.diagnostic().expect("config error should be typed");
    assert!(matches!(
        diagnostic.payload,
        DiagnosticPayload::InvalidConfig {
            reason: InvalidConfigReason::InvalidProjectSettingValue { .. },
            ..
        }
    ));
}

#[test]
fn parser_uses_precise_location_from_setting_locations() {
    use crate::compiler_frontend::compiler_errors::SourceLocation;

    let mut config = Config::new(PathBuf::from("project"));
    config
        .settings
        .insert(String::from("origin"), String::from("invalid"));

    // Store a precise location for the origin key
    let mut string_table = StringTable::new();
    let precise_location = SourceLocation::new(
        InternedPath::try_from_filesystem_path(
            PathBuf::from("project/config.bst").as_path(),
            &mut string_table,
        )
        .expect("test path should be UTF-8"),
        Default::default(),
        Default::default(),
    );
    config
        .setting_locations
        .insert(String::from("origin"), precise_location.clone());

    let error =
        parse_html_site_config(&config, &mut string_table).expect_err("invalid origin should fail");

    // Verify the error uses the precise location from setting_locations
    let diagnostic = error.diagnostic().expect("config error should be typed");
    assert_eq!(diagnostic.primary_location.scope, precise_location.scope);
}

#[test]
fn parser_falls_back_to_file_location_when_key_not_in_setting_locations() {
    let mut config = Config::new(PathBuf::from("project"));
    config
        .settings
        .insert(String::from("origin"), String::from("invalid"));

    // Don't add the key to setting_locations

    let mut string_table = StringTable::new();
    let error =
        parse_html_site_config(&config, &mut string_table).expect_err("invalid origin should fail");

    // Verify the error falls back to file-level location
    let diagnostic = error.diagnostic().expect("config error should be typed");
    assert_eq!(
        diagnostic.primary_location.scope.to_path_buf(&string_table),
        PathBuf::from("project/config.bst")
    );
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
