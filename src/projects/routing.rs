//! Shared HTML page-routing policy parsing and defaults.
//!
//! WHAT: parses routing-related `#config.bst` settings into typed values.
//! WHY: keeping one parser avoids drift between builder validation and dev-server runtime behavior.

use crate::compiler_frontend::compiler_errors::{CompilerError, ErrorLocation, ErrorType};
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

/// Effective HTML site configuration used by builders and the dev server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HtmlSiteConfig {
    pub origin: String,
    pub page_url_style: PageUrlStyle,
    pub redirect_index_html: bool,
}

impl Default for HtmlSiteConfig {
    fn default() -> Self {
        Self {
            origin: String::from("/"),
            page_url_style: PageUrlStyle::TrailingSlash,
            redirect_index_html: true,
        }
    }
}

/// Parse and validate HTML site config keys from the project config map.
///
/// WHAT: resolves defaults plus optional overrides from `#config.bst`.
/// WHY: site configuration must be explicit and strict so all runtime/build tooling stays aligned.
pub fn parse_html_site_config(config: &Config) -> Result<HtmlSiteConfig, CompilerError> {
    let origin = parse_origin(config)?;
    let page_url_style = parse_page_url_style(config)?;
    let redirect_index_html = parse_redirect_index_html(config)?;

    Ok(HtmlSiteConfig {
        origin,
        page_url_style,
        redirect_index_html,
    })
}

fn parse_origin(config: &Config) -> Result<String, CompilerError> {
    let Some(raw_value) = config.settings.get("origin") else {
        return Ok(String::from("/"));
    };

    validate_origin(config, raw_value)?;

    Ok(raw_value.to_owned())
}

fn validate_origin(config: &Config, origin: &str) -> Result<(), CompilerError> {
    if origin.is_empty() {
        return Err(config_error(
            config,
            "origin",
            String::from("'#origin' cannot be empty."),
        ));
    }

    if !origin.starts_with('/') {
        return Err(config_error(
            config,
            "origin",
            format!("'#origin' must start with '/'. Found: '{origin}'"),
        ));
    }

    if origin.len() > 1 && origin.ends_with('/') {
        return Err(config_error(
            config,
            "origin",
            format!("'#origin' must not end with '/' unless it is exactly '/'. Found: '{origin}'"),
        ));
    }

    if origin.contains('?') || origin.contains('#') {
        return Err(config_error(
            config,
            "origin",
            format!(
                "'#origin' must not contain query (?) or fragment (#) characters. Found: '{origin}'"
            ),
        ));
    }

    if origin.contains('\\') {
        return Err(config_error(
            config,
            "origin",
            format!("'#origin' must not contain backslashes. Found: '{origin}'"),
        ));
    }

    Ok(())
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
            "page_url_style",
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
            "redirect_index_html",
            format!(
                "Invalid '#redirect_index_html' value '{raw_value}'. Allowed values: true or false."
            ),
        )),
    }
}

/// WHAT: creates a config error with precise location from setting_locations if available.
/// WHY: precise error locations help users quickly identify and fix config issues.
fn config_error(config: &Config, key: &str, message: String) -> CompilerError {
    let location = config
        .setting_locations
        .get(key)
        .cloned()
        .unwrap_or_else(|| {
            // Fall back to file-level location if key location not found
            let config_path = config.entry_dir.join(CONFIG_FILE_NAME);
            ErrorLocation::new(config_path, Default::default(), Default::default())
        });

    let mut error = CompilerError::new(message, location, ErrorType::Config);
    // Add actionable suggestion based on the key
    let suggestion = match key {
        "page_url_style" => "Use 'trailing_slash', 'no_trailing_slash', or 'ignore'",
        "redirect_index_html" => "Use 'true' to redirect or 'false' to keep index.html URLs",
        _ => "Check the config documentation for valid values",
    };
    error.metadata.insert(
        crate::compiler_frontend::compiler_errors::ErrorMetaDataKey::PrimarySuggestion,
        suggestion.to_string(),
    );
    error
}

// --- Public Path Helpers ---

/// Prefixes a site-local path with the given origin.
pub fn prefix_origin(origin: &str, site_path: &str) -> String {
    if origin == "/" {
        return site_path.to_owned();
    }

    if site_path == "/" {
        return format!("{origin}/");
    }

    format!("{origin}{site_path}")
}

/// Strips the origin prefix from a public path, returning the site-local path.
pub fn strip_origin_prefix(public_path: &str, origin: &str) -> Option<String> {
    if origin == "/" {
        return Some(public_path.to_owned());
    }

    // Origin is like "/beanstalk"
    // Public path must start with "/beanstalk"
    if !public_path.starts_with(origin) {
        return None;
    }

    let stripped = &public_path[origin.len()..];

    // If public_path was "/beanstalk", stripped is ""
    // If public_path was "/beanstalk/", stripped is "/"
    // If public_path was "/beanstalk/about", stripped is "/about"
    if stripped.is_empty() {
        return Some(String::from("/"));
    }

    if stripped.starts_with('/') {
        Some(stripped.to_owned())
    } else {
        // This handles cases like "/beanstalkish" when origin is "/beanstalk"
        None
    }
}

/// Returns the canonical public URL for the site root.
pub fn origin_root_url(origin: &str) -> String {
    if origin == "/" {
        String::from("/")
    } else {
        format!("{origin}/")
    }
}

/// Converts a site-local page URL to a public URL including the origin.
#[allow(dead_code)] // Prepared for future path-coercion use
pub fn public_page_url(site_local_page_url: &str, origin: &str) -> String {
    prefix_origin(origin, site_local_page_url)
}

/// Converts a site-local asset path to a public URL including the origin.
#[allow(dead_code)] // Prepared for future path-coercion use
pub fn public_asset_url(site_local_asset_path: &str, origin: &str) -> String {
    prefix_origin(origin, site_local_asset_path)
}

#[cfg(test)]
#[path = "tests/routing_tests.rs"]
mod tests;
