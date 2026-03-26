//! Typed HTML document-shell configuration parsing.
//!
//! WHAT: parses HTML-shell-specific `#config.bst` settings into a strict typed struct.
//! WHY: keeping document policy separate from routing config avoids one oversized parser and
//!      gives the HTML builder a single source of truth for shell defaults.

use crate::compiler_frontend::compiler_errors::{CompilerError, ErrorLocation, ErrorType};
use crate::projects::settings::{CONFIG_FILE_NAME, Config};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HtmlDocumentConfig {
    pub lang: String,
    pub title_prefix: String,
    pub title_postfix: String,
    pub favicon: Option<String>,
    pub inject_charset: bool,
    pub inject_viewport: bool,
    pub inject_color_scheme: bool,
    pub inject_core_css: bool,
    pub body_style: String,
}

impl Default for HtmlDocumentConfig {
    fn default() -> Self {
        Self {
            lang: String::from("en"),
            title_prefix: String::new(),
            title_postfix: String::new(),
            favicon: None,
            inject_charset: true,
            inject_viewport: true,
            inject_color_scheme: true,
            inject_core_css: true,
            body_style: String::new(),
        }
    }
}

pub(crate) fn parse_html_document_config(
    config: &Config,
) -> Result<HtmlDocumentConfig, CompilerError> {
    Ok(HtmlDocumentConfig {
        lang: parse_required_string(config, "html_lang", "en", true)?,
        title_prefix: parse_required_string(config, "html_title_prefix", "", false)?,
        title_postfix: parse_required_string(config, "html_title_postfix", "", false)?,
        favicon: parse_optional_string(config, "html_favicon")?,
        inject_charset: parse_bool(config, "html_inject_charset", true)?,
        inject_viewport: parse_bool(config, "html_inject_viewport", true)?,
        inject_color_scheme: parse_bool(config, "html_inject_color_scheme", true)?,
        inject_core_css: parse_bool(config, "html_inject_core_css", true)?,
        body_style: parse_required_string(config, "html_body_style", "", false)?,
    })
}

fn parse_required_string(
    config: &Config,
    key: &str,
    default: &str,
    reject_empty: bool,
) -> Result<String, CompilerError> {
    let Some(raw_value) = config.settings.get(key) else {
        return Ok(default.to_string());
    };

    if reject_empty && raw_value.is_empty() {
        return Err(config_error(
            config,
            key,
            format!("'#{key}' cannot be empty."),
        ));
    }

    Ok(raw_value.to_owned())
}

fn parse_optional_string(config: &Config, key: &str) -> Result<Option<String>, CompilerError> {
    let Some(raw_value) = config.settings.get(key) else {
        return Ok(None);
    };

    if raw_value.is_empty() {
        return Err(config_error(
            config,
            key,
            format!("'#{key}' cannot be empty when provided."),
        ));
    }

    Ok(Some(raw_value.to_owned()))
}

fn parse_bool(config: &Config, key: &str, default: bool) -> Result<bool, CompilerError> {
    let Some(raw_value) = config.settings.get(key) else {
        return Ok(default);
    };

    match raw_value.as_str() {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err(config_error(
            config,
            key,
            format!("Invalid '#{key}' value '{raw_value}'. Allowed values: true or false."),
        )),
    }
}

fn config_error(config: &Config, key: &str, message: String) -> CompilerError {
    let location = config
        .setting_locations
        .get(key)
        .cloned()
        .unwrap_or_else(|| {
            let config_path = config.entry_dir.join(CONFIG_FILE_NAME);
            ErrorLocation::new(config_path, Default::default(), Default::default())
        });

    let mut error = CompilerError::new(message, location, ErrorType::Config);
    let suggestion = match key {
        "html_lang" => "Use a non-empty language tag such as 'en' or 'en-GB'",
        "html_favicon" => "Use a non-empty favicon path such as '/assets/favicon.ico'",
        "html_inject_charset"
        | "html_inject_viewport"
        | "html_inject_color_scheme"
        | "html_inject_core_css" => "Use 'true' or 'false'",
        _ => "Check the HTML builder config documentation for valid values",
    };
    error.metadata.insert(
        crate::compiler_frontend::compiler_errors::ErrorMetaDataKey::PrimarySuggestion,
        suggestion.to_string(),
    );
    error
}

#[cfg(test)]
#[path = "tests/document_config_tests.rs"]
mod tests;
