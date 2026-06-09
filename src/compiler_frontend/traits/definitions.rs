//! Resolved trait metadata produced during AST environment construction.
//!
//! WHAT: stores trait definitions after names, visibility, `This`, and requirement signatures
//! have been resolved into compiler-owned identities.
//! WHY: traits are compile-time metadata, not `DataType` declarations. Keeping them here gives
//! conformance, bounds, and dynamic-trait phases a focused lookup surface without widening the
//! normal value/type declaration table.

use crate::compiler_frontend::ast::statements::functions::ReturnChannel;
use crate::compiler_frontend::datatypes::ids::TypeId;
use crate::compiler_frontend::symbols::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::StringId;
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;
use crate::compiler_frontend::traits::ids::{TraitId, TraitRequirementId};
use crate::compiler_frontend::value_mode::ValueMode;

/// Source/visibility ownership for a resolved trait definition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum TraitVisibility {
    /// Authored source trait. `exported` means the trait is part of its file/facade public surface.
    Source { exported: bool },
    /// Compiler-owned core trait metadata visible without a user declaration.
    Core,
}

/// Dynamic-safety classification for using a trait as an erased runtime value type.
///
/// WHAT: records whether a trait can be called through erased concrete identity.
/// WHY: all traits remain valid static bounds, but only dynamic-safe traits may resolve as normal
/// value type annotations.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum TraitDynamicSafety {
    DynamicSafe,
    BoundOnly {
        reason: BoundOnlyTraitReason,
        offending_requirement: TraitRequirementId,
    },
}

/// Reason a trait can only be used as a static bound.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BoundOnlyTraitReason {
    ThisParameter,
    ThisReturn,
}

/// Resolved method requirement inside a trait declaration.
#[derive(Clone, Debug)]
#[allow(dead_code)] // Complete requirement facts are retained for diagnostics and HIR projection.
pub(crate) struct ResolvedTraitRequirement {
    pub(crate) id: TraitRequirementId,
    pub(crate) name: StringId,
    pub(crate) name_location: SourceLocation,
    pub(crate) receiver: TraitReceiverRequirement,
    pub(crate) parameters: Vec<ResolvedTraitParameter>,
    pub(crate) returns: Vec<ResolvedTraitReturn>,
    pub(crate) location: SourceLocation,
}

/// Required receiver access for a trait method.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TraitReceiverRequirement {
    Immutable { this_type: TypeId },
    Mutable { this_type: TypeId },
}

/// One non-receiver requirement parameter.
#[derive(Clone, Debug)]
#[allow(dead_code)] // Parameter names/locations remain available for precise diagnostics.
pub(crate) struct ResolvedTraitParameter {
    pub(crate) name: InternedPath,
    pub(crate) value_mode: ValueMode,
    pub(crate) type_id: TypeId,
    pub(crate) location: SourceLocation,
}

/// One requirement return slot.
#[derive(Clone, Debug)]
#[allow(dead_code)] // Return locations remain available for precise diagnostics.
pub(crate) struct ResolvedTraitReturn {
    pub(crate) type_id: TypeId,
    pub(crate) channel: ReturnChannel,
    pub(crate) location: SourceLocation,
}

/// Complete resolved trait definition.
#[derive(Clone, Debug)]
#[allow(dead_code)] // Complete trait facts are projected across frontend/HIR/backend boundaries.
pub(crate) struct ResolvedTraitDefinition {
    pub(crate) id: TraitId,
    pub(crate) name: StringId,
    pub(crate) canonical_path: InternedPath,
    pub(crate) source_file: InternedPath,
    pub(crate) this_type: TypeId,
    pub(crate) requirements: Vec<ResolvedTraitRequirement>,
    pub(crate) declaration_location: SourceLocation,
    pub(crate) visibility: TraitVisibility,
    pub(crate) dynamic_safety: TraitDynamicSafety,
}
