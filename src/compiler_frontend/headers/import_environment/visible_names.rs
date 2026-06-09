//! Header-stage visible-name registry.
//!
//! WHAT: tracks which names are visible in one source file, classifies their binding kind,
//! and detects collisions.
//! WHY: same-file declarations, imports, aliases, builtins, and prelude symbols share one
//! namespace; silent shadowing must be rejected before AST body parsing.
//! MUST NOT: resolve import paths to files or external packages (that belongs in target
//! and facade resolution).

use crate::compiler_frontend::compiler_messages::CompilerDiagnostic;
use crate::compiler_frontend::external_packages::{ExternalFunctionId, ExternalSymbolId};
use crate::compiler_frontend::headers::import_environment::NamespaceRecordSource;
use crate::compiler_frontend::headers::import_environment::diagnostics;
use crate::compiler_frontend::symbols::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::{StringId, StringTable};
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;
use rustc_hash::FxHashMap;

/// Target of an explicit grouped receiver-method import.
///
/// WHY: source and external receiver methods use different canonical identifiers,
/// so the registry needs to distinguish them for `is_same_target`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ReceiverMethodImportTarget {
    SourceMethod { canonical_path: InternedPath },
    ExternalMethod { function_id: ExternalFunctionId },
}

/// Classification of a visible name binding.
///
/// WHY: collision logic depends on whether two bindings refer to the same underlying target.
/// Enums make the resolution path explicit in type names and match arms.
pub(crate) enum VisibleNameBinding {
    SameFileDeclaration {
        declaration_path: InternedPath,
    },
    SourceImport {
        canonical_path: InternedPath,
    },
    TypeAlias {
        canonical_path: InternedPath,
    },
    Trait {
        canonical_path: InternedPath,
    },
    ExternalImport {
        symbol_id: ExternalSymbolId,
    },
    Builtin,
    Prelude {
        symbol_id: ExternalSymbolId,
    },
    NamespaceRecord {
        record_source: NamespaceRecordSource,
    },
    ReceiverMethodImport {
        target: ReceiverMethodImportTarget,
    },
}

/// Stored entry for a registered visible name, including the binding and its source location.
///
/// WHY: preserving the original location lets collision diagnostics emit secondary labels
/// pointing to the first declaration or import.
struct VisibleNameEntry {
    binding: VisibleNameBinding,
    location: SourceLocation,
}

/// Per-file registry of visible names.
///
/// WHAT: maintains a map from local spelling to its binding classification and source location.
/// WHY: centralizing collision checks prevents drift between same-file declarations,
/// imports, builtins, and prelude symbols.
pub(crate) struct VisibleNameRegistry {
    names: FxHashMap<StringId, VisibleNameEntry>,
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
    ///
    /// Returns `Ok(())` when the name is registered or already present with the same target.
    /// Returns `Err` with a structured diagnostic when the name collides with a different target.
    // The typed diagnostic payload is still large enough to trigger clippy::result_large_err here.
    #[allow(clippy::result_large_err)]
    pub(crate) fn register(
        &mut self,
        local_name: StringId,
        binding: VisibleNameBinding,
        location: SourceLocation,
    ) -> Result<(), CompilerDiagnostic> {
        if let Some(entry) = self.names.get(&local_name) {
            if can_coexist(&entry.binding, &binding) {
                return Ok(());
            }
            return Err(diagnostics::import_name_collision(
                local_name,
                location,
                Some(entry.location.clone()),
            ));
        }
        self.names
            .insert(local_name, VisibleNameEntry { binding, location });
        Ok(())
    }

    /// Retrieve the binding for a name, if any.
    pub(crate) fn get(&self, name: StringId) -> Option<&VisibleNameBinding> {
        self.names.get(&name).map(|entry| &entry.binding)
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
            }
            | VisibleNameBinding::Trait {
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
            }
            | VisibleNameBinding::Trait {
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
        (
            VisibleNameBinding::NamespaceRecord {
                record_source: a_src,
            },
            VisibleNameBinding::NamespaceRecord {
                record_source: b_src,
            },
        ) => a_src == b_src,
        (
            VisibleNameBinding::ReceiverMethodImport { target: a_target },
            VisibleNameBinding::ReceiverMethodImport { target: b_target },
        ) => match (a_target, b_target) {
            (
                ReceiverMethodImportTarget::SourceMethod {
                    canonical_path: a_path,
                },
                ReceiverMethodImportTarget::SourceMethod {
                    canonical_path: b_path,
                },
            ) => a_path == b_path,
            (
                ReceiverMethodImportTarget::ExternalMethod { function_id: a_id },
                ReceiverMethodImportTarget::ExternalMethod { function_id: b_id },
            ) => a_id == b_id,
            _ => false,
        },
        _ => false,
    }
}

/// Determine whether two bindings can coexist under the same visible name.
///
/// WHY: receiver methods are dispatched by `(receiver_type, method_name)`, so two
/// receiver-method imports with the same local name but different underlying targets
/// (different source paths or different external function IDs) do not conflict in practice.
/// They must still be prevented from colliding with ordinary value/type bindings.
fn can_coexist(a: &VisibleNameBinding, b: &VisibleNameBinding) -> bool {
    if is_same_target(a, b) {
        return true;
    }
    matches!(
        (a, b),
        (
            VisibleNameBinding::ReceiverMethodImport { .. },
            VisibleNameBinding::ReceiverMethodImport { .. },
        )
    )
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
) -> Option<CompilerDiagnostic> {
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

    Some(CompilerDiagnostic::import_alias_case_mismatch(
        local_name,
        symbol_name,
        location,
    ))
}
