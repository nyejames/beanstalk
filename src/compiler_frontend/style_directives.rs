//! Frontend style-directive registry used by tokenization and template parsing.
//!
//! WHAT:
//! - Defines the directive contract (`StyleDirectiveSpec`) shared by builders and built-ins.
//! - Builds a merged registry where compiler built-ins are always present.
//! - Supports strict lookups used by tokenizer/AST to reject unknown `$directive` names.
//!
//! WHY:
//! - The frontend must know the directive set before backend lowering.
//! - Build systems can extend or override built-ins without changing parser code.
//! - A single merged registry avoids tokenizer/AST drift and keeps diagnostics consistent.

use crate::compiler_frontend::tokenizer::tokens::TemplateBodyMode;
use std::fmt::Write as _;

/// Origin of a registered style directive.
///
/// Built-ins use compiler-owned semantics. Builder directives are explicit no-op
/// directives by default unless they intentionally override behavior metadata.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StyleDirectiveSource {
    BuiltIn,
    Builder,
}

/// Built-in directive behavior handled directly by compiler-owned template parsing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BuiltInStyleDirectiveKind {
    Markdown,
    Children,
    Fresh,
    Slot,
    Insert,
    Note,
    Todo,
    Doc,
    Code,
    Css,
    Html,
    Raw,
    EscapeHtml,
}

/// Argument policy used by explicit no-op directives.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NoOpDirectiveArgumentPolicy {
    /// Rejects parenthesized arguments (`$name(...)`) outright.
    Reject,
    /// Consumes optional parenthesized arguments but does not interpret them.
    ConsumeOptionalParenthesized,
}

/// Parser behavior contract for one style directive.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StyleDirectiveBehavior {
    BuiltIn(BuiltInStyleDirectiveKind),
    ExplicitNoOp {
        argument_policy: NoOpDirectiveArgumentPolicy,
    },
}

/// Frontend registration contract for one style directive.
///
/// `body_mode` controls template-body tokenization after the directive appears in
/// the template head. `behavior` controls AST-level directive handling.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StyleDirectiveSpec {
    pub name: String,
    pub body_mode: TemplateBodyMode,
    pub source: StyleDirectiveSource,
    pub behavior: StyleDirectiveBehavior,
}

impl StyleDirectiveSpec {
    /// Build-system API for adding explicit no-op frontend directives.
    ///
    /// This default rejects parenthesized arguments so no-op behavior is explicit.
    pub fn explicit_noop(name: impl Into<String>, body_mode: TemplateBodyMode) -> Self {
        Self {
            name: name.into(),
            body_mode,
            source: StyleDirectiveSource::Builder,
            behavior: StyleDirectiveBehavior::ExplicitNoOp {
                argument_policy: NoOpDirectiveArgumentPolicy::Reject,
            },
        }
    }

    /// Build-system API for no-op directives that need a custom argument policy.
    pub fn explicit_noop_with_argument_policy(
        name: impl Into<String>,
        body_mode: TemplateBodyMode,
        argument_policy: NoOpDirectiveArgumentPolicy,
    ) -> Self {
        Self {
            name: name.into(),
            body_mode,
            source: StyleDirectiveSource::Builder,
            behavior: StyleDirectiveBehavior::ExplicitNoOp { argument_policy },
        }
    }

    /// Internal helper for compiler-owned built-in directives.
    pub(crate) fn built_in(
        name: &str,
        body_mode: TemplateBodyMode,
        kind: BuiltInStyleDirectiveKind,
    ) -> Self {
        Self {
            name: name.to_string(),
            body_mode,
            source: StyleDirectiveSource::BuiltIn,
            behavior: StyleDirectiveBehavior::BuiltIn(kind),
        }
    }
}

/// Ordered registry used by tokenizer and AST template parsing.
///
/// The order is stable and intentionally used for diagnostics so unsupported-directive
/// errors show a deterministic directive list.
#[derive(Clone, Debug, Default)]
pub struct StyleDirectiveRegistry {
    ordered: Vec<StyleDirectiveSpec>,
}

impl StyleDirectiveRegistry {
    /// Compiler built-ins that are always available unless a builder overrides a name.
    ///
    /// This list defines the default frontend directive surface for the HTML builder
    /// and test helpers that do not provide additional project directives.
    pub fn built_ins() -> Self {
        Self {
            ordered: vec![
                StyleDirectiveSpec::built_in(
                    "markdown",
                    TemplateBodyMode::Normal,
                    BuiltInStyleDirectiveKind::Markdown,
                ),
                StyleDirectiveSpec::built_in(
                    "children",
                    TemplateBodyMode::Normal,
                    BuiltInStyleDirectiveKind::Children,
                ),
                StyleDirectiveSpec::built_in(
                    "fresh",
                    TemplateBodyMode::Normal,
                    BuiltInStyleDirectiveKind::Fresh,
                ),
                StyleDirectiveSpec::built_in(
                    "slot",
                    TemplateBodyMode::Normal,
                    BuiltInStyleDirectiveKind::Slot,
                ),
                StyleDirectiveSpec::built_in(
                    "insert",
                    TemplateBodyMode::Normal,
                    BuiltInStyleDirectiveKind::Insert,
                ),
                StyleDirectiveSpec::built_in(
                    "note",
                    TemplateBodyMode::DiscardBalanced,
                    BuiltInStyleDirectiveKind::Note,
                ),
                StyleDirectiveSpec::built_in(
                    "todo",
                    TemplateBodyMode::DiscardBalanced,
                    BuiltInStyleDirectiveKind::Todo,
                ),
                StyleDirectiveSpec::built_in(
                    "doc",
                    TemplateBodyMode::Normal,
                    BuiltInStyleDirectiveKind::Doc,
                ),
                StyleDirectiveSpec::built_in(
                    "code",
                    TemplateBodyMode::Balanced,
                    BuiltInStyleDirectiveKind::Code,
                ),
                StyleDirectiveSpec::built_in(
                    "css",
                    TemplateBodyMode::Balanced,
                    BuiltInStyleDirectiveKind::Css,
                ),
                // `$html` uses the same normal template-body parsing as `$markdown`.
                StyleDirectiveSpec::built_in(
                    "html",
                    TemplateBodyMode::Normal,
                    BuiltInStyleDirectiveKind::Html,
                ),
                StyleDirectiveSpec::built_in(
                    "raw",
                    TemplateBodyMode::Normal,
                    BuiltInStyleDirectiveKind::Raw,
                ),
                StyleDirectiveSpec::built_in(
                    "escape_html",
                    TemplateBodyMode::Normal,
                    BuiltInStyleDirectiveKind::EscapeHtml,
                ),
            ],
        }
    }

    /// Merge builder directives over compiler built-ins.
    ///
    /// Precedence is "built-ins first, then exact-name builder override" because:
    /// - users still get a complete default directive set,
    /// - builders can replace specific directive names without forking frontend code.
    pub fn merged(builder_specs: &[StyleDirectiveSpec]) -> Self {
        let mut merged = Self::built_ins().ordered;

        for builder_spec in builder_specs {
            if let Some(existing) = merged
                .iter_mut()
                .find(|spec| spec.name == builder_spec.name)
            {
                *existing = StyleDirectiveSpec {
                    name: builder_spec.name.to_owned(),
                    body_mode: builder_spec.body_mode,
                    source: StyleDirectiveSource::Builder,
                    behavior: builder_spec.behavior,
                };
                continue;
            }

            merged.push(StyleDirectiveSpec {
                name: builder_spec.name.to_owned(),
                body_mode: builder_spec.body_mode,
                source: StyleDirectiveSource::Builder,
                behavior: builder_spec.behavior,
            });
        }

        Self { ordered: merged }
    }

    pub fn find(&self, name: &str) -> Option<&StyleDirectiveSpec> {
        self.ordered.iter().find(|spec| spec.name == name)
    }

    /// Resolve only the template-body tokenization mode for a directive.
    pub fn body_mode_for(&self, name: &str) -> Option<TemplateBodyMode> {
        self.find(name).map(|spec| spec.body_mode)
    }

    /// Render a deterministic directive list for user-facing diagnostics.
    pub fn supported_directives_for_diagnostic(&self) -> String {
        let mut rendered = String::new();
        for (index, spec) in self.ordered.iter().enumerate() {
            if index > 0 {
                rendered.push_str(", ");
            }
            let _ = write!(rendered, "'${}'", spec.name);
        }
        rendered
    }
}

#[cfg(test)]
#[path = "tests/style_directives_tests.rs"]
mod style_directives_tests;
