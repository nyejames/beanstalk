//! # Place Registry
//!
//! Manages the universe of all places that can be borrowed, providing unique identifiers
//! and hierarchical relationships for precise conflict detection.
//!
//! A place represents a memory location that can be borrowed - variables, struct fields,
//! array elements, etc. The registry tracks parent-child relationships to enable
//! field-level precision in borrow checking.

use crate::compiler::hir::nodes::{HirExpr, HirPlace as HirExpressionPlace};
use crate::compiler::string_interning::InternedString;
use std::collections::HashMap;

/// Unique identifier for a place in the registry
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PlaceId(pub usize);

/// A place represents a memory location that can be borrowed
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Place {
    /// A variable binding
    Variable(InternedString),

    /// Field access on a base place
    Field {
        base: PlaceId,
        field: InternedString,
    },

    /// Array/collection index access (for statically known indices)
    Index { base: PlaceId, index: PlaceId },

    /// Unknown place for dynamic indices or complex expressions
    Unknown,
}

/// Type of conflict between two places
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConflictType {
    /// No conflict - places are completely independent
    NoConflict,

    /// Direct conflict - same place
    DirectConflict,

    /// Parent-child relationship - one place contains the other
    ParentChild,

    /// Disjoint fields of the same struct
    Disjoint,
}

/// Registry that manages all places and their relationships
#[derive(Clone)]
pub struct PlaceRegistry {
    /// All registered places
    places: Vec<Place>,

    /// Reverse lookup: Place -> PlaceId
    place_map: HashMap<Place, PlaceId>,

    /// Parent-child relationships: Parent -> Children
    parent_child: HashMap<PlaceId, Vec<PlaceId>>,

    /// Child-parent relationships: Child -> Parent
    child_parent: HashMap<PlaceId, PlaceId>,
}

impl PlaceRegistry {
    /// Create a new empty place registry
    pub fn new() -> Self {
        Self {
            places: Vec::new(),
            place_map: HashMap::new(),
            parent_child: HashMap::new(),
            child_parent: HashMap::new(),
        }
    }

    /// Register a place and return its unique ID
    /// If the place already exists, returns the existing ID
    pub fn register_place(&mut self, place: Place) -> PlaceId {
        if let Some(&existing_id) = self.place_map.get(&place) {
            return existing_id;
        }

        let id = PlaceId(self.places.len());
        self.places.push(place.clone());
        self.place_map.insert(place.clone(), id);

        // Establish parent-child relationships
        match &place {
            Place::Field { base, .. } | Place::Index { base, .. } => {
                self.parent_child.entry(*base).or_default().push(id);
                self.child_parent.insert(id, *base);
            }
            Place::Variable(_) | Place::Unknown => {
                // No parent relationship
            }
        }

        id
    }

    /// Get the place for a given ID
    pub fn get_place(&self, id: PlaceId) -> Option<&Place> {
        self.places.get(id.0)
    }

    /// Iterate over all registered places
    pub fn iter(&self) -> impl Iterator<Item = (PlaceId, &Place)> {
        self.places
            .iter()
            .enumerate()
            .map(|(idx, place)| (PlaceId(idx), place))
    }

    /// Find the type of conflict between two places
    pub fn find_conflicts(&self, place1: PlaceId, place2: PlaceId) -> ConflictType {
        if place1 == place2 {
            return ConflictType::DirectConflict;
        }

        // Check if one is an ancestor of the other
        if self.is_ancestor(place1, place2) || self.is_ancestor(place2, place1) {
            return ConflictType::ParentChild;
        }

        // Check if they share a common parent (disjoint fields)
        if let (Some(parent1), Some(parent2)) = (
            self.child_parent.get(&place1),
            self.child_parent.get(&place2),
        ) {
            if parent1 == parent2 {
                return ConflictType::Disjoint;
            }
        }

        ConflictType::NoConflict
    }

    /// Get all children of a place
    pub fn get_children(&self, place: PlaceId) -> &[PlaceId] {
        self.parent_child
            .get(&place)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    /// Get the parent of a place
    pub fn get_parent(&self, place: PlaceId) -> Option<PlaceId> {
        self.child_parent.get(&place).copied()
    }

    /// Check if place1 is an ancestor of place2
    fn is_ancestor(&self, place1: PlaceId, place2: PlaceId) -> bool {
        let mut current = place2;
        while let Some(parent) = self.child_parent.get(&current) {
            if *parent == place1 {
                return true;
            }
            current = *parent;
        }
        false
    }

    /// Convert a HIR expression to a place (if possible)
    pub fn expr_to_place(&mut self, expr: &HirExpr) -> Option<PlaceId> {
        use crate::compiler::hir::nodes::HirExprKind;

        match &expr.kind {
            HirExprKind::Load(hir_place) | HirExprKind::Move(hir_place) => {
                Some(self.hir_place_to_place(hir_place))
            }
            HirExprKind::Field { base, field } => Some(self.register_field(*base, *field)),
            HirExprKind::Call { .. }
            | HirExprKind::MethodCall { .. }
            | HirExprKind::StructConstruct { .. }
            | HirExprKind::Collection(_)
            | HirExprKind::Range { .. } => Some(self.register_place(Place::Unknown)),
            _ => None,
        }
    }

    /// Convert a HIR place to our internal place representation
    pub fn hir_place_to_place(&mut self, hir_place: &HirExpressionPlace) -> PlaceId {
        match hir_place {
            HirExpressionPlace::Var(name) => self.register_place(Place::Variable(*name)),
            HirExpressionPlace::Field { base, field } => {
                let base_id = self.hir_place_to_place(base);
                let field_place = Place::Field {
                    base: base_id,
                    field: *field,
                };
                self.register_place(field_place)
            }
            HirExpressionPlace::Index { base, .. } => {
                let base_id = self.hir_place_to_place(base);
                let unknown_index = self.register_place(Place::Unknown);
                self.register_place(Place::Index {
                    base: base_id,
                    index: unknown_index,
                })
            }
        }
    }

    fn register_field(&mut self, base: InternedString, field: InternedString) -> PlaceId {
        let base_place = self.register_place(Place::Variable(base));
        let field_place = Place::Field {
            base: base_place,
            field,
        };

        self.register_place(field_place)
    }

    /// Get the number of registered places
    pub fn len(&self) -> usize {
        self.places.len()
    }

    /// Check if the registry is empty
    pub fn is_empty(&self) -> bool {
        self.places.is_empty()
    }
}

impl Default for PlaceRegistry {
    fn default() -> Self {
        Self::new()
    }
}
