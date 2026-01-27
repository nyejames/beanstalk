//! Property-based tests for Borrow Checker components
//!
//! These tests validate correctness properties for the borrow checker system.
//! Tests are organized by the design document properties they validate.
//!
//! Property 4: Place Hierarchy Conflict Detection
//! For any two places in a hierarchy, the conflict detection should correctly identify
//! their relationship (disjoint, parent-child, or identical) and enforce appropriate access rules.
//! Validates: Requirements 6.2, 6.3

#[cfg(test)]
mod place_registry_tests {
    use crate::compiler::borrow_checker::place_registry::{ConflictType, Place, PlaceRegistry};
    use crate::compiler::string_interning::{InternedString, StringTable};
    use quickcheck::{Arbitrary, Gen, QuickCheck, TestResult};

    // =========================================================================
    // Property 4: Place Hierarchy Conflict Detection
    // For any two places in a hierarchy, the conflict detection should correctly
    // identify their relationship (disjoint, parent-child, or identical) and
    // enforce appropriate access rules.
    // Validates: Requirements 6.2, 6.3
    // =========================================================================

    /// Generate arbitrary interned strings for testing
    #[derive(Clone, Debug)]
    struct ArbitraryInternedString {
        string_table: StringTable,
        id: InternedString,
    }

    impl ArbitraryInternedString {
        fn new(s: &str) -> Self {
            let mut string_table = StringTable::new();
            let id = string_table.intern(s);
            Self { string_table, id }
        }

        fn get_id(&self) -> InternedString {
            self.id
        }
    }

    impl Arbitrary for ArbitraryInternedString {
        fn arbitrary(g: &mut Gen) -> Self {
            let names = ["x", "y", "z", "a", "b", "c", "field1", "field2", "value", "data"];
            let choice = usize::arbitrary(g) % names.len();
            ArbitraryInternedString::new(names[choice])
        }
    }

    /// Generate arbitrary places for testing
    #[derive(Clone, Debug)]
    struct ArbitraryPlace {
        place: Place,
        string_table: StringTable,
    }

    impl ArbitraryPlace {
        fn new(place: Place) -> Self {
            Self {
                place,
                string_table: StringTable::new(),
            }
        }
    }

    impl Arbitrary for ArbitraryPlace {
        fn arbitrary(g: &mut Gen) -> Self {
            let mut string_table = StringTable::new();
            
            let choice = usize::arbitrary(g) % 4;
            let place = match choice {
                0 => {
                    // Variable
                    let var_name = string_table.intern("x");
                    Place::Variable(var_name)
                }
                1 => {
                    // Field access - create a simple hierarchy
                    let base_name = string_table.intern("obj");
                    let field_name = string_table.intern("field");
                    // For testing, we'll use PlaceId(0) as a placeholder base
                    Place::Field {
                        base: crate::compiler::borrow_checker::place_registry::PlaceId(0),
                        field: field_name,
                    }
                }
                2 => {
                    // Index access
                    Place::Index {
                        base: crate::compiler::borrow_checker::place_registry::PlaceId(0),
                        index: crate::compiler::borrow_checker::place_registry::PlaceId(1),
                    }
                }
                _ => Place::Unknown,
            };

            Self { place, string_table }
        }
    }

    /// Generate place hierarchies for testing
    #[derive(Clone, Debug)]
    struct PlaceHierarchy {
        registry: PlaceRegistry,
        string_table: StringTable,
        places: Vec<crate::compiler::borrow_checker::place_registry::PlaceId>,
    }

    impl PlaceHierarchy {
        fn new() -> Self {
            Self {
                registry: PlaceRegistry::new(),
                string_table: StringTable::new(),
                places: Vec::new(),
            }
        }

        fn add_variable(&mut self, name: &str) -> crate::compiler::borrow_checker::place_registry::PlaceId {
            let interned_name = self.string_table.intern(name);
            let place = Place::Variable(interned_name);
            let id = self.registry.register_place(place);
            self.places.push(id);
            id
        }

        fn add_field(&mut self, base_id: crate::compiler::borrow_checker::place_registry::PlaceId, field_name: &str) -> crate::compiler::borrow_checker::place_registry::PlaceId {
            let interned_field = self.string_table.intern(field_name);
            let place = Place::Field {
                base: base_id,
                field: interned_field,
            };
            let id = self.registry.register_place(place);
            self.places.push(id);
            id
        }
    }

    impl Arbitrary for PlaceHierarchy {
        fn arbitrary(g: &mut Gen) -> Self {
            let mut hierarchy = PlaceHierarchy::new();
            
            // Create a few variables
            let var_count = 1 + usize::arbitrary(g) % 3;
            for i in 0..var_count {
                hierarchy.add_variable(&format!("var{}", i));
            }

            // Add some fields to the first variable if it exists
            if !hierarchy.places.is_empty() {
                let base_id = hierarchy.places[0];
                let field_count = usize::arbitrary(g) % 3;
                for i in 0..field_count {
                    hierarchy.add_field(base_id, &format!("field{}", i));
                }
            }

            hierarchy
        }
    }

    // =========================================================================
    // Property: Place Hierarchy Conflict Detection
    // Feature: borrow-checker-implementation, Property 4: Place Hierarchy Conflict Detection
    // Validates: Requirements 6.2, 6.3
    // =========================================================================

    #[test]
    fn property_place_hierarchy_conflict_detection() {
        fn prop(hierarchy: PlaceHierarchy) -> TestResult {
            let registry = &hierarchy.registry;
            
            // Test all pairs of places
            for &place1 in &hierarchy.places {
                for &place2 in &hierarchy.places {
                    let conflict = registry.find_conflicts(place1, place2);
                    
                    // Property: Same place should always have DirectConflict
                    if place1 == place2 {
                        if conflict != ConflictType::DirectConflict {
                            return TestResult::failed();
                        }
                    }
                    
                    // Property: Parent-child relationships should be detected
                    if let Some(parent1) = registry.get_parent(place1) {
                        if parent1 == place2 {
                            if conflict != ConflictType::ParentChild {
                                return TestResult::failed();
                            }
                        }
                    }
                    
                    if let Some(parent2) = registry.get_parent(place2) {
                        if parent2 == place1 {
                            if conflict != ConflictType::ParentChild {
                                return TestResult::failed();
                            }
                        }
                    }
                    
                    // Property: Conflict detection should be symmetric
                    let reverse_conflict = registry.find_conflicts(place2, place1);
                    if conflict != reverse_conflict {
                        return TestResult::failed();
                    }
                }
            }
            
            TestResult::passed()
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(prop as fn(PlaceHierarchy) -> TestResult);
    }

    // =========================================================================
    // Property: Place Registration Uniqueness
    // Feature: borrow-checker-implementation, Property 4: Place Hierarchy Conflict Detection
    // Validates: Requirements 6.2
    // =========================================================================

    #[test]
    fn property_place_registration_uniqueness() {
        fn prop(places: Vec<ArbitraryPlace>) -> TestResult {
            let mut registry = PlaceRegistry::new();
            let mut registered_ids = Vec::new();
            
            for arbitrary_place in places {
                let place = arbitrary_place.place;
                let id1 = registry.register_place(place.clone());
                let id2 = registry.register_place(place);
                
                // Property: Registering the same place twice should return the same ID
                if id1 != id2 {
                    return TestResult::failed();
                }
                
                registered_ids.push(id1);
            }
            
            TestResult::passed()
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(prop as fn(Vec<ArbitraryPlace>) -> TestResult);
    }

    // =========================================================================
    // Property: Parent-Child Relationship Consistency
    // Feature: borrow-checker-implementation, Property 4: Place Hierarchy Conflict Detection
    // Validates: Requirements 6.3
    // =========================================================================

    #[test]
    fn property_parent_child_relationship_consistency() {
        fn prop() -> TestResult {
            let mut registry = PlaceRegistry::new();
            let mut string_table = StringTable::new();
            
            // Create a hierarchy: obj -> obj.field -> obj.field.subfield
            let obj_name = string_table.intern("obj");
            let field_name = string_table.intern("field");
            let subfield_name = string_table.intern("subfield");
            
            let obj_id = registry.register_place(Place::Variable(obj_name));
            let field_id = registry.register_place(Place::Field {
                base: obj_id,
                field: field_name,
            });
            let subfield_id = registry.register_place(Place::Field {
                base: field_id,
                field: subfield_name,
            });
            
            // Property: Parent-child relationships should be consistent
            if registry.get_parent(field_id) != Some(obj_id) {
                return TestResult::failed();
            }
            
            if registry.get_parent(subfield_id) != Some(field_id) {
                return TestResult::failed();
            }
            
            if registry.get_parent(obj_id) != None {
                return TestResult::failed();
            }
            
            // Property: Children should be tracked correctly
            let obj_children = registry.get_children(obj_id);
            if !obj_children.contains(&field_id) {
                return TestResult::failed();
            }
            
            let field_children = registry.get_children(field_id);
            if !field_children.contains(&subfield_id) {
                return TestResult::failed();
            }
            
            // Property: Conflict detection should work for hierarchies
            let obj_field_conflict = registry.find_conflicts(obj_id, field_id);
            if obj_field_conflict != ConflictType::ParentChild {
                return TestResult::failed();
            }
            
            let obj_subfield_conflict = registry.find_conflicts(obj_id, subfield_id);
            if obj_subfield_conflict != ConflictType::ParentChild {
                return TestResult::failed();
            }
            
            let field_subfield_conflict = registry.find_conflicts(field_id, subfield_id);
            if field_subfield_conflict != ConflictType::ParentChild {
                return TestResult::failed();
            }
            
            TestResult::passed()
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(prop as fn() -> TestResult);
    }

    // =========================================================================
    // Property: Disjoint Field Access Independence
    // Feature: borrow-checker-implementation, Property 4: Place Hierarchy Conflict Detection
    // Validates: Requirements 6.2, 6.3
    // =========================================================================

    #[test]
    fn property_disjoint_field_access_independence() {
        fn prop() -> TestResult {
            let mut registry = PlaceRegistry::new();
            let mut string_table = StringTable::new();
            
            // Create disjoint fields: obj.field1 and obj.field2
            let obj_name = string_table.intern("obj");
            let field1_name = string_table.intern("field1");
            let field2_name = string_table.intern("field2");
            
            let obj_id = registry.register_place(Place::Variable(obj_name));
            let field1_id = registry.register_place(Place::Field {
                base: obj_id,
                field: field1_name,
            });
            let field2_id = registry.register_place(Place::Field {
                base: obj_id,
                field: field2_name,
            });
            
            // Property: Disjoint fields should not conflict
            let conflict = registry.find_conflicts(field1_id, field2_id);
            if conflict != ConflictType::Disjoint {
                return TestResult::failed();
            }
            
            // Property: Both fields should conflict with their parent
            let field1_obj_conflict = registry.find_conflicts(field1_id, obj_id);
            if field1_obj_conflict != ConflictType::ParentChild {
                return TestResult::failed();
            }
            
            let field2_obj_conflict = registry.find_conflicts(field2_id, obj_id);
            if field2_obj_conflict != ConflictType::ParentChild {
                return TestResult::failed();
            }
            
            TestResult::passed()
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(prop as fn() -> TestResult);
    }

    // =========================================================================
    // Unit Tests for Edge Cases
    // =========================================================================

    #[test]
    fn test_empty_registry() {
        let registry = PlaceRegistry::new();
        assert_eq!(registry.len(), 0);
        assert!(registry.is_empty());
    }

    #[test]
    fn test_unknown_place_handling() {
        let mut registry = PlaceRegistry::new();
        
        let unknown_id = registry.register_place(Place::Unknown);
        let unknown_id2 = registry.register_place(Place::Unknown);
        
        // Unknown places should be treated as the same
        assert_eq!(unknown_id, unknown_id2);
        
        // Unknown places should not conflict with themselves
        let conflict = registry.find_conflicts(unknown_id, unknown_id2);
        assert_eq!(conflict, ConflictType::DirectConflict);
    }

    #[test]
    fn test_complex_hierarchy() {
        let mut registry = PlaceRegistry::new();
        let mut string_table = StringTable::new();
        
        // Create: person.address.street and person.name
        let person_name = string_table.intern("person");
        let address_name = string_table.intern("address");
        let street_name = string_table.intern("street");
        let name_name = string_table.intern("name");
        
        let person_id = registry.register_place(Place::Variable(person_name));
        let address_id = registry.register_place(Place::Field {
            base: person_id,
            field: address_name,
        });
        let street_id = registry.register_place(Place::Field {
            base: address_id,
            field: street_name,
        });
        let name_id = registry.register_place(Place::Field {
            base: person_id,
            field: name_name,
        });
        
        // Test various conflict relationships
        assert_eq!(registry.find_conflicts(person_id, address_id), ConflictType::ParentChild);
        assert_eq!(registry.find_conflicts(person_id, street_id), ConflictType::ParentChild);
        assert_eq!(registry.find_conflicts(person_id, name_id), ConflictType::ParentChild);
        assert_eq!(registry.find_conflicts(address_id, street_id), ConflictType::ParentChild);
        assert_eq!(registry.find_conflicts(address_id, name_id), ConflictType::Disjoint);
        assert_eq!(registry.find_conflicts(street_id, name_id), ConflictType::NoConflict);
    }
}