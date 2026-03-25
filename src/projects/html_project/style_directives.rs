//! HTML project style-directive registration.
//!
//! WHAT:
//! - Declares all non-core style directives used by the HTML build system.
//! - Provides full directive behavior through formatter factories and argument contracts.
//!
//! WHY:
//! - Non-core directive behavior should be owned by build systems, not hardcoded in frontend core.

use crate::compiler_frontend::ast::templates::styles::css::css_formatter_factory;
use crate::compiler_frontend::ast::templates::styles::escape_html::escape_html_formatter_factory;
use crate::compiler_frontend::ast::templates::styles::html::html_formatter_factory;
use crate::compiler_frontend::ast::templates::styles::markdown::markdown_formatter_factory;
use crate::compiler_frontend::style_directives::{
    ProvidedStyleDirectiveSpec, ProvidedStyleEffects, StyleDirectiveArgumentType,
    StyleDirectiveSpec,
};
use crate::compiler_frontend::tokenizer::tokens::TemplateBodyMode;

/// Full non-core style-directive set for the HTML project builder.
pub(crate) fn html_project_style_directives() -> Vec<StyleDirectiveSpec> {
    vec![
        StyleDirectiveSpec::provided(
            "markdown",
            TemplateBodyMode::Normal,
            ProvidedStyleDirectiveSpec::new(
                None,
                ProvidedStyleEffects {
                    style_id: Some("markdown"),
                    ..ProvidedStyleEffects::default()
                },
                Some(markdown_formatter_factory),
            ),
        ),
        StyleDirectiveSpec::provided(
            "html",
            TemplateBodyMode::Normal,
            ProvidedStyleDirectiveSpec::new(
                None,
                ProvidedStyleEffects {
                    style_id: Some("html"),
                    ..ProvidedStyleEffects::default()
                },
                Some(html_formatter_factory),
            ),
        ),
        StyleDirectiveSpec::provided(
            "css",
            TemplateBodyMode::Balanced,
            ProvidedStyleDirectiveSpec::new(
                Some(StyleDirectiveArgumentType::String),
                ProvidedStyleEffects {
                    style_id: Some("css"),
                    ..ProvidedStyleEffects::default()
                },
                Some(css_formatter_factory),
            ),
        ),
        StyleDirectiveSpec::provided(
            "escape_html",
            TemplateBodyMode::Normal,
            ProvidedStyleDirectiveSpec::new(
                None,
                ProvidedStyleEffects {
                    style_id: Some("escape_html"),
                    ..ProvidedStyleEffects::default()
                },
                Some(escape_html_formatter_factory),
            ),
        ),
    ]
}
