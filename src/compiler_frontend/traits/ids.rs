//! Compact identifiers owned by the trait metadata environment.
//!
//! WHAT: defines dense IDs for resolved trait definitions and their method requirements.
//! WHY: later trait phases need stable typed handles instead of passing paths or names through
//! evidence validation, generic bounds, dynamic safety, and call resolution.

/// Dense module-local identifier for one resolved trait definition.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TraitId(pub(crate) u32);

/// Dense module-local identifier for one requirement inside a resolved trait definition.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TraitRequirementId(pub(crate) u32);

/// Dense module-local identifier for one validated trait conformance evidence record.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct TraitEvidenceId(pub(crate) u32);
