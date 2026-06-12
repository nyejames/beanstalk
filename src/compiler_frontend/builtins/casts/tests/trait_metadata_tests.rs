//! Core cast trait metadata unit tests.
//!
//! WHAT: pins down the table-driven metadata table for the twelve core
//!      cast traits so future additions cannot drift on the requirement
//!      name, builtin target, or fallibility classifier.
//! WHY: the metadata table is the single source of truth for the cast
//!      trait catalogue. Tests here prevent accidental edits to the
//!      requirement names and target classifications that other phases
//!      rely on.

use crate::compiler_frontend::builtins::casts::targets::{
    BuiltinCastFallibility, BuiltinCastTarget,
};
use crate::compiler_frontend::builtins::casts::traits::{
    BUILTIN_CAST_TRAIT_ROWS, CoreCastTrait, builtin_cast_requirement_name,
    builtin_cast_trait_fallibility, builtin_cast_trait_metadata, builtin_cast_trait_name,
    builtin_cast_trait_target, is_core_cast_trait_name,
};

#[test]
fn metadata_table_covers_every_core_cast_trait() {
    assert_eq!(BUILTIN_CAST_TRAIT_ROWS.len(), 12);
    let mut kinds: Vec<CoreCastTrait> =
        BUILTIN_CAST_TRAIT_ROWS.iter().map(|row| row.kind).collect();
    kinds.sort_by_key(|kind| format!("{kind:?}"));
    kinds.dedup();
    assert_eq!(kinds.len(), 12, "every variant must appear exactly once");
}

#[test]
fn infallible_traits_use_to_requirement_prefix() {
    for row in BUILTIN_CAST_TRAIT_ROWS {
        if row.fallibility == BuiltinCastFallibility::Infallible {
            let expected = format!(
                "to_{}",
                match row.target {
                    BuiltinCastTarget::Bool => "bool",
                    BuiltinCastTarget::Int => "int",
                    BuiltinCastTarget::String => "string",
                    BuiltinCastTarget::Char => "char",
                    BuiltinCastTarget::Float => "float",
                    BuiltinCastTarget::Error => "error",
                }
            );
            assert_eq!(
                row.requirement_name, expected,
                "{:?} requirement name",
                row.kind
            );
        }
    }
}

#[test]
fn fallible_traits_use_try_to_requirement_prefix() {
    for row in BUILTIN_CAST_TRAIT_ROWS {
        if row.fallibility == BuiltinCastFallibility::Fallible {
            let expected = format!(
                "try_to_{}",
                match row.target {
                    BuiltinCastTarget::Bool => "bool",
                    BuiltinCastTarget::Int => "int",
                    BuiltinCastTarget::String => "string",
                    BuiltinCastTarget::Char => "char",
                    BuiltinCastTarget::Float => "float",
                    BuiltinCastTarget::Error => "error",
                }
            );
            assert_eq!(
                row.requirement_name, expected,
                "{:?} requirement name",
                row.kind
            );
        }
    }
}

#[test]
fn trait_names_match_plan_section_3_1() {
    assert_eq!(
        builtin_cast_trait_name(CoreCastTrait::CastableToInt),
        "CASTABLE_TO_INT"
    );
    assert_eq!(
        builtin_cast_trait_name(CoreCastTrait::TryCastableToError),
        "TRY_CASTABLE_TO_ERROR"
    );
    assert_eq!(
        builtin_cast_trait_name(CoreCastTrait::CastableToFloat),
        "CASTABLE_TO_FLOAT"
    );
    assert_eq!(
        builtin_cast_trait_name(CoreCastTrait::CastableToString),
        "CASTABLE_TO_STRING"
    );
}

#[test]
fn target_lookup_round_trips_through_metadata() {
    for kind in [
        CoreCastTrait::CastableToInt,
        CoreCastTrait::TryCastableToInt,
        CoreCastTrait::CastableToError,
        CoreCastTrait::TryCastableToError,
    ] {
        let metadata = builtin_cast_trait_metadata(kind);
        assert_eq!(builtin_cast_trait_target(kind), metadata.target);
        assert_eq!(
            builtin_cast_requirement_name(kind),
            metadata.requirement_name
        );
        assert_eq!(builtin_cast_trait_fallibility(kind), metadata.fallibility);
    }
}

#[test]
fn is_core_cast_trait_name_matches_exact_trait_spellings_only() {
    assert!(is_core_cast_trait_name("CASTABLE_TO_INT"));
    assert!(is_core_cast_trait_name("TRY_CASTABLE_TO_STRING"));
    assert!(!is_core_cast_trait_name("DISPLAYABLE"));
    assert!(!is_core_cast_trait_name("castable_to_int"));
    assert!(!is_core_cast_trait_name("CASTABLE_TO_COLOR"));
}
