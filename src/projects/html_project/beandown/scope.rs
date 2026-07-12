//! Caller-supplied Beandown scope placeholders.
//!
//! WHAT: defines the request-side scope shape promised by the direct API without exposing AST
//! folded constants, `StringId`s, `InternedPath`s, or const-record internals.
//! WHY: current compiler-integrated Beandown scope support is built from
//! header/public-surface data. A public conversion for arbitrary folded caller
//! constants needs a separate design so this API remains narrow instead of
//! leaking frontend internals.

use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct BeandownPathScope {
    pub(crate) source_path: PathBuf,
    pub(crate) constants: Vec<BeandownScopeConstant>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct BeandownScopeConstant {
    _private: (),
}

impl BeandownScopeConstant {
    #[cfg(test)]
    pub(crate) fn test_placeholder() -> Self {
        Self { _private: () }
    }
}
