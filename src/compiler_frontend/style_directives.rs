//! Frontend style-directive registry used by tokenization and template parsing.
//!
//! WHAT:
//! - Defines the directive contract (`StyleDirectiveSpec`) shared by core language directives
//!   and handler-based formatter directives.
//! - Builds a merged registry where compiler-built directives are always present.
//! - Supports strict lookups used by tokenizer/AST to reject unknown `$directive` names.
//!
//! WHY:
//! - The frontend must know the directive set before backend lowering.
//! - Project builders can register project-specific directives without changing parser code.
//! - A single merged registry avoids tokenizer/AST drift and keeps diagnostics consistent.
//!
//! Directive ownership policy:
//! - Frontend built-ins define language/template semantics and generic formatter directives
//!   such as `$markdown`.
//! - Project builders may only register additional project-owned directives such as the HTML
//!   project's `$html`, `$css`, and `$escape_html`.
//! - The frontend always executes directive handlers during parsing/folding, regardless of
//!   whether the directive itself is frontend-owned or project-owned.

use crate::compiler_frontend::ast::templates::create_template_node::Template;
use crate::compiler_frontend::ast::templates::styles::markdown::markdown_formatter_factory;
use crate::compiler_frontend::ast::templates::template::{BodyWhitespacePolicy, Formatter};
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::tokenizer::tokens::TemplateBodyMode;
use std::fmt::Write as _;

/// Core language directive behavior handled directly by compiler-owned template parsing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CoreStyleDirectiveKind {
    Children,
    Fresh,
    Slot,
    Insert,
    Note,
    Todo,
    Doc,
    Code,
    Raw,
}

/// Supported optional single-argument types for handler-based directives.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StyleDirectiveArgumentType {
    String,
    Template,
    Number,
    Bool,
}

/// Parsed value for an optional handler-based style argument.
#[derive(Clone, Debug)]
pub enum StyleDirectiveArgumentValue {
    String(String),
    Template(Template),
    Number(f64),
    Bool(bool),
}

/// Template-style toggles that a handler-based directive can apply when used.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StyleDirectiveEffects {
    /// Optional semantic style label applied to `Template.style.id`.
    ///
    /// This is intentionally distinct from `Formatter.id`:
    /// - `style_id` tags parsed template semantics.
    /// - `Formatter.id` identifies the concrete formatter implementation.
    pub style_id: Option<&'static str>,
    /// Optional body-whitespace policy override.
    pub body_whitespace_policy: Option<BodyWhitespacePolicy>,
    /// Optional toggle for parsing `[...]` as literal body text.
    pub suppress_child_templates: Option<bool>,
    /// Optional toggle that opts this template out of parent `$children(..)` wrappers.
    pub skip_parent_child_wrappers: Option<bool>,
}

/// Formatter factory used by handler-based directives.
///
/// Returns:
/// - `Ok(Some(formatter))` to set/replace the active formatter.
/// - `Ok(None)` to leave formatter untouched (or clear if the caller chooses).
/// - `Err(message)` for user-facing directive argument/configuration errors.
pub type StyleDirectiveFormatterFactory =
    fn(Option<&StyleDirectiveArgumentValue>) -> Result<Option<Formatter>, String>;

/// Full behavior contract for one handler-based style directive.
///
/// WHAT:
/// - Carries optional argument typing, style-state effects, and formatter factory behavior.
///
/// WHY:
/// - The same contract is used for frontend-owned formatter built-ins and project-owned
///   directives, so file location determines ownership while parser dispatch stays generic.
#[derive(Clone, Debug)]
pub struct StyleDirectiveHandlerSpec {
    /// Optional single argument contract for `$name(...)`.
    pub argument_type: Option<StyleDirectiveArgumentType>,
    /// Template style-state toggles applied when this directive is parsed.
    pub effects: StyleDirectiveEffects,
    /// Optional formatter factory invoked with the parsed optional argument.
    pub formatter_factory: Option<StyleDirectiveFormatterFactory>,
}

impl StyleDirectiveHandlerSpec {
    pub fn new(
        argument_type: Option<StyleDirectiveArgumentType>,
        effects: StyleDirectiveEffects,
        formatter_factory: Option<StyleDirectiveFormatterFactory>,
    ) -> Self {
        Self {
            argument_type,
            effects,
            formatter_factory,
        }
    }

    pub fn no_op() -> Self {
        Self {
            argument_type: None,
            effects: StyleDirectiveEffects::default(),
            formatter_factory: None,
        }
    }
}

/// Directive class contract.
#[derive(Clone, Debug)]
pub enum StyleDirectiveKind {
    Core(CoreStyleDirectiveKind),
    Handler(StyleDirectiveHandlerSpec),
}

/// Frontend registration contract for one style directive.
///
/// `body_mode` controls template-body tokenization after the directive appears in the template
/// head. `kind` controls AST-level directive handling.
#[derive(Clone, Debug)]
pub struct StyleDirectiveSpec {
    pub name: String,
    pub body_mode: TemplateBodyMode,
    pub kind: StyleDirectiveKind,
}

impl StyleDirectiveSpec {
    /// Register a handler-based directive.
    pub fn handler(
        name: impl Into<String>,
        body_mode: TemplateBodyMode,
        handler: StyleDirectiveHandlerSpec,
    ) -> Self {
        Self {
            name: name.into(),
            body_mode,
            kind: StyleDirectiveKind::Handler(handler),
        }
    }

    /// Register an explicit no-op handler-based directive.
    pub fn handler_no_op(name: impl Into<String>, body_mode: TemplateBodyMode) -> Self {
        Self::handler(name, body_mode, StyleDirectiveHandlerSpec::no_op())
    }

    /// Internal helper for compiler-owned core directives.
    pub(crate) fn core(
        name: &str,
        body_mode: TemplateBodyMode,
        kind: CoreStyleDirectiveKind,
    ) -> Self {
        Self {
            name: name.to_string(),
            body_mode,
            kind: StyleDirectiveKind::Core(kind),
        }
    }

    fn is_core(&self) -> bool {
        matches!(self.kind, StyleDirectiveKind::Core(_))
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
    /// Frontend-owned directives that are always available.
    pub fn built_ins() -> Self {
        Self {
            ordered: vec![
                StyleDirectiveSpec::core(
                    "children",
                    TemplateBodyMode::Normal,
                    CoreStyleDirectiveKind::Children,
                ),
                StyleDirectiveSpec::core(
                    "fresh",
                    TemplateBodyMode::Normal,
                    CoreStyleDirectiveKind::Fresh,
                ),
                StyleDirectiveSpec::core(
                    "slot",
                    TemplateBodyMode::Normal,
                    CoreStyleDirectiveKind::Slot,
                ),
                StyleDirectiveSpec::core(
                    "insert",
                    TemplateBodyMode::Normal,
                    CoreStyleDirectiveKind::Insert,
                ),
                StyleDirectiveSpec::core(
                    "note",
                    TemplateBodyMode::DiscardBalanced,
                    CoreStyleDirectiveKind::Note,
                ),
                StyleDirectiveSpec::core(
                    "todo",
                    TemplateBodyMode::DiscardBalanced,
                    CoreStyleDirectiveKind::Todo,
                ),
                StyleDirectiveSpec::core(
                    "doc",
                    TemplateBodyMode::Normal,
                    CoreStyleDirectiveKind::Doc,
                ),
                StyleDirectiveSpec::core(
                    "code",
                    TemplateBodyMode::Balanced,
                    CoreStyleDirectiveKind::Code,
                ),
                StyleDirectiveSpec::core(
                    "raw",
                    TemplateBodyMode::Normal,
                    CoreStyleDirectiveKind::Raw,
                ),
                StyleDirectiveSpec::handler(
                    "markdown",
                    TemplateBodyMode::Normal,
                    StyleDirectiveHandlerSpec::new(
                        None,
                        StyleDirectiveEffects {
                            style_id: Some("markdown"),
                            ..StyleDirectiveEffects::default()
                        },
                        Some(markdown_formatter_factory),
                    ),
                ),
            ],
        }
    }

    /// Merge project-builder directives over frontend-owned directives.
    ///
    /// Rules:
    /// - Frontend-owned directive names cannot be overridden by project builders.
    /// - Project builders may only provide `StyleDirectiveKind::Handler`.
    /// - For project-owned names, later entries replace earlier entries by exact name.
    pub fn merged(builder_specs: &[StyleDirectiveSpec]) -> Result<Self, CompilerError> {
        let frontend_owned = Self::built_ins().ordered;
        let mut merged = frontend_owned.clone();

        for builder_spec in builder_specs {
            if builder_spec.is_core() {
                return Err(CompilerError::compiler_error(format!(
                    "Project builder style directive '${}' cannot be registered as a core directive.",
                    builder_spec.name
                )));
            }

            if let Some(frontend_conflict) = frontend_owned
                .iter()
                .find(|spec| spec.name == builder_spec.name)
            {
                return Err(CompilerError::compiler_error(format!(
                    "Project builder style directive '${}' cannot override frontend-owned directive '${}'.",
                    builder_spec.name, frontend_conflict.name
                )));
            }

            if let Some(existing) = merged
                .iter_mut()
                .find(|spec| spec.name == builder_spec.name)
            {
                *existing = builder_spec.clone();
            } else {
                merged.push(builder_spec.clone());
            }
        }

        Ok(Self { ordered: merged })
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
