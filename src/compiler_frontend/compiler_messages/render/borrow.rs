//! Borrow diagnostic prose.
//!
//! WHAT: renders borrow-access facts into user-facing conflict and ownership messages.
//! WHY: borrow payloads are structured facts; this module is the render-boundary owner
//! for turning those facts into stable text.

use super::*;

pub(crate) fn diagnostic_place_name(place: &DiagnosticPlace, string_table: &StringTable) -> String {
    match place {
        DiagnosticPlace::Local(name) | DiagnosticPlace::RenderedText(name) => {
            format!("'{}'", string_table.resolve(*name))
        }
        DiagnosticPlace::Path(path) => format!("'{}'", path.to_portable_string(string_table)),
        DiagnosticPlace::Unknown => "this value".to_string(),
    }
}

pub(crate) fn borrow_access_name(access: BorrowAccessKind) -> &'static str {
    match access {
        BorrowAccessKind::Shared => "shared",
        BorrowAccessKind::Mutable => "mutable",
        BorrowAccessKind::Move => "move",
    }
}

pub(crate) fn borrow_conflict_message(
    place: &DiagnosticPlace,
    existing_access: BorrowAccessKind,
    requested_access: BorrowAccessKind,
    string_table: &StringTable,
) -> String {
    format!(
        "Cannot access {}: existing {} access conflicts with requested {} access.",
        diagnostic_place_name(place, string_table),
        borrow_access_name(existing_access),
        borrow_access_name(requested_access)
    )
}

pub(crate) fn multiple_mutable_borrows_message(
    place: &DiagnosticPlace,
    string_table: &StringTable,
) -> String {
    format!(
        "Cannot mutably access {} because it is already mutably active.",
        diagnostic_place_name(place, string_table)
    )
}

pub(crate) fn shared_mutable_conflict_message(
    place: &DiagnosticPlace,
    existing_access: BorrowAccessKind,
    requested_access: BorrowAccessKind,
    conflicting_place: Option<&DiagnosticPlace>,
    string_table: &StringTable,
) -> String {
    let place_name = diagnostic_place_name(place, string_table);
    let conflicting_name = conflicting_place
        .map(|place| diagnostic_place_name(place, string_table))
        .unwrap_or_else(|| place_name.clone());

    match (existing_access, requested_access) {
        (BorrowAccessKind::Mutable, BorrowAccessKind::Shared) => format!(
            "Cannot read {place_name} as shared while mutable alias {conflicting_name} is still active."
        ),
        (BorrowAccessKind::Shared, BorrowAccessKind::Mutable) => format!(
            "Cannot mutably access {place_name} while shared access to {conflicting_name} is still active."
        ),
        (BorrowAccessKind::Move, _) | (_, BorrowAccessKind::Move) => format!(
            "Cannot access {place_name} because it conflicts with a possible ownership move of {conflicting_name}."
        ),
        _ => format!(
            "Cannot access {place_name}: existing {} access conflicts with requested {} access.",
            borrow_access_name(existing_access),
            borrow_access_name(requested_access)
        ),
    }
}

pub(crate) fn use_after_possible_move_message(
    place: &DiagnosticPlace,
    string_table: &StringTable,
) -> String {
    format!(
        "Cannot use {} because it may have been moved or left its valid scope.",
        diagnostic_place_name(place, string_table)
    )
}

pub(crate) fn move_while_borrowed_message(
    place: &DiagnosticPlace,
    existing_access: BorrowAccessKind,
    string_table: &StringTable,
) -> String {
    format!(
        "Cannot move {} while it has an active {} borrow.",
        diagnostic_place_name(place, string_table),
        borrow_access_name(existing_access)
    )
}

pub(crate) fn whole_object_borrow_conflict_message(
    whole_place: &DiagnosticPlace,
    part_place: &DiagnosticPlace,
    string_table: &StringTable,
) -> String {
    format!(
        "Cannot borrow whole object {} while part {} is already borrowed.",
        diagnostic_place_name(whole_place, string_table),
        diagnostic_place_name(part_place, string_table)
    )
}

pub(crate) fn invalid_mutable_access_message(
    place: &DiagnosticPlace,
    reason: InvalidMutableAccessReason,
    conflicting_place: Option<&DiagnosticPlace>,
    string_table: &StringTable,
) -> String {
    let place_name = diagnostic_place_name(place, string_table);

    match reason {
        InvalidMutableAccessReason::ImmutablePlace => {
            format!("Cannot mutably access immutable local {place_name}.")
        }
        InvalidMutableAccessReason::OverlappingAccess => {
            format!(
                "Cannot mutably access {place_name} due to overlapping access in the same evaluation sequence."
            )
        }
        InvalidMutableAccessReason::AliasedValueRequiresExclusiveAccess => {
            let conflicting_name = conflicting_place
                .map(|place| diagnostic_place_name(place, string_table))
                .unwrap_or_else(|| "another live alias".to_string());
            format!(
                "Cannot mutably access {place_name} because {conflicting_name} may alias the same value."
            )
        }
    }
}

pub(crate) fn invalid_access_after_possible_ownership_transfer_message(
    place: &DiagnosticPlace,
    string_table: &StringTable,
) -> String {
    format!(
        "Inconsistent ownership outcome for {} across control-flow paths.",
        diagnostic_place_name(place, string_table)
    )
}

pub(crate) fn use_of_uninitialized_local_message(
    place: &DiagnosticPlace,
    string_table: &StringTable,
) -> String {
    format!(
        "Use of {} before initialization or after scope end.",
        diagnostic_place_name(place, string_table)
    )
}
