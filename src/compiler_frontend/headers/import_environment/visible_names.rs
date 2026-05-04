//! Header-stage visible-name registry.
//!
//! WHAT: tracks which names are visible in one source file, classifies their binding kind,
//! and detects collisions.
//! WHY: same-file declarations, imports, aliases, builtins, and prelude symbols share one
//! namespace; silent shadowing must be rejected before AST body parsing.
//! MUST NOT: resolve import paths to files or external packages (that belongs in target
//! and facade resolution).

use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::compiler_warnings::{CompilerWarning, WarningKind};
use crate::compiler_frontend::external_packages::ExternalSymbolId;
use crate::compiler_frontend::headers::import_environment::diagnostics;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::{StringId, StringTable};
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;
use rustc_hash::FxHashMap;

/// Classification of a visible name binding.
///
/// WHY: collision logic depends on whether two bindings refer to the same underlying target.
/// Enums make the resolution path explicit in type names and match arms.
pub(crate) enum VisibleNameBinding {
    SameFileDeclaration { declaration_path: InternedPath },
    SourceImport { canonical_path: InternedPath },
    TypeAlias { canonical_path: InternedPath },
    ExternalImport { symbol_id: ExternalSymbolId },
    Builtin,
    Prelude { symbol_id: ExternalSymbolId },
}

/// Result of attempting to register a visible name.
#[allow(dead_code)]
pub(crate) enum RegisterVisibleNameResult {
    /// Name was registered without conflict.
    Registered,
    /// Name collides with an existing binding that targets a different symbol.
    Duplicate { previous: VisibleNameBinding },
}

/// Per-file registry of visible names.
///
/// WHAT: maintains a map from local spelling to its binding classification.
/// WHY: centralizing collision checks prevents drift between same-file declarations,
/// imports, builtins, and prelude symbols.
pub(crate) struct VisibleNameRegistry {
    names: FxHashMap<StringId, VisibleNameBinding>,
}

impl VisibleNameRegistry {
    pub(crate) fn new() -> Self {
        Self {
            names: FxHashMap::default(),
        }
    }

    /// Attempt to register a visible name.
    ///
    /// WHY: same target from two sources (e.g., re-importing the same symbol) is harmless.
    /// Different targets with the same local spelling is a collision.
    pub(crate) fn register(
        &mut self,
        local_name: StringId,
        binding: VisibleNameBinding,
        location: SourceLocation,
        string_table: &StringTable,
    ) -> Result<RegisterVisibleNameResult, CompilerError> {
        if let Some(previous) = self.names.get(&local_name) {
            if is_same_target(previous, &binding) {
                return Ok(RegisterVisibleNameResult::Registered);
            }
            return Err(diagnostics::import_name_collision(
                local_name,
                location,
                string_table,
            ));
        }
        self.names.insert(local_name, binding);
        Ok(RegisterVisibleNameResult::Registered)
    }

    /// Check whether a name is already registered.
    #[allow(dead_code)]
    pub(crate) fn is_registered(&self, name: StringId) -> bool {
        self.names.contains_key(&name)
    }

    /// Retrieve the binding for a name, if any.
    #[allow(dead_code)]
    pub(crate) fn get(&self, name: StringId) -> Option<&VisibleNameBinding> {
        self.names.get(&name)
    }
}

/// Determine whether two bindings refer to the same underlying target.
///
/// WHY: importing the same symbol twice (without alias, or with the same alias) is not an error.
fn is_same_target(a: &VisibleNameBinding, b: &VisibleNameBinding) -> bool {
    match (a, b) {
        (
            VisibleNameBinding::SameFileDeclaration {
                declaration_path: a_path,
            }
            | VisibleNameBinding::SourceImport {
                canonical_path: a_path,
            }
            | VisibleNameBinding::TypeAlias {
                canonical_path: a_path,
            },
            VisibleNameBinding::SameFileDeclaration {
                declaration_path: b_path,
            }
            | VisibleNameBinding::SourceImport {
                canonical_path: b_path,
            }
            | VisibleNameBinding::TypeAlias {
                canonical_path: b_path,
            },
        ) => a_path == b_path,
        (
            VisibleNameBinding::ExternalImport { symbol_id: a_id },
            VisibleNameBinding::ExternalImport { symbol_id: b_id },
        ) => a_id == b_id,
        (
            VisibleNameBinding::Prelude { symbol_id: a_id },
            VisibleNameBinding::Prelude { symbol_id: b_id },
        ) => a_id == b_id,
        (
            VisibleNameBinding::ExternalImport { symbol_id: a_id },
            VisibleNameBinding::Prelude { symbol_id: b_id },
        )
        | (
            VisibleNameBinding::Prelude { symbol_id: a_id },
            VisibleNameBinding::ExternalImport { symbol_id: b_id },
        ) => a_id == b_id,
        _ => false,
    }
}

/// Generate a case-convention warning when an alias uses different leading case than the symbol.
///
/// WHY: Beanstalk naming conventions use leading case to distinguish types from values.
/// An alias that changes leading case is allowed but warned because it misleads readers.
pub(crate) fn check_alias_case_warning(
    alias_location: &Option<SourceLocation>,
    path_location: &SourceLocation,
    local_name: StringId,
    symbol_name: StringId,
    string_table: &StringTable,
) -> Option<CompilerWarning> {
    let alias_str = string_table.resolve(local_name);
    let symbol_str = string_table.resolve(symbol_name);

    let a = alias_str.chars().next()?;
    let s = symbol_str.chars().next()?;

    if !a.is_alphabetic() || !s.is_alphabetic() {
        return None;
    }

    let alias_upper = a.is_uppercase();
    let symbol_upper = s.is_uppercase();

    if alias_upper == symbol_upper {
        return None;
    }

    let location = alias_location
        .clone()
        .unwrap_or_else(|| path_location.clone());

    Some(CompilerWarning::new(
        &format!(
            "Import alias '{alias_str}' uses different leading-name case than imported symbol '{symbol_str}'."
        ),
        location,
        WarningKind::ImportAliasCaseMismatch,
    ))
}
