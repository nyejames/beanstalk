//! Transient AST-owned resolved public type-root handoff.
//!
//! WHAT: builds one deterministic vector of directly-defined active-root public type-root
//! records plus the receiver methods attached to directly-defined public nominal receivers,
//! from the same already-resolved facts consumed by public-surface validation.
//! WHY: canonical type projection needs resolved roots available immediately before HIR
//! lowering without reconstructing public semantics from HIR or source. This is transient
//! donor-local AST data consumed before HIR; it never enters `CompiledModuleResult`,
//! `Module`, or a cross-module interface.

use crate::compiler_frontend::ast::generic_functions::GenericFunctionTemplate;
use crate::compiler_frontend::ast::module_ast::environment::TopLevelDeclarationTable;
use crate::compiler_frontend::ast::module_ast::scope_context::ReceiverMethodCatalog;
use crate::compiler_frontend::ast::receiver_methods::ReceiverMethodEntry;
use crate::compiler_frontend::ast::statements::functions::FunctionSignature;
use crate::compiler_frontend::ast::type_resolution::ResolvedFunctionSignature;
use crate::compiler_frontend::ast::type_resolution::ResolvedTypeAnnotation;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::datatypes::ReceiverKey;
use crate::compiler_frontend::datatypes::ids::{GenericParameterListId, TypeId};
use crate::compiler_frontend::headers::parse_file_headers::{FileRole, Header, HeaderKind};
use crate::compiler_frontend::symbols::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::StringTable;

use rustc_hash::{FxHashMap, FxHashSet};

/// The resolved category and identity of one directly-defined active-root public type root.
///
/// WHAT: one root admitted by the active-root public declaration-kind gate, carrying the
/// resolved `TypeId` facts already produced by AST environment construction.
/// WHY: keeps the handoff value-shaped and deterministic so later projection reads named
/// roots and their resolved identities without re-scanning headers or HIR.
/// Consumed by the defined public type-surface projection immediately before HIR lowering.
#[derive(Clone, Debug)]
pub(crate) enum ResolvedPublicTypeRootKind {
    /// Public free function with resolved parameter and return `TypeId`s.
    ///
    /// WHAT: carries the full resolved signature so parameter and return `TypeId`s, including
    /// generic-parameter `TypeId`s, remain intact for later projection. Receiver methods are
    /// not free roots; they are selected in a separate pass into `receiver_methods`.
    Function {
        signature: FunctionSignature,
        generic_parameter_list_id: Option<GenericParameterListId>,
    },

    /// Public nominal struct with its canonical `TypeId`.
    Struct { type_id: TypeId },

    /// Public nominal choice with its canonical `TypeId`.
    Choice { type_id: TypeId },

    /// Public transparent type alias with its resolved target `TypeId`.
    ///
    /// WHAT: the target `TypeId` is materialized once by the public-surface validation owner
    /// and retained on the resolved annotation. This table consumes the retained fact; it
    /// does not resolve the alias target a second time. The alias does not introduce its own
    /// type identity; only the resolved target `TypeId` is carried.
    TransparentAlias { target_type_id: TypeId },

    /// Public compile-time constant with its resolved `TypeId`.
    Constant { type_id: TypeId },
}

/// One named directly-defined active-root public type root.
/// Consumed by the defined public type-surface projection immediately before HIR lowering.
#[derive(Clone, Debug)]
pub(crate) struct ResolvedPublicTypeRoot {
    pub(crate) path: InternedPath,
    pub(crate) kind: ResolvedPublicTypeRootKind,
}

/// Transient AST-owned table of resolved public type roots for the module handoff.
///
/// WHAT: `roots` holds the named declaration roots in deterministic sorted-header order;
/// `receiver_methods` holds the receiver methods attached to directly-defined public
/// nominal receivers, selected in a separate pass so private method headers outside
/// `export:` are admitted by receiver ownership rather than their own export binding.
/// WHY: the semantic orchestration consumes this immediately before HIR lowering in the
/// next slice. Donor-local `TypeId`s must not cross the module result boundary.
/// Consumed by the defined public type-surface projection immediately before HIR lowering;
/// retained as transient donor-local AST data that never crosses the module result boundary.
#[derive(Clone, Default)]
pub(crate) struct ResolvedPublicTypeRootTable {
    pub(crate) roots: Vec<ResolvedPublicTypeRoot>,
    pub(crate) receiver_methods: Vec<ReceiverMethodEntry>,
}

/// Inputs required to build the resolved public type-root table.
///
/// WHAT: groups the resolved signature, nominal identity, alias, constant and receiver
/// side tables used while building the root table.
/// WHY: building the table sits at the join of completed AST environment facts; keeping the
/// inputs named makes that boundary easier to audit than a long positional list.
pub(crate) struct BuildResolvedPublicTypeRootsInput<'a> {
    pub sorted_headers: &'a [Header],
    pub resolved_function_signatures_by_path:
        &'a FxHashMap<InternedPath, ResolvedFunctionSignature>,
    pub nominal_type_ids_by_path: &'a FxHashMap<InternedPath, TypeId>,
    pub resolved_type_aliases_by_path: &'a FxHashMap<InternedPath, ResolvedTypeAnnotation>,
    pub declaration_table: &'a TopLevelDeclarationTable,
    pub generic_function_templates_by_path: &'a FxHashMap<InternedPath, GenericFunctionTemplate>,
    pub receiver_methods: &'a ReceiverMethodCatalog,
    pub string_table: &'a StringTable,
}

/// Build the resolved public type-root table from completed AST environment facts.
///
/// WHAT: walks `sorted_headers` in two deterministic passes. The first pass admits only
/// directly-defined active-root public declarations: free functions, nominal structs/choices,
/// transparent aliases and constants become roots, and the complete set of public nominal
/// paths is collected. The second pass selects receiver methods: every active-root function
/// header whose resolved receiver is one of the directly-defined public nominal roots is
/// admitted as a method entry, regardless of the method header's own export mode. Real
/// methods on a public receiver are normally private headers outside `export:`, so the method
/// pass ignores export mode and selects by receiver ownership. Every admitted root requires
/// an already-resolved fact; missing data is an internal `CompilerError` rather than silent
/// omission.
/// WHY: two passes over the same sorted headers keep a single deterministic owner and let
/// the method pass see the complete public nominal set even when a method precedes its
/// receiver in supplied order. One pass would miss methods that appear before their nominal
/// receiver and would wrongly gate private methods on their own export binding.
pub(crate) fn build_resolved_public_type_roots(
    input: BuildResolvedPublicTypeRootsInput<'_>,
) -> Result<ResolvedPublicTypeRootTable, CompilerError> {
    let BuildResolvedPublicTypeRootsInput {
        sorted_headers,
        resolved_function_signatures_by_path,
        nominal_type_ids_by_path,
        resolved_type_aliases_by_path,
        declaration_table,
        generic_function_templates_by_path,
        receiver_methods,
        string_table,
    } = input;

    let mut roots = Vec::new();
    // Directly-defined active-root public nominal paths, collected in full before the method
    // pass so method selection is independent of where a method appears relative to its
    // receiver in sorted-header order.
    let mut public_nominal_paths: FxHashSet<InternedPath> = FxHashSet::default();

    // Pass 1: collect declaration roots and the complete public nominal path set.
    for header in sorted_headers {
        if !is_active_root_public_declaration(header) {
            continue;
        }

        let path = &header.tokens.src_path;

        match &header.kind {
            HeaderKind::Function { .. } => {
                let Some(resolved) = resolved_function_signatures_by_path.get(path) else {
                    return Err(missing_resolved_function_signature(path, string_table));
                };

                // Receiver methods are not free export bindings. They are selected in the
                // separate method pass by receiver ownership, not here.
                if resolved.receiver.is_some() {
                    continue;
                }

                let generic_parameter_list_id = generic_function_templates_by_path
                    .get(path)
                    .map(|template| template.generic_parameter_list_id);

                roots.push(ResolvedPublicTypeRoot {
                    path: path.to_owned(),
                    kind: ResolvedPublicTypeRootKind::Function {
                        signature: resolved.signature.clone(),
                        generic_parameter_list_id,
                    },
                });
            }

            HeaderKind::Struct { .. } => {
                let Some(&type_id) = nominal_type_ids_by_path.get(path) else {
                    return Err(missing_nominal_type_id(path, string_table));
                };
                public_nominal_paths.insert(path.to_owned());
                roots.push(ResolvedPublicTypeRoot {
                    path: path.to_owned(),
                    kind: ResolvedPublicTypeRootKind::Struct { type_id },
                });
            }

            HeaderKind::Choice { .. } => {
                let Some(&type_id) = nominal_type_ids_by_path.get(path) else {
                    return Err(missing_nominal_type_id(path, string_table));
                };
                public_nominal_paths.insert(path.to_owned());
                roots.push(ResolvedPublicTypeRoot {
                    path: path.to_owned(),
                    kind: ResolvedPublicTypeRootKind::Choice { type_id },
                });
            }

            HeaderKind::TypeAlias { .. } => {
                let Some(annotation) = resolved_type_aliases_by_path.get(path) else {
                    return Err(missing_resolved_alias(path, string_table));
                };
                // The public-surface validation owner materializes and retains the alias
                // target `TypeId`. This table consumes the retained fact; a missing `TypeId`
                // here is an internal invariant failure, not a reason to resolve again.
                let Some(target_type_id) = annotation.type_id else {
                    return Err(missing_resolved_alias_type_id(path, string_table));
                };
                roots.push(ResolvedPublicTypeRoot {
                    path: path.to_owned(),
                    kind: ResolvedPublicTypeRootKind::TransparentAlias { target_type_id },
                });
            }

            HeaderKind::Constant { .. } => {
                let Some(declaration) = declaration_table.get_by_path(path) else {
                    return Err(missing_resolved_constant(path, string_table));
                };
                roots.push(ResolvedPublicTypeRoot {
                    path: path.to_owned(),
                    kind: ResolvedPublicTypeRootKind::Constant {
                        type_id: declaration.value.type_id,
                    },
                });
            }

            // Trait/evidence semantic projection remains a later slice; trait declarations
            // pass the declaration-kind gate but are not type-root categories yet.
            HeaderKind::Trait { .. } => {}
            HeaderKind::ConstTemplate { .. }
            | HeaderKind::StartFunction
            | HeaderKind::TraitConformance { .. }
            | HeaderKind::TraitIncompatibility { .. } => {}
        }
    }

    // Pass 2: select receiver methods attached to directly-defined public nominal receivers.
    // Real methods on a public receiver are normally private headers outside `export:`, so
    // this pass ignores the method header's export mode and selects by receiver ownership.
    // Imported module roots and builtin/external receivers are excluded because their
    // nominal paths are not in the directly-defined public nominal set.
    let mut receiver_method_entries = Vec::new();
    for header in sorted_headers {
        if header.file_role != FileRole::ActiveModuleRoot {
            continue;
        }
        if !matches!(&header.kind, HeaderKind::Function { .. }) {
            continue;
        }

        let path = &header.tokens.src_path;
        // AST environment construction resolves every function signature before this table,
        // so an active-root function missing from the signature table is an internal error,
        // not a skip. Imported roots and non-function headers are excluded above.
        let Some(resolved) = resolved_function_signatures_by_path.get(path) else {
            return Err(missing_resolved_function_signature(path, string_table));
        };
        let Some(receiver) = resolved.receiver.as_ref() else {
            continue;
        };
        let Some(receiver_path) = nominal_receiver_path(receiver) else {
            continue;
        };
        if !public_nominal_paths.contains(receiver_path) {
            continue;
        }

        let Some(entry) = receiver_methods.by_function_path.get(path) else {
            return Err(missing_receiver_method_entry(path, string_table));
        };
        receiver_method_entries.push(entry.clone());
    }

    Ok(ResolvedPublicTypeRootTable {
        roots,
        receiver_methods: receiver_method_entries,
    })
}

/// Whether a header is a directly-defined active-root public authored declaration.
///
/// WHAT: only declarations in the active module root's public export surface, admitted by the
/// shared [`HeaderKind::is_authored_public_export_declaration`] gate, become roots. Imported
/// module roots, private declarations, builtin declarations and source-package-only facts
/// are excluded.
/// WHY: the retained table covers only roots directly defined by the module being compiled.
fn is_active_root_public_declaration(header: &Header) -> bool {
    header.file_role == FileRole::ActiveModuleRoot
        && header.export_mode.is_public()
        && header.kind.is_authored_public_export_declaration()
}

/// The nominal receiver path for a struct/choice receiver, if any.
///
/// WHAT: external and builtin-scalar receivers are not directly-defined nominal roots, so
/// their methods are not retained by this table.
fn nominal_receiver_path(receiver: &ReceiverKey) -> Option<&InternedPath> {
    match receiver {
        ReceiverKey::Struct(path) | ReceiverKey::Choice(path) => Some(path),
        ReceiverKey::External(_) | ReceiverKey::BuiltinScalar(_) => None,
    }
}

fn missing_resolved_function_signature(
    path: &InternedPath,
    string_table: &StringTable,
) -> CompilerError {
    CompilerError::compiler_error(format!(
        "Active-root function '{}' had no resolved signature during root-table construction.",
        path.to_string(string_table)
    ))
}

fn missing_nominal_type_id(path: &InternedPath, string_table: &StringTable) -> CompilerError {
    CompilerError::compiler_error(format!(
        "Public active-root nominal declaration '{}' had no canonical TypeId during root-table construction.",
        path.to_string(string_table)
    ))
}

fn missing_resolved_alias(path: &InternedPath, string_table: &StringTable) -> CompilerError {
    CompilerError::compiler_error(format!(
        "Public active-root transparent alias '{}' had no resolved annotation during root-table construction.",
        path.to_string(string_table)
    ))
}

fn missing_resolved_alias_type_id(
    path: &InternedPath,
    string_table: &StringTable,
) -> CompilerError {
    CompilerError::compiler_error(format!(
        "Public active-root transparent alias '{}' had no retained target TypeId during root-table construction; the public-surface owner should have materialized and retained it.",
        path.to_string(string_table)
    ))
}

fn missing_resolved_constant(path: &InternedPath, string_table: &StringTable) -> CompilerError {
    CompilerError::compiler_error(format!(
        "Public active-root constant '{}' had no resolved declaration during root-table construction.",
        path.to_string(string_table)
    ))
}

fn missing_receiver_method_entry(path: &InternedPath, string_table: &StringTable) -> CompilerError {
    CompilerError::compiler_error(format!(
        "Public receiver method '{}' was missing from the receiver catalog during root-table construction.",
        path.to_string(string_table)
    ))
}
