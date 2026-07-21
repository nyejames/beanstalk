//! State-safe prepared source input for discovered project compilation.
//!
//! WHAT: one build-system-private owned enum variant per source kind. The Beanstalk variant
//!       carries the retained `FileTokens` from the single Stage 0 lexical pass; the Beandown and
//!       PlainMarkdown variants carry only raw source text.
//! WHY: the variant makes the source-kind/token relationship unrepresentable as an invalid
//!      state. A discovered Beanstalk source always carries its `FileTokens` by type, so frontend
//!      header preparation receives tokens directly and cannot panic on absent tokens, while
//!      Beandown and PlainMarkdown cannot accidentally carry Beanstalk tokens.
//!
//! This type is the build-system-owned storage threaded through `ReachableSourceInventory`
//! assembly, `DiscoveredModule`, single-file compilation and `FrontendModuleBuildContext`.

use crate::compiler_frontend::tokenizer::tokens::FileTokens;

use std::path::{Path, PathBuf};

/// Owned prepared source input carrying the strict source-kind/token relationship.
///
/// Construct this only from Stage 0 reachable-file discovery. Beanstalk files must already have
/// been tokenized once; the retained `FileTokens` are carried here so header preparation never
/// lexes the same source again.
///
/// The Beanstalk `tokens` are boxed so the enum is not sized by `FileTokens` (which is large);
/// the Beandown and PlainMarkdown variants stay small and moving a `PreparedSourceInput` only
/// copies a pointer for the retained token stream.
pub(crate) enum PreparedSourceInput {
    /// A Beanstalk module source with the retained token stream from its single lexical pass.
    Beanstalk {
        source_code: String,
        source_path: PathBuf,
        tokens: Box<FileTokens>,
    },
    /// A Beandown template body, tokenized once by the template-body preparation path.
    Beandown {
        source_code: String,
        source_path: PathBuf,
    },
    /// Plain Markdown content, never tokenized.
    PlainMarkdown {
        source_code: String,
        source_path: PathBuf,
    },
}

impl PreparedSourceInput {
    /// Raw source text for byte-counting and diagnostics.
    pub(crate) fn source_code(&self) -> &str {
        match self {
            PreparedSourceInput::Beanstalk { source_code, .. }
            | PreparedSourceInput::Beandown { source_code, .. }
            | PreparedSourceInput::PlainMarkdown { source_code, .. } => source_code,
        }
    }

    /// Canonical source path used for identity and source-table registration.
    pub(crate) fn source_path(&self) -> &Path {
        match self {
            PreparedSourceInput::Beanstalk { source_path, .. }
            | PreparedSourceInput::Beandown { source_path, .. }
            | PreparedSourceInput::PlainMarkdown { source_path, .. } => source_path,
        }
    }
}
