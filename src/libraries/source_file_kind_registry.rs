//! Builder-declared source file kind registry.
//!
//! WHAT: tracks which non-`.bst` source file kinds the active builder supports.
//! WHY: the compiler owns `.bst` as the built-in source kind; builders opt into additional
//!      file kinds such as Beandown `.bd` and plain Markdown `.md` through `LibrarySet` so support
//!      is builder-controlled.

use std::collections::HashMap;

/// Identifies a category of source file that the compiler can ingest.
///
/// WHAT: distinguishes built-in Beanstalk source from builder-supported extensions.
/// WHY: Stage 0 discovery and later frontend stages branch on source kind to apply the
///      correct tokenization, header preparation, and AST lowering rules.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SourceFileKind {
    /// Standard Beanstalk `.bst` source files.
    Beanstalk,
    /// Beandown `.bd` template-body files.
    Beandown,
    /// Plain Markdown `.md` content files.
    ///
    /// WHY: HTML projects can import Markdown as a generated `content #String` constant.
    PlainMarkdown,
}

/// A single registered source file kind mapping.
///
/// WHAT: pairs a file extension with its source kind for lookup.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SupportedSourceFileKind {
    pub extension: &'static str,
    pub kind: SourceFileKind,
}

/// Registry of builder-supported source file kinds.
///
/// WHAT: collects extensions the active builder wants the compiler to recognize.
/// WHY: keeps source-kind support declarative and builder-local instead of hard-coding
///      extensions in Stage 0 or import resolution.
///
/// `.bst` is always implicitly supported and does not need registration.
#[derive(Clone, Debug, Default)]
pub struct SourceFileKindRegistry {
    kinds: HashMap<&'static str, SourceFileKind>,
}

impl SourceFileKindRegistry {
    /// Creates an empty registry.
    pub fn new() -> Self {
        Self {
            kinds: HashMap::new(),
        }
    }

    /// Registers a source file kind for the given extension.
    ///
    /// WHY: builders declare support so the compiler can discover and handle non-`.bst`
    ///      source files during module building.
    pub fn register(&mut self, extension: &'static str, kind: SourceFileKind) {
        self.kinds.insert(extension, kind);
    }

    /// Looks up the source kind for a file extension.
    ///
    /// Returns `None` for unrecognized extensions. Callers should treat `.bst` as
    /// `SourceFileKind::Beanstalk` even when this returns `None`.
    pub fn kind_for_extension(&self, extension: &str) -> Option<SourceFileKind> {
        self.kinds.get(extension).copied()
    }

    /// Returns whether the given extension is registered as a supported source kind.
    pub fn is_supported(&self, extension: &str) -> bool {
        self.kinds.contains_key(extension)
    }

    /// Returns all registered supported source kinds.
    pub fn supported_kinds(&self) -> Vec<SupportedSourceFileKind> {
        let mut supported_kinds: Vec<_> = self
            .kinds
            .iter()
            .map(|(&extension, &kind)| SupportedSourceFileKind { extension, kind })
            .collect();

        supported_kinds.sort_by_key(|kind| kind.extension);
        supported_kinds
    }

    /// Returns whether this registry supports a recognized source-file extension.
    ///
    /// `.bst` is compiler-owned and always supported. Builder-owned source kinds must be
    /// explicitly registered by the active builder.
    pub fn supports_recognized_extension(&self, extension: &str) -> bool {
        match SourceFileKind::from_extension(extension) {
            Some(SourceFileKind::Beanstalk) => true,
            Some(kind) => self.kind_for_extension(extension) == Some(kind),
            None => false,
        }
    }
}

impl SourceFileKind {
    /// Looks up compiler-recognized source-file kinds by extension.
    ///
    /// WHAT: separates recognition from active-builder support.
    /// WHY: Stage 0 must diagnose a known but unsupported source kind, such as `.bd` under a
    ///      non-HTML builder, instead of falling through to a missing-import error.
    pub fn from_extension(extension: &str) -> Option<Self> {
        match extension {
            "bst" => Some(Self::Beanstalk),
            "bd" => Some(Self::Beandown),
            "md" => Some(Self::PlainMarkdown),
            _ => None,
        }
    }

    /// Returns the canonical extension for this source-file kind.
    pub fn extension(self) -> &'static str {
        match self {
            Self::Beanstalk => "bst",
            Self::Beandown => "bd",
            Self::PlainMarkdown => "md",
        }
    }

    /// Returns the canonical extension suffix used in source symbol paths.
    pub fn extension_suffix(self) -> &'static str {
        match self {
            Self::Beanstalk => ".bst",
            Self::Beandown => ".bd",
            Self::PlainMarkdown => ".md",
        }
    }

    /// Returns all compiler-recognized source-file kinds.
    pub fn recognized_kinds() -> &'static [SupportedSourceFileKind] {
        const RECOGNIZED_KINDS: &[SupportedSourceFileKind] = &[
            SupportedSourceFileKind {
                extension: "bst",
                kind: SourceFileKind::Beanstalk,
            },
            SupportedSourceFileKind {
                extension: "bd",
                kind: SourceFileKind::Beandown,
            },
            SupportedSourceFileKind {
                extension: "md",
                kind: SourceFileKind::PlainMarkdown,
            },
        ];

        RECOGNIZED_KINDS
    }
}
