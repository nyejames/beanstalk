//! Shared HTML page-routing policy parsing and defaults.
//!
//! WHAT: parses routing-related `#config.bst` settings into typed values.
//! WHY: keeping one parser avoids drift between builder validation and dev-server runtime behavior.

use crate::compiler_frontend::compiler_errors::{CompilerError, ErrorType};
use crate::projects::settings::{CONFIG_FILE_NAME, Config};

/// Canonical page URL style used for directory-backed HTML routes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageUrlStyle {
    /// `/about/` is canonical and `/about` redirects.
    TrailingSlash,
    /// `/about` is canonical and `/about/` redirects.
    NoTrailingSlash,
    /// Both `/about` and `/about/` are accepted without slash redirects.
    Ignore,
}

/// Effective HTML routing settings used by builders and the dev server.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HtmlRoutingConfig {
    pub page_url_style: PageUrlStyle,
    pub redirect_index_html: bool,
}

impl Default for HtmlRoutingConfig {
    fn default() -> Self {
        Self {
            page_url_style: PageUrlStyle::TrailingSlash,
            redirect_index_html: true,
        }
    }
}

/// Parse and validate routing-related HTML config keys from the project config map.
///
/// WHAT: resolves defaults plus optional overrides from `#config.bst`.
/// WHY: HTML routing policy must be explicit and strict so all runtime/build tooling stays aligned.
pub fn parse_html_routing_config(config: &Config) -> Result<HtmlRoutingConfig, CompilerError> {
    let page_url_style = parse_page_url_style(config)?;
    let redirect_index_html = parse_redirect_index_html(config)?;

    Ok(HtmlRoutingConfig {
        page_url_style,
        redirect_index_html,
    })
}

fn parse_page_url_style(config: &Config) -> Result<PageUrlStyle, CompilerError> {
    let Some(raw_value) = config.settings.get("page_url_style") else {
        return Ok(PageUrlStyle::TrailingSlash);
    };

    match raw_value.as_str() {
        "trailing_slash" => Ok(PageUrlStyle::TrailingSlash),
        "no_trailing_slash" => Ok(PageUrlStyle::NoTrailingSlash),
        "ignore" => Ok(PageUrlStyle::Ignore),
        _ => Err(config_error(
            config,
            format!(
                "Invalid '#page_url_style' value '{raw_value}'. Allowed values: \"trailing_slash\", \"no_trailing_slash\", \"ignore\"."
            ),
        )),
    }
}

fn parse_redirect_index_html(config: &Config) -> Result<bool, CompilerError> {
    let Some(raw_value) = config.settings.get("redirect_index_html") else {
        return Ok(true);
    };

    match raw_value.as_str() {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err(config_error(
            config,
            format!(
                "Invalid '#redirect_index_html' value '{raw_value}'. Allowed values: true or false."
            ),
        )),
    }
}

fn config_error(config: &Config, message: String) -> CompilerError {
    let config_path = config.entry_dir.join(CONFIG_FILE_NAME);
    CompilerError::file_error(&config_path, message).with_error_type(ErrorType::Config)
}

#[cfg(test)]
#[path = "tests/routing_tests.rs"]
mod tests;
