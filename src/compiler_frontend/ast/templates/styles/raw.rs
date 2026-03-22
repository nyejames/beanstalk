//! Built-in `$raw` template style support.
//!
//! WHAT:
//! - Disables the default template-body whitespace normalization (dedent/trim) for the
//!   current template while keeping all other template features intact.
//! - Child templates, expressions, and composition work the same as the default template.
//!
//! WHY:
//! - The compiler normalizes template body whitespace by default for readability.
//! - `$raw` provides an explicit opt-out for templates where authored whitespace must be
//!   preserved exactly as written. Raw strings inside templates use [`raw string`] syntax.

use crate::compiler_frontend::ast::templates::create_template_node::Template;
use crate::compiler_frontend::ast::templates::template::BodyWhitespacePolicy;

pub(crate) fn configure_raw_style(template: &mut Template) {
    template.apply_style_updates(|style| {
        style.body_whitespace_policy = BodyWhitespacePolicy::StyleDirectiveControlled;
    });
}
