//! Test module entry for the builtin cast surface.
//!
//! WHAT: gathers the policy, evidence, trait metadata, and target unit
//!      tests into one sub-tree so they run alongside other cast tests.
//! WHY: keeps test files separate from production code while giving the
//!      cast module one clear test surface to read.

#[path = "policies_tests.rs"]
mod policies_tests;

#[path = "evidence_tests.rs"]
mod evidence_tests;

#[path = "trait_metadata_tests.rs"]
mod trait_metadata_tests;
