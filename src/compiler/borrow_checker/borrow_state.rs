//! # Borrow State Management
//!
//! Tracks the current borrowing state at each program point, including active borrows,
//! their types, and move states. This module provides the core data structures for
//! dataflow analysis and conflict detection.

pub(crate) use super::control_flow::ProgramPoint;
use crate::compiler::parsers::tokenizer::tokens::TextLocation;
use crate::compiler::string_interning::InternedString;
use std::collections::{HashMap, HashSet};

use super::place_registry::PlaceId;

/// Type of borrow or access
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BorrowKind {
    /// Shared/immutable reference (default)
    Shared,

    /// Mutable/exclusive reference
    Mutable,

    /// Candidate for ownership transfer (determined by last-use analysis)
    CandidateMove,

    /// Confirmed ownership transfer
    Move,
}

/// A borrow with its context information
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Borrow {
    /// The place being borrowed
    pub place: PlaceId,

    /// Type of borrow
    pub kind: BorrowKind,

    /// Source location where the borrow was created
    pub location: TextLocation,

    /// Optional last use location (for move analysis)
    pub last_use: Option<ProgramPoint>,
}

/// Represents a borrow conflict for error reporting
#[derive(Debug, Clone)]
pub struct BorrowConflict {
    pub conflict_type: ConflictType,
    pub existing_borrow: Borrow,
    pub attempted_access: AccessAttempt,
    pub suggestion: Option<String>,
}

/// Type of conflict between borrows
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConflictType {
    /// Multiple mutable borrows of the same place
    MultipleMutableBorrows,

    /// Shared and mutable borrow conflict
    SharedMutableConflict,

    /// Use after move
    UseAfterMove,

    /// Move while borrowed
    MoveWhileBorrowed,

    /// Whole object borrow while part is borrowed
    WholeObjectBorrow,
}

/// An attempted access that may conflict with existing borrows
#[derive(Debug, Clone)]
pub struct AccessAttempt {
    pub place: PlaceId,
    pub kind: AccessKind,
    pub location: TextLocation,
}

/// Type of access being attempted
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccessKind {
    /// Read access (shared borrow)
    Read,

    /// Write access (mutable borrow)
    Write,

    /// Ownership transfer (move)
    Move,
}

/// The complete borrow state at a program point
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BorrowState {
    /// Active borrows by place
    active_borrows: HashMap<PlaceId, Vec<Borrow>>,

    /// Places that have been moved
    moved_places: HashSet<PlaceId>,

    /// Last use points for each place
    last_uses: HashMap<PlaceId, ProgramPoint>,

    /// Variable names for error reporting
    variable_names: HashMap<PlaceId, InternedString>,
}

impl BorrowState {
    /// Create a new empty borrow state
    pub fn new() -> Self {
        Self {
            active_borrows: HashMap::new(),
            moved_places: HashSet::new(),
            last_uses: HashMap::new(),
            variable_names: HashMap::new(),
        }
    }

    /// Add a new borrow, checking for conflicts
    pub fn add_borrow(&mut self, borrow: Borrow) -> Result<(), BorrowConflict> {
        let place = borrow.place;

        // Check for conflicts with existing borrows
        if let Some(existing_borrows) = self.active_borrows.get(&place) {
            for existing in existing_borrows {
                if let Some(conflict) = self.check_borrow_conflict(existing, &borrow) {
                    return Err(conflict);
                }
            }
        }

        // Check if the place has been moved
        if self.moved_places.contains(&place) {
            return Err(BorrowConflict {
                conflict_type: ConflictType::UseAfterMove,
                existing_borrow: borrow.clone(), // Placeholder
                attempted_access: AccessAttempt {
                    place,
                    kind: match borrow.kind {
                        BorrowKind::Shared => AccessKind::Read,
                        BorrowKind::Mutable => AccessKind::Write,
                        BorrowKind::CandidateMove | BorrowKind::Move => AccessKind::Move,
                    },
                    location: borrow.location.clone(),
                },
                suggestion: Some(
                    "Consider using a reference instead of moving the value".to_string(),
                ),
            });
        }

        // Add the borrow
        self.active_borrows.entry(place).or_default().push(borrow);
        Ok(())
    }

    /// Remove a borrow at a specific location
    pub fn remove_borrow(&mut self, place: PlaceId, location: TextLocation) {
        if let Some(borrows) = self.active_borrows.get_mut(&place) {
            borrows.retain(|b| b.location != location);
            if borrows.is_empty() {
                self.active_borrows.remove(&place);
            }
        }
    }

    /// Mark a place as moved
    pub fn mark_moved(&mut self, place: PlaceId, _location: TextLocation) {
        self.moved_places.insert(place);
        // Remove any active borrows since the value is moved
        self.active_borrows.remove(&place);
    }

    /// Check if a place has been moved
    pub fn is_moved(&self, place: PlaceId) -> bool {
        self.moved_places.contains(&place)
    }

    /// Get active borrows for a place
    pub fn get_active_borrows(&self, place: PlaceId) -> &[Borrow] {
        self.active_borrows
            .get(&place)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    /// Check if an access would conflict with existing borrows
    pub fn check_access(
        &self,
        place: PlaceId,
        access_kind: AccessKind,
        location: TextLocation,
    ) -> Result<(), BorrowConflict> {
        // Check if moved
        if self.moved_places.contains(&place) && access_kind != AccessKind::Move {
            return Err(BorrowConflict {
                conflict_type: ConflictType::UseAfterMove,
                existing_borrow: Borrow {
                    place,
                    kind: BorrowKind::Move,
                    location: location.clone(),
                    last_use: None,
                },
                attempted_access: AccessAttempt {
                    place,
                    kind: access_kind,
                    location,
                },
                suggestion: Some("Value has been moved".to_string()),
            });
        }

        // Check conflicts with active borrows
        if let Some(borrows) = self.active_borrows.get(&place) {
            for borrow in borrows {
                match (borrow.kind, access_kind) {
                    // Shared borrows allow more shared access
                    (BorrowKind::Shared, AccessKind::Read) => continue,

                    // Mutable borrows are exclusive
                    (BorrowKind::Mutable, _) => {
                        return Err(BorrowConflict {
                            conflict_type: ConflictType::SharedMutableConflict,
                            existing_borrow: borrow.clone(),
                            attempted_access: AccessAttempt {
                                place,
                                kind: access_kind,
                                location,
                            },
                            suggestion: Some("Mutable borrow is exclusive".to_string()),
                        });
                    }

                    // Any existing borrow prevents mutable access
                    (_, AccessKind::Write) => {
                        return Err(BorrowConflict {
                            conflict_type: ConflictType::SharedMutableConflict,
                            existing_borrow: borrow.clone(),
                            attempted_access: AccessAttempt {
                                place,
                                kind: access_kind,
                                location,
                            },
                            suggestion: Some(
                                "Cannot mutably borrow while other borrows exist".to_string(),
                            ),
                        });
                    }

                    // Any existing borrow prevents moves
                    (_, AccessKind::Move) => {
                        return Err(BorrowConflict {
                            conflict_type: ConflictType::MoveWhileBorrowed,
                            existing_borrow: borrow.clone(),
                            attempted_access: AccessAttempt {
                                place,
                                kind: access_kind,
                                location,
                            },
                            suggestion: Some("Cannot move while borrowed".to_string()),
                        });
                    }

                    _ => continue,
                }
            }
        }

        Ok(())
    }

    /// Set the last use point for a place
    pub fn set_last_use(&mut self, place: PlaceId, point: ProgramPoint) {
        self.last_uses.insert(place, point);
    }

    /// Get the last use point for a place
    pub fn get_last_use(&self, place: PlaceId) -> Option<ProgramPoint> {
        self.last_uses.get(&place).copied()
    }

    /// Set variable name for error reporting
    pub fn set_variable_name(&mut self, place: PlaceId, name: InternedString) {
        self.variable_names.insert(place, name);
    }

    /// Get variable name for error reporting
    pub fn get_variable_name(&self, place: PlaceId) -> Option<InternedString> {
        self.variable_names.get(&place).copied()
    }

    /// Check for conflicts between two borrows
    fn check_borrow_conflict(&self, existing: &Borrow, new: &Borrow) -> Option<BorrowConflict> {
        match (existing.kind, new.kind) {
            // Multiple shared borrows are allowed
            (BorrowKind::Shared, BorrowKind::Shared) => None,

            // Multiple mutable borrows are not allowed
            (BorrowKind::Mutable, BorrowKind::Mutable) => Some(BorrowConflict {
                conflict_type: ConflictType::MultipleMutableBorrows,
                existing_borrow: existing.clone(),
                attempted_access: AccessAttempt {
                    place: new.place,
                    kind: AccessKind::Write,
                    location: new.location.clone(),
                },
                suggestion: Some("Only one mutable borrow allowed at a time".to_string()),
            }),

            // Shared and mutable borrows conflict
            (BorrowKind::Shared, BorrowKind::Mutable)
            | (BorrowKind::Mutable, BorrowKind::Shared) => Some(BorrowConflict {
                conflict_type: ConflictType::SharedMutableConflict,
                existing_borrow: existing.clone(),
                attempted_access: AccessAttempt {
                    place: new.place,
                    kind: match new.kind {
                        BorrowKind::Mutable => AccessKind::Write,
                        _ => AccessKind::Read,
                    },
                    location: new.location.clone(),
                },
                suggestion: Some("Shared and mutable borrows cannot coexist".to_string()),
            }),

            // Moves conflict with any existing borrow
            (_, BorrowKind::Move) | (_, BorrowKind::CandidateMove) => Some(BorrowConflict {
                conflict_type: ConflictType::MoveWhileBorrowed,
                existing_borrow: existing.clone(),
                attempted_access: AccessAttempt {
                    place: new.place,
                    kind: AccessKind::Move,
                    location: new.location.clone(),
                },
                suggestion: Some("Cannot move while borrowed".to_string()),
            }),

            (BorrowKind::Move, _) | (BorrowKind::CandidateMove, _) => Some(BorrowConflict {
                conflict_type: ConflictType::UseAfterMove,
                existing_borrow: existing.clone(),
                attempted_access: AccessAttempt {
                    place: new.place,
                    kind: match new.kind {
                        BorrowKind::Shared => AccessKind::Read,
                        BorrowKind::Mutable => AccessKind::Write,
                        _ => AccessKind::Move,
                    },
                    location: new.location.clone(),
                },
                suggestion: Some("Cannot use after move".to_string()),
            }),
        }
    }

    /// Merge two borrow states (for control flow joins)
    pub fn merge(&self, other: &BorrowState) -> BorrowState {
        let mut merged = BorrowState::new();

        // Conservative merge: include borrows that exist in both states
        for (place, borrows) in &self.active_borrows {
            if let Some(other_borrows) = other.active_borrows.get(place) {
                // Only keep borrows that exist in both states
                let mut common_borrows = Vec::new();
                for borrow in borrows {
                    if other_borrows.iter().any(|b| b.location == borrow.location) {
                        common_borrows.push(borrow.clone());
                    }
                }
                if !common_borrows.is_empty() {
                    merged.active_borrows.insert(*place, common_borrows);
                }
            }
        }

        // Conservative merge: a place is moved only if moved in both states
        for place in &self.moved_places {
            if other.moved_places.contains(place) {
                merged.moved_places.insert(*place);
            }
        }

        // Merge last uses (take the later one)
        for (place, point) in &self.last_uses {
            if let Some(other_point) = other.last_uses.get(place) {
                merged.last_uses.insert(*place, (*point).max(*other_point));
            } else {
                merged.last_uses.insert(*place, *point);
            }
        }

        // Merge variable names
        merged.variable_names.extend(&self.variable_names);
        merged.variable_names.extend(&other.variable_names);

        merged
    }

    /// Records an access and updates borrow state, returning a conflict if one is found.
    pub fn record_access(
        &mut self,
        place: PlaceId,
        access_kind: AccessKind,
        location: TextLocation,
    ) -> Result<(), BorrowConflict> {
        self.check_access(place, access_kind, location.clone())?;

        match access_kind {
            AccessKind::Read => self.add_borrow(Borrow {
                place,
                kind: BorrowKind::Shared,
                location,
                last_use: None,
            })?,
            AccessKind::Write => self.add_borrow(Borrow {
                place,
                kind: BorrowKind::Mutable,
                location,
                last_use: None,
            })?,
            AccessKind::Move => self.mark_moved(place, location),
        }

        Ok(())
    }

    /// Ends all active borrows for a place when it reaches a proven last use.
    pub fn end_borrows_for_place(&mut self, place: PlaceId) {
        self.active_borrows.remove(&place);
    }
}

impl Default for BorrowState {
    fn default() -> Self {
        Self::new()
    }
}
