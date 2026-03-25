//! Frontend style-directive registry used by tokenization and template parsing.
//!
//! WHAT:
//! - Defines the directive contract (`StyleDirectiveSpec`) shared by core language directives
//!   and build-system-provided directives.
//! - Builds a merged registry where compiler core directives are always present.
//! - Supports strict lookups used by tokenizer/AST to reject unknown `$directive` names.
//!
//! WHY:
//! - The frontend must know the directive set before backend lowering.
//! - Build systems can provide full non-core directive behavior without changing parser code.
//! - A single merged registry avoids tokenizer/AST drift and keeps diagnostics consistent.

use crate::compiler_frontend::ast::templates::create_template_node::Template;
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

/// Supported optional single-argument types for build-system-provided directives.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StyleDirectiveArgumentType {
    String,
    Template,
    Number,
    Bool,
}

/// Parsed value for a build-system-provided optional style argument.
#[derive(Clone, Debug)]
pub enum StyleDirectiveArgumentValue {
    String(String),
    Template(Template),
    Number(f64),
    Bool(bool),
}

/// Template-style toggles that a provided directive can apply when used.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ProvidedStyleEffects {
    /// Optional semantic style ID used for formatter precedence/debugging.
    pub style_id: Option<&'static str>,
    /// Optional body-whitespace policy override.
    pub body_whitespace_policy: Option<BodyWhitespacePolicy>,
    /// Optional toggle for parsing `[...]` as literal body text.
    pub suppress_child_templates: Option<bool>,
    /// Optional toggle that opts this template out of parent `$children(..)` wrappers.
    pub skip_parent_child_wrappers: Option<bool>,
}

/// Formatter factory used by build-system-provided directives.
///
/// Returns:
/// - `Ok(Some(formatter))` to set/replace the active formatter.
/// - `Ok(None)` to leave formatter untouched (or clear if the caller chooses).
/// - `Err(message)` for user-facing directive argument/configuration errors.
pub type ProvidedFormatterFactory =
    fn(Option<&StyleDirectiveArgumentValue>) -> Result<Option<Formatter>, String>;

/// Full behavior contract for one build-system-provided style directive.
#[derive(Clone, Debug)]
pub struct ProvidedStyleDirectiveSpec {
    /// Optional single argument contract for `$name(...)`.
    pub argument_type: Option<StyleDirectiveArgumentType>,
    /// Template style-state toggles applied when this directive is parsed.
    pub style_effects: ProvidedStyleEffects,
    /// Optional formatter factory invoked with the parsed optional argument.
    pub formatter_factory: Option<ProvidedFormatterFactory>,
}

impl ProvidedStyleDirectiveSpec {
    pub fn new(
        argument_type: Option<StyleDirectiveArgumentType>,
        style_effects: ProvidedStyleEffects,
        formatter_factory: Option<ProvidedFormatterFactory>,
    ) -> Self {
        Self {
            argument_type,
            style_effects,
            formatter_factory,
        }
    }

    pub fn no_op() -> Self {
        Self {
            argument_type: None,
            style_effects: ProvidedStyleEffects::default(),
            formatter_factory: None,
        }
    }
}

/// Directive class contract.
#[derive(Clone, Debug)]
pub enum StyleDirectiveKind {
    Core(CoreStyleDirectiveKind),
    Provided(ProvidedStyleDirectiveSpec),
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
    /// Build-system API for registering a fully provided style directive.
    pub fn provided(
        name: impl Into<String>,
        body_mode: TemplateBodyMode,
        provided: ProvidedStyleDirectiveSpec,
    ) -> Self {
        Self {
            name: name.into(),
            body_mode,
            kind: StyleDirectiveKind::Provided(provided),
        }
    }

    /// Build-system API for explicit no-op directive registration.
    pub fn provided_no_op(name: impl Into<String>, body_mode: TemplateBodyMode) -> Self {
        Self::provided(name, body_mode, ProvidedStyleDirectiveSpec::no_op())
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
    /// Compiler core directives that are always available.
    ///
    /// Non-core directives must be provided by the active build system.
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
            ],
        }
    }

    /// Merge build-system directives over compiler core directives.
    ///
    /// Rules:
    /// - Core directive names cannot be overridden by build systems.
    /// - Build systems may only provide `StyleDirectiveKind::Provided`.
    /// - For non-core names, later entries replace earlier entries by exact name.
    pub fn merged(builder_specs: &[StyleDirectiveSpec]) -> Result<Self, CompilerError> {
        let mut merged = Self::built_ins().ordered;

        for builder_spec in builder_specs {
            if builder_spec.is_core() {
                return Err(CompilerError::compiler_error(format!(
                    "Build-system style directive '${}' cannot be registered as a core directive.",
                    builder_spec.name
                )));
            }

            if let Some(core_conflict) = merged
                .iter()
                .find(|spec| spec.name == builder_spec.name && spec.is_core())
            {
                return Err(CompilerError::compiler_error(format!(
                    "Build-system style directive '${}' cannot override compiler core directive '${}'.",
                    builder_spec.name, core_conflict.name
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
