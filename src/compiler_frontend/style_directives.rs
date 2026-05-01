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

use crate::compiler_frontend::ast::templates::styles::markdown::markdown_formatter_factory;
use crate::compiler_frontend::ast::templates::template::{BodyWhitespacePolicy, Formatter};
use crate::compiler_frontend::ast::templates::template_types::Template;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::tokenizer::tokens::TemplateBodyMode;
use std::fmt::Write as _;
use std::ops::{BitAnd, BitOr, BitOrAssign};

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
    Raw,
}

/// Template-head compatibility tags for directives and other meaningful head items.
///
/// WHAT:
/// - Encodes compatibility constraints as cheap bit tags in parse-time head state.
///
/// WHY:
/// - Keeps compatibility policy in directive specs instead of hardcoded parser branches.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct TemplateHeadTag(u64);

impl TemplateHeadTag {
    pub const MEANINGFUL_ITEM: Self = Self(1 << 0);
    pub const SLOT_DIRECTIVE: Self = Self(1 << 1);
    pub const INSERT_DIRECTIVE: Self = Self(1 << 2);
    pub const COMMENT_DIRECTIVE: Self = Self(1 << 3);
    pub const FORMATTER_DIRECTIVE: Self = Self(1 << 4);
    pub const CHILDREN_DIRECTIVE: Self = Self(1 << 5);
    pub const FRESH_DIRECTIVE: Self = Self(1 << 6);
    pub const RAW_DIRECTIVE: Self = Self(1 << 7);
    pub const CODE_DIRECTIVE: Self = Self(1 << 8);

    pub const fn empty() -> Self {
        Self(0)
    }

    pub const fn intersects(self, other: Self) -> bool {
        (self.0 & other.0) != 0
    }
}

impl BitOr for TemplateHeadTag {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        Self(self.0 | rhs.0)
    }
}

impl BitOrAssign for TemplateHeadTag {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

impl BitAnd for TemplateHeadTag {
    type Output = Self;

    fn bitand(self, rhs: Self) -> Self::Output {
        Self(self.0 & rhs.0)
    }
}

/// Data-driven template-head compatibility rules attached to each directive spec.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TemplateHeadCompatibility {
    pub presence_tags: TemplateHeadTag,
    pub required_absent_tags: TemplateHeadTag,
    pub blocks_future_tags: TemplateHeadTag,
}

impl TemplateHeadCompatibility {
    pub fn fully_compatible_meaningful() -> Self {
        Self {
            presence_tags: TemplateHeadTag::MEANINGFUL_ITEM,
            required_absent_tags: TemplateHeadTag::empty(),
            blocks_future_tags: TemplateHeadTag::empty(),
        }
    }

    pub fn exclusive_meaningful() -> Self {
        Self {
            presence_tags: TemplateHeadTag::MEANINGFUL_ITEM,
            required_absent_tags: TemplateHeadTag::MEANINGFUL_ITEM,
            blocks_future_tags: TemplateHeadTag::MEANINGFUL_ITEM,
        }
    }

    pub fn blocks_same(tag: TemplateHeadTag) -> Self {
        Self {
            presence_tags: TemplateHeadTag::MEANINGFUL_ITEM | tag,
            required_absent_tags: TemplateHeadTag::empty(),
            blocks_future_tags: tag,
        }
    }
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
    Template(Box<Template>),
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
pub type FormatterFactory = fn(Option<&StyleDirectiveArgumentValue>) -> Result<Formatter, String>;

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
    pub formatter_factory: Option<FormatterFactory>,
}

impl StyleDirectiveHandlerSpec {
    pub fn new(
        argument_type: Option<StyleDirectiveArgumentType>,
        effects: StyleDirectiveEffects,
        formatter_factory: Option<FormatterFactory>,
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
/// head. `head_compatibility` controls parse-time compatibility against other meaningful head
/// items. `kind` controls AST-level directive handling.
#[derive(Clone, Debug)]
pub struct StyleDirectiveSpec {
    pub name: String,
    pub body_mode: TemplateBodyMode,
    pub head_compatibility: TemplateHeadCompatibility,
    pub kind: StyleDirectiveKind,
}

impl StyleDirectiveSpec {
    /// Register a handler-based directive.
    pub fn handler(
        name: impl Into<String>,
        body_mode: TemplateBodyMode,
        head_compatibility: TemplateHeadCompatibility,
        handler: StyleDirectiveHandlerSpec,
    ) -> Self {
        Self {
            name: name.into(),
            body_mode,
            head_compatibility,
            kind: StyleDirectiveKind::Handler(handler),
        }
    }

    /// Register an explicit no-op handler-based directive.
    pub fn handler_no_op(name: impl Into<String>, body_mode: TemplateBodyMode) -> Self {
        Self::handler(
            name,
            body_mode,
            TemplateHeadCompatibility::fully_compatible_meaningful(),
            StyleDirectiveHandlerSpec::no_op(),
        )
    }

    /// Internal helper for compiler-owned core directives.
    pub(crate) fn core(
        name: &str,
        body_mode: TemplateBodyMode,
        head_compatibility: TemplateHeadCompatibility,
        kind: CoreStyleDirectiveKind,
    ) -> Self {
        Self {
            name: name.to_string(),
            body_mode,
            head_compatibility,
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
                    TemplateHeadCompatibility {
                        presence_tags: TemplateHeadTag::MEANINGFUL_ITEM
                            | TemplateHeadTag::CHILDREN_DIRECTIVE,
                        required_absent_tags: TemplateHeadTag::empty(),
                        blocks_future_tags: TemplateHeadTag::empty(),
                    },
                    CoreStyleDirectiveKind::Children,
                ),
                StyleDirectiveSpec::core(
                    "fresh",
                    TemplateBodyMode::Normal,
                    TemplateHeadCompatibility {
                        presence_tags: TemplateHeadTag::MEANINGFUL_ITEM
                            | TemplateHeadTag::FRESH_DIRECTIVE,
                        required_absent_tags: TemplateHeadTag::empty(),
                        blocks_future_tags: TemplateHeadTag::empty(),
                    },
                    CoreStyleDirectiveKind::Fresh,
                ),
                StyleDirectiveSpec::core(
                    "slot",
                    TemplateBodyMode::Normal,
                    TemplateHeadCompatibility {
                        presence_tags: TemplateHeadTag::MEANINGFUL_ITEM
                            | TemplateHeadTag::SLOT_DIRECTIVE,
                        required_absent_tags: TemplateHeadTag::MEANINGFUL_ITEM,
                        blocks_future_tags: TemplateHeadTag::MEANINGFUL_ITEM,
                    },
                    CoreStyleDirectiveKind::Slot,
                ),
                StyleDirectiveSpec::core(
                    "insert",
                    TemplateBodyMode::Normal,
                    TemplateHeadCompatibility::blocks_same(TemplateHeadTag::INSERT_DIRECTIVE),
                    CoreStyleDirectiveKind::Insert,
                ),
                StyleDirectiveSpec::core(
                    "note",
                    TemplateBodyMode::DiscardBalanced,
                    TemplateHeadCompatibility {
                        presence_tags: TemplateHeadTag::MEANINGFUL_ITEM
                            | TemplateHeadTag::COMMENT_DIRECTIVE,
                        required_absent_tags: TemplateHeadTag::MEANINGFUL_ITEM,
                        blocks_future_tags: TemplateHeadTag::MEANINGFUL_ITEM,
                    },
                    CoreStyleDirectiveKind::Note,
                ),
                StyleDirectiveSpec::core(
                    "todo",
                    TemplateBodyMode::DiscardBalanced,
                    TemplateHeadCompatibility {
                        presence_tags: TemplateHeadTag::MEANINGFUL_ITEM
                            | TemplateHeadTag::COMMENT_DIRECTIVE,
                        required_absent_tags: TemplateHeadTag::MEANINGFUL_ITEM,
                        blocks_future_tags: TemplateHeadTag::MEANINGFUL_ITEM,
                    },
                    CoreStyleDirectiveKind::Todo,
                ),
                StyleDirectiveSpec::core(
                    "doc",
                    TemplateBodyMode::Normal,
                    TemplateHeadCompatibility {
                        presence_tags: TemplateHeadTag::MEANINGFUL_ITEM
                            | TemplateHeadTag::COMMENT_DIRECTIVE,
                        required_absent_tags: TemplateHeadTag::MEANINGFUL_ITEM,
                        blocks_future_tags: TemplateHeadTag::MEANINGFUL_ITEM,
                    },
                    CoreStyleDirectiveKind::Doc,
                ),
                StyleDirectiveSpec::core(
                    "raw",
                    TemplateBodyMode::Normal,
                    TemplateHeadCompatibility {
                        presence_tags: TemplateHeadTag::MEANINGFUL_ITEM
                            | TemplateHeadTag::FORMATTER_DIRECTIVE
                            | TemplateHeadTag::RAW_DIRECTIVE,
                        required_absent_tags: TemplateHeadTag::empty(),
                        blocks_future_tags: TemplateHeadTag::FORMATTER_DIRECTIVE,
                    },
                    CoreStyleDirectiveKind::Raw,
                ),
                StyleDirectiveSpec::handler(
                    "markdown",
                    TemplateBodyMode::Normal,
                    TemplateHeadCompatibility {
                        presence_tags: TemplateHeadTag::MEANINGFUL_ITEM
                            | TemplateHeadTag::FORMATTER_DIRECTIVE,
                        required_absent_tags: TemplateHeadTag::empty(),
                        blocks_future_tags: TemplateHeadTag::FORMATTER_DIRECTIVE,
                    },
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
