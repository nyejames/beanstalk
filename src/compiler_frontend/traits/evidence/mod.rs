//! Trait conformance evidence module.
//!
//! WHAT: organizes evidence structures, environment lookup, conformance target resolution,
//!       conformance validation, signature matching, and diagnostics.
//! WHY: keeps compiler stage boundaries clean and makes trait conformance code easier to maintain.

pub(crate) mod diagnostics;
pub(crate) mod environment;
pub(crate) mod requirement_matching;
pub(crate) mod target_resolution;
pub(crate) mod validation;

pub(crate) use environment::{TraitEvidenceDefinition, TraitEvidenceEnvironment};
pub(crate) use validation::{ValidateTraitEvidenceInput, validate_trait_evidence};
