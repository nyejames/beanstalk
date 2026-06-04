//! Output types for the direct Beandown API.
//!
//! WHAT: keeps the public result surface limited to compiled strings, source paths, relative
//! directory metadata, and warnings.
//! WHY: callers should not depend on AST constants, interned paths, folded-value internals, HIR, or
//! builder artifact policy.

use crate::compiler_frontend::compiler_messages::CompilerDiagnostic;
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct BeandownCompileOutput {
    pub(crate) documents: Vec<CompiledBeandownDocument>,
    pub(crate) warnings: Vec<CompilerDiagnostic>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CompiledBeandownDocument {
    pub(crate) source_path: PathBuf,
    pub(crate) relative_path: Option<PathBuf>,
    pub(crate) content: String,
}
