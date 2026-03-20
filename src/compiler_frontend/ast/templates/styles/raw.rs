//! Built-in `$raw` template style support.
//!
//! WHAT:
//! - Disables default template-body whitespace normalization for the current template.
//!
//! WHY:
//! - The compiler now normalizes template body whitespace by default.
//! - `$raw` provides an explicit opt-out for templates that must preserve authored bytes.

use crate::compiler_frontend::ast::templates::create_template_node::Template;
use crate::compiler_frontend::ast::templates::template::BodyWhitespacePolicy;

pub(crate) fn configure_raw_style(template: &mut Template) {
    template.apply_style_updates(|style| {
        style.body_whitespace_policy = BodyWhitespacePolicy::StyleDirectiveControlled;
    });
}
