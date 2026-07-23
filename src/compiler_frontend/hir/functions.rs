//! HIR function declarations.
//!
//! WHAT: function-level HIR metadata, including entry block, parameters, return type, and semantic
//! origin classification.
//! WHY: backends need to distinguish regular functions from the implicit entry `start` function.

use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::datatypes::ids::TypeId;
use crate::compiler_frontend::hir::ids::{BlockId, FunctionId, LocalId};
use crate::compiler_frontend::semantic_identity::OriginFunctionId;
use crate::compiler_frontend::symbols::interned_path::InternedPath;
use rustc_hash::{FxHashMap, FxHashSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HirFunctionOrigin {
    /// Regular user-declared function.
    Normal,
    /// Implicit start function for the module entry file.
    EntryStart,
}

/// Transient exact declaration-path seed for one HIR lowering.
///
/// WHAT: carries the retained declaration path and stable origin until HIR assigns a local
/// `FunctionId`. It belongs to the AST-to-HIR stage handoff, not stable semantic identity.
/// WHY: public function joins need exact declaration identity without rendering names or relying
/// on declaration order. The seed is consumed before the completed HIR artefact boundary.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct FunctionOriginSeed {
    pub(crate) path: InternedPath,
    pub(crate) origin: OriginFunctionId,
}

/// Provider-independent stable-origin lookup retained only during one HIR lowering.
#[derive(Clone, Debug, Default)]
pub(crate) struct HirFunctionOriginLookup {
    by_path: FxHashMap<InternedPath, OriginFunctionId>,
}

impl HirFunctionOriginLookup {
    pub(crate) fn from_seeds(seeds: Vec<FunctionOriginSeed>) -> Result<Self, CompilerError> {
        let mut by_path = FxHashMap::default();
        let mut origins = FxHashSet::default();

        for seed in seeds {
            if origins.contains(&seed.origin) {
                return Err(CompilerError::compiler_error(format!(
                    "HIR function-origin lowering received duplicate stable origin {:?}",
                    seed.origin
                )));
            }
            if by_path.contains_key(&seed.path) {
                return Err(CompilerError::compiler_error(
                    "HIR function-origin lowering received duplicate declaration paths",
                ));
            }

            origins.insert(seed.origin.clone());
            by_path.insert(seed.path, seed.origin);
        }

        Ok(Self { by_path })
    }

    pub(crate) fn origin_for(&self, path: &InternedPath) -> Option<&OriginFunctionId> {
        self.by_path.get(path)
    }
}

#[derive(Debug, Clone)]
pub struct HirFunction {
    pub id: FunctionId,
    pub entry: BlockId,
    pub params: Vec<LocalId>,
    pub return_type: TypeId,
    pub return_aliases: Vec<Option<Vec<usize>>>,
}
