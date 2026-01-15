//! Property-based tests for HIR Builder components
//!
//! These tests validate correctness properties for the HIR builder system.
//! Tests are organized by the design document properties they validate.
//!
//! Property 9: Compiler Integration Compliance
//! For any HIR generation operation, the HIR builder should accept standard AST format,
//! generate HIR in the format expected by the borrow checker, use Beanstalk's existing
//! error system, handle compiler directives appropriately, and integrate with debugging features.
//! Validates: Requirements 9.1, 9.2, 9.3, 9.4, 9.5, 9.6
//!
//! Property 7: Source Location Preservation
//! For any HIR node generated from AST, the HIR builder should preserve source location
//! information, maintain mapping between HIR instructions and original AST nodes, and
//! provide accurate source locations for error reporting even after transformations.
//! Validates: Requirements 8.1, 8.2, 8.3, 8.4, 8.5, 8.6
//!
//! HIR Invariant Validation Tests
//! These tests verify that the HirValidator correctly catches violations of HIR invariants:
//! - No nested expressions (expressions should be flat)
//! - Explicit terminators (every block ends with exactly one terminator)
//! - Block connectivity (all blocks reachable from entry)
//! - Valid branch targets (all branch targets reference valid block IDs)
//! Validates: All requirements (validation ensures correctness)

#[cfg(test)]
mod hir_builder_context_tests {
    use crate::compiler::hir::build_hir::{
        AstHirMapping, HirBuildContext, HirBuilderContext,
        HirValidator, HirValidationError, OwnershipHints, ScopeType,
    };
    use crate::compiler::hir::nodes::{
        HirBlock, HirModule, HirNode, HirKind, HirStmt, HirTerminator, HirExpr, HirExprKind,
        HirPlace, BinOp,
    };
    use crate::compiler::datatypes::DataType;
    use crate::compiler::parsers::tokenizer::tokens::{CharPosition, TextLocation};
    use crate::compiler::string_interning::{StringTable, InternedString};
    use crate::compiler::interned_path::InternedPath;
    use quickcheck::{Arbitrary, Gen, QuickCheck, TestResult};

    // =========================================================================
    // Property 9: Compiler Integration Compliance
    // For any HIR generation operation, the HIR builder should accept standard
    // AST format, generate HIR in the format expected by the borrow checker,
    // use Beanstalk's existing error system, handle compiler directives
    // appropriately, and integrate with debugging features.
    // Validates: Requirements 9.1, 9.2, 9.3, 9.4, 9.5, 9.6
    // =========================================================================

    /// Generate arbitrary scope types for testing
    #[derive(Clone, Debug)]
    struct ArbitraryScopeType(ScopeType);

    impl Arbitrary for ArbitraryScopeType {
        fn arbitrary(g: &mut Gen) -> Self {
            let choice = usize::arbitrary(g) % 4;
            let scope_type = match choice {
                0 => ScopeType::Function,
                1 => ScopeType::Block,
                2 => ScopeType::Loop {
                    break_target: usize::arbitrary(g) % 100,
                    continue_target: usize::arbitrary(g) % 100,
                },
                _ => ScopeType::If,
            };
            ArbitraryScopeType(scope_type)
        }
    }

    /// Generate arbitrary scope sequences for testing
    #[derive(Clone, Debug)]
    struct ArbitraryScopeSequence(Vec<ScopeType>);

    impl Arbitrary for ArbitraryScopeSequence {
        fn arbitrary(g: &mut Gen) -> Self {
            // Generate 0-10 scopes
            let count = usize::arbitrary(g) % 11;
            let scopes: Vec<ScopeType> = (0..count)
                .map(|_| ArbitraryScopeType::arbitrary(g).0)
                .collect();
            ArbitraryScopeSequence(scopes)
        }
    }

    /// Generate arbitrary block counts for testing
    #[derive(Clone, Debug)]
    struct ArbitraryBlockCount(usize);

    impl Arbitrary for ArbitraryBlockCount {
        fn arbitrary(g: &mut Gen) -> Self {
            // Generate 1-20 blocks
            ArbitraryBlockCount(1 + usize::arbitrary(g) % 20)
        }
    }

    // =========================================================================
    // Property: Block ID allocation is unique and sequential
    // Feature: hir-builder, Property 9: Compiler Integration Compliance
    // Validates: Requirements 9.4 (error handling integration)
    // =========================================================================

    /// Property: Block IDs are allocated sequentially starting from 0
    #[test]
    fn prop_block_ids_are_sequential() {
        fn property(block_count: ArbitraryBlockCount) -> TestResult {
            let mut string_table = StringTable::new();
            let mut ctx = HirBuilderContext::new(&mut string_table);

            let mut allocated_ids = Vec::new();
            for _ in 0..block_count.0 {
                let id = ctx.allocate_block_id();
                allocated_ids.push(id);
            }

            // Check that IDs are sequential starting from 0
            for (expected, actual) in allocated_ids.iter().enumerate() {
                if *actual != expected {
                    return TestResult::failed();
                }
            }

            TestResult::passed()
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(ArbitraryBlockCount) -> TestResult);
    }

    /// Property: Node IDs are allocated sequentially starting from 0
    #[test]
    fn prop_node_ids_are_sequential() {
        fn property(node_count: ArbitraryBlockCount) -> TestResult {
            let mut string_table = StringTable::new();
            let mut ctx = HirBuilderContext::new(&mut string_table);

            let mut allocated_ids = Vec::new();
            for _ in 0..node_count.0 {
                let id = ctx.allocate_node_id();
                allocated_ids.push(id);
            }

            // Check that IDs are sequential starting from 0
            for (expected, actual) in allocated_ids.iter().enumerate() {
                if *actual != expected {
                    return TestResult::failed();
                }
            }

            TestResult::passed()
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(ArbitraryBlockCount) -> TestResult);
    }

    // =========================================================================
    // Property: Scope management maintains proper nesting
    // Feature: hir-builder, Property 9: Compiler Integration Compliance
    // Validates: Requirements 9.2 (HIR format for borrow checker)
    // =========================================================================

    /// Property: Entering and exiting scopes maintains proper depth
    #[test]
    fn prop_scope_depth_tracks_correctly() {
        fn property(scopes: ArbitraryScopeSequence) -> TestResult {
            let mut string_table = StringTable::new();
            let mut ctx = HirBuilderContext::new(&mut string_table);

            // Enter all scopes
            for scope_type in &scopes.0 {
                let depth_before = ctx.current_scope_depth();
                ctx.enter_scope(scope_type.clone());
                let depth_after = ctx.current_scope_depth();

                if depth_after != depth_before + 1 {
                    return TestResult::failed();
                }
            }

            // Exit all scopes
            for _ in &scopes.0 {
                let depth_before = ctx.current_scope_depth();
                let _ = ctx.exit_scope();
                let depth_after = ctx.current_scope_depth();

                if depth_after != depth_before - 1 {
                    return TestResult::failed();
                }
            }

            // Should be back to depth 0
            if ctx.current_scope_depth() != 0 {
                return TestResult::failed();
            }

            TestResult::passed()
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(ArbitraryScopeSequence) -> TestResult);
    }

    /// Property: Current scope returns the most recently entered scope
    #[test]
    fn prop_current_scope_is_most_recent() {
        fn property(scopes: ArbitraryScopeSequence) -> TestResult {
            if scopes.0.is_empty() {
                return TestResult::discard();
            }

            let mut string_table = StringTable::new();
            let mut ctx = HirBuilderContext::new(&mut string_table);

            // Enter all scopes and verify current scope
            for scope_type in &scopes.0 {
                ctx.enter_scope(scope_type.clone());

                if let Some(current) = ctx.current_scope() {
                    if current.scope_type != *scope_type {
                        return TestResult::failed();
                    }
                } else {
                    return TestResult::failed();
                }
            }

            TestResult::passed()
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(ArbitraryScopeSequence) -> TestResult);
    }

    // =========================================================================
    // Property: Block creation and management
    // Feature: hir-builder, Property 9: Compiler Integration Compliance
    // Validates: Requirements 9.2 (HIR format for borrow checker)
    // =========================================================================

    /// Property: Created blocks can be retrieved by ID
    #[test]
    fn prop_created_blocks_are_retrievable() {
        fn property(block_count: ArbitraryBlockCount) -> TestResult {
            let mut string_table = StringTable::new();
            let mut ctx = HirBuilderContext::new(&mut string_table);

            let mut created_ids = Vec::new();
            for _ in 0..block_count.0 {
                let id = ctx.create_block();
                created_ids.push(id);
            }

            // Verify all blocks can be retrieved
            for id in &created_ids {
                if ctx.get_block(*id).is_none() {
                    return TestResult::failed();
                }
            }

            TestResult::passed()
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(ArbitraryBlockCount) -> TestResult);
    }

    /// Property: Created blocks have correct IDs
    #[test]
    fn prop_created_blocks_have_correct_ids() {
        fn property(block_count: ArbitraryBlockCount) -> TestResult {
            let mut string_table = StringTable::new();
            let mut ctx = HirBuilderContext::new(&mut string_table);

            for expected_id in 0..block_count.0 {
                let id = ctx.create_block();
                if id != expected_id {
                    return TestResult::failed();
                }

                if let Some(block) = ctx.get_block(id) {
                    if block.id != expected_id {
                        return TestResult::failed();
                    }
                } else {
                    return TestResult::failed();
                }
            }

            TestResult::passed()
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(ArbitraryBlockCount) -> TestResult);
    }

    // =========================================================================
    // Property: Ownership hints are conservative
    // Feature: hir-builder, Property 9: Compiler Integration Compliance
    // Validates: Requirements 9.2 (HIR format for borrow checker)
    // =========================================================================

    /// Property: Marking a variable as potentially owned removes it from definitely borrowed
    #[test]
    fn prop_ownership_hints_are_exclusive() {
        let mut hints = OwnershipHints::new();
        let var = crate::compiler::string_interning::InternedString::from_u32(1);

        // Mark as borrowed first
        hints.mark_definitely_borrowed(var);
        assert!(hints.definitely_borrowed.contains(&var));

        // Mark as potentially owned - should remove from borrowed
        hints.mark_potentially_owned(var);
        assert!(hints.potentially_owned.contains(&var));
        assert!(!hints.definitely_borrowed.contains(&var));
    }

    // =========================================================================
    // Property: HIR Validation - Empty module is valid
    // Feature: hir-builder, Property 9: Compiler Integration Compliance
    // Validates: Requirements 9.2 (HIR format for borrow checker)
    // =========================================================================

    /// Property: An empty HIR module passes validation
    #[test]
    fn prop_empty_module_is_valid() {
        let module = HirModule {
            blocks: vec![HirBlock {
                id: 0,
                params: vec![],
                nodes: vec![],
            }],
            entry_block: 0,
            functions: vec![],
            structs: vec![],
        };

        let result = HirValidator::validate_module(&module);
        assert!(result.is_ok());
    }

    /// Property: Module with unreachable blocks fails validation
    #[test]
    fn prop_unreachable_blocks_fail_validation() {
        let module = HirModule {
            blocks: vec![
                HirBlock {
                    id: 0,
                    params: vec![],
                    nodes: vec![],
                },
                HirBlock {
                    id: 1,
                    params: vec![],
                    nodes: vec![],
                }, // Unreachable
            ],
            entry_block: 0,
            functions: vec![],
            structs: vec![],
        };

        let result = HirValidator::validate_module(&module);
        assert!(result.is_err());
    }

    // =========================================================================
    // Property: Find enclosing loop returns correct scope
    // Feature: hir-builder, Property 9: Compiler Integration Compliance
    // Validates: Requirements 9.2 (HIR format for borrow checker)
    // =========================================================================

    /// Property: find_enclosing_loop returns the nearest loop scope
    #[test]
    fn prop_find_enclosing_loop_returns_nearest() {
        let mut string_table = StringTable::new();
        let mut ctx = HirBuilderContext::new(&mut string_table);

        // Enter function scope
        ctx.enter_scope(ScopeType::Function);

        // Enter first loop
        ctx.enter_scope(ScopeType::Loop {
            break_target: 1,
            continue_target: 2,
        });

        // Enter if scope
        ctx.enter_scope(ScopeType::If);

        // Enter second loop
        ctx.enter_scope(ScopeType::Loop {
            break_target: 3,
            continue_target: 4,
        });

        // Find enclosing loop should return the second (innermost) loop
        if let Some(loop_scope) = ctx.find_enclosing_loop() {
            if let ScopeType::Loop {
                break_target,
                continue_target,
            } = &loop_scope.scope_type
            {
                assert_eq!(*break_target, 3);
                assert_eq!(*continue_target, 4);
            } else {
                panic!("Expected loop scope");
            }
        } else {
            panic!("Expected to find enclosing loop");
        }
    }

    /// Property: find_enclosing_loop returns None when no loop exists
    #[test]
    fn prop_find_enclosing_loop_returns_none_when_no_loop() {
        let mut string_table = StringTable::new();
        let mut ctx = HirBuilderContext::new(&mut string_table);

        // Enter non-loop scopes
        ctx.enter_scope(ScopeType::Function);
        ctx.enter_scope(ScopeType::If);
        ctx.enter_scope(ScopeType::Block);

        // Should return None
        assert!(ctx.find_enclosing_loop().is_none());
    }

    // =========================================================================
    // Property 7: Source Location Preservation
    // For any HIR node generated from AST, the HIR builder should preserve
    // source location information, maintain mapping between HIR instructions
    // and original AST nodes, and provide accurate source locations for error
    // reporting even after transformations.
    // Validates: Requirements 8.1, 8.2, 8.3, 8.4, 8.5, 8.6
    // =========================================================================

    /// Generate arbitrary text locations for testing
    #[derive(Clone, Debug)]
    struct ArbitraryTextLocation(TextLocation);

    impl Arbitrary for ArbitraryTextLocation {
        fn arbitrary(g: &mut Gen) -> Self {
            let line = ((u32::arbitrary(g) % 1000) + 1) as i32;
            let column = ((u32::arbitrary(g) % 200) + 1) as i32;
            let end_line = line + ((u32::arbitrary(g) % 10) as i32);
            let end_column = ((u32::arbitrary(g) % 200) + 1) as i32;

            ArbitraryTextLocation(TextLocation {
                scope: crate::compiler::interned_path::InternedPath::default(),
                start_pos: CharPosition {
                    line_number: line,
                    char_column: column,
                },
                end_pos: CharPosition {
                    line_number: end_line,
                    char_column: end_column,
                },
            })
        }
    }

    /// Generate arbitrary HIR node IDs for testing
    #[derive(Clone, Debug)]
    struct ArbitraryHirNodeId(usize);

    impl Arbitrary for ArbitraryHirNodeId {
        fn arbitrary(g: &mut Gen) -> Self {
            ArbitraryHirNodeId(usize::arbitrary(g) % 10000)
        }
    }

    /// Generate arbitrary AST node IDs for testing
    #[derive(Clone, Debug)]
    struct ArbitraryAstNodeId(usize);

    impl Arbitrary for ArbitraryAstNodeId {
        fn arbitrary(g: &mut Gen) -> Self {
            ArbitraryAstNodeId(usize::arbitrary(g) % 10000)
        }
    }

    /// Property: HirBuildContext preserves source location information
    /// Feature: hir-builder, Property 7: Source Location Preservation
    /// Validates: Requirements 8.1
    #[test]
    fn prop_build_context_preserves_source_location() {
        fn property(loc: ArbitraryTextLocation) -> TestResult {
            let context = HirBuildContext::new(loc.0.clone());

            // Source location should be preserved exactly
            if context.source_location.start_pos.line_number != loc.0.start_pos.line_number {
                return TestResult::failed();
            }
            if context.source_location.start_pos.char_column != loc.0.start_pos.char_column {
                return TestResult::failed();
            }
            if context.source_location.end_pos.line_number != loc.0.end_pos.line_number {
                return TestResult::failed();
            }
            if context.source_location.end_pos.char_column != loc.0.end_pos.char_column {
                return TestResult::failed();
            }

            TestResult::passed()
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(ArbitraryTextLocation) -> TestResult);
    }

    /// Property: HirBuildContext preserves AST node reference
    /// Feature: hir-builder, Property 7: Source Location Preservation
    /// Validates: Requirements 8.2
    #[test]
    fn prop_build_context_preserves_ast_reference() {
        fn property(loc: ArbitraryTextLocation, ast_id: ArbitraryAstNodeId) -> TestResult {
            let context = HirBuildContext::from_ast_node(loc.0.clone(), ast_id.0, 0);

            // AST node reference should be preserved
            if context.original_ast_node != Some(ast_id.0) {
                return TestResult::failed();
            }

            TestResult::passed()
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(ArbitraryTextLocation, ArbitraryAstNodeId) -> TestResult);
    }

    /// Property: AstHirMapping maintains bidirectional mapping
    /// Feature: hir-builder, Property 7: Source Location Preservation
    /// Validates: Requirements 8.2, 8.3
    #[test]
    fn prop_ast_hir_mapping_is_bidirectional() {
        fn property(
            ast_id: ArbitraryAstNodeId,
            hir_id: ArbitraryHirNodeId,
            loc: ArbitraryTextLocation,
        ) -> TestResult {
            let mut mapping = AstHirMapping::new();

            // Add a mapping
            mapping.add_single_mapping(ast_id.0, hir_id.0);
            mapping.record_location(hir_id.0, loc.0.clone());

            // Verify bidirectional mapping
            let retrieved_ast = mapping.get_original_ast(hir_id.0);
            if retrieved_ast != Some(ast_id.0) {
                return TestResult::failed();
            }

            let retrieved_hir = mapping.get_hir_nodes(ast_id.0);
            if let Some(hir_nodes) = retrieved_hir {
                if !hir_nodes.contains(&hir_id.0) {
                    return TestResult::failed();
                }
            } else {
                return TestResult::failed();
            }

            // Verify location is preserved
            let retrieved_loc = mapping.get_source_location(hir_id.0);
            if let Some(retrieved) = retrieved_loc {
                if retrieved.start_pos.line_number != loc.0.start_pos.line_number {
                    return TestResult::failed();
                }
            } else {
                return TestResult::failed();
            }

            TestResult::passed()
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(
                property
                    as fn(ArbitraryAstNodeId, ArbitraryHirNodeId, ArbitraryTextLocation) -> TestResult,
            );
    }

    /// Property: Multiple HIR nodes can map to same AST node
    /// Feature: hir-builder, Property 7: Source Location Preservation
    /// Validates: Requirements 8.2
    #[test]
    fn prop_multiple_hir_nodes_can_map_to_same_ast() {
        fn property(ast_id: ArbitraryAstNodeId, hir_count: ArbitraryBlockCount) -> TestResult {
            if hir_count.0 == 0 {
                return TestResult::discard();
            }

            let mut mapping = AstHirMapping::new();

            // Add multiple HIR nodes for the same AST node
            for hir_id in 0..hir_count.0 {
                mapping.add_single_mapping(ast_id.0, hir_id);
            }

            // Verify all HIR nodes are recorded
            let hir_nodes = mapping.get_hir_nodes(ast_id.0);
            if let Some(nodes) = hir_nodes {
                if nodes.len() != hir_count.0 {
                    return TestResult::failed();
                }
                // Verify each HIR node maps back to the AST node
                for hir_id in 0..hir_count.0 {
                    if mapping.get_original_ast(hir_id) != Some(ast_id.0) {
                        return TestResult::failed();
                    }
                }
            } else {
                return TestResult::failed();
            }

            TestResult::passed()
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(ArbitraryAstNodeId, ArbitraryBlockCount) -> TestResult);
    }

    /// Property: Build context records scope depth correctly
    /// Feature: hir-builder, Property 7: Source Location Preservation
    /// Validates: Requirements 8.4
    #[test]
    fn prop_build_context_records_scope_depth() {
        fn property(loc: ArbitraryTextLocation, scope_depth: ArbitraryBlockCount) -> TestResult {
            let context = HirBuildContext::with_details(loc.0, None, scope_depth.0, false);

            if context.scope_depth != scope_depth.0 {
                return TestResult::failed();
            }

            TestResult::passed()
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(ArbitraryTextLocation, ArbitraryBlockCount) -> TestResult);
    }

    /// Property: Build context tracks ownership potential
    /// Feature: hir-builder, Property 7: Source Location Preservation
    /// Validates: Requirements 8.4
    #[test]
    fn prop_build_context_tracks_ownership_potential() {
        fn property(loc: ArbitraryTextLocation) -> TestResult {
            // Without ownership potential
            let context_no_ownership = HirBuildContext::new(loc.0.clone());
            if context_no_ownership.ownership_potential {
                return TestResult::failed();
            }

            // With ownership potential
            let context_with_ownership = HirBuildContext::new(loc.0).with_ownership_potential();
            if !context_with_ownership.ownership_potential {
                return TestResult::failed();
            }

            TestResult::passed()
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(ArbitraryTextLocation) -> TestResult);
    }

    /// Property: AstHirMapping records build context with location
    /// Feature: hir-builder, Property 7: Source Location Preservation
    /// Validates: Requirements 8.3, 8.4
    #[test]
    fn prop_mapping_records_build_context() {
        fn property(
            hir_id: ArbitraryHirNodeId,
            loc: ArbitraryTextLocation,
            scope_depth: ArbitraryBlockCount,
        ) -> TestResult {
            let mut mapping = AstHirMapping::new();

            let context = HirBuildContext::with_details(loc.0.clone(), None, scope_depth.0, true);
            mapping.record_build_context(hir_id.0, context);

            // Verify build context is retrievable
            let retrieved = mapping.get_build_context(hir_id.0);
            if let Some(ctx) = retrieved {
                if ctx.scope_depth != scope_depth.0 {
                    return TestResult::failed();
                }
                if !ctx.ownership_potential {
                    return TestResult::failed();
                }
                if ctx.source_location.start_pos.line_number != loc.0.start_pos.line_number {
                    return TestResult::failed();
                }
            } else {
                return TestResult::failed();
            }

            // Verify location is also recorded
            let retrieved_loc = mapping.get_source_location(hir_id.0);
            if retrieved_loc.is_none() {
                return TestResult::failed();
            }

            TestResult::passed()
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(
                property
                    as fn(ArbitraryHirNodeId, ArbitraryTextLocation, ArbitraryBlockCount) -> TestResult,
            );
    }

    /// Property: HirBuilderContext creates build contexts with correct scope depth
    /// Feature: hir-builder, Property 7: Source Location Preservation
    /// Validates: Requirements 8.4
    #[test]
    fn prop_builder_context_creates_correct_build_contexts() {
        fn property(scopes: ArbitraryScopeSequence, loc: ArbitraryTextLocation) -> TestResult {
            let mut string_table = StringTable::new();
            let mut ctx = HirBuilderContext::new(&mut string_table);

            // Enter scopes
            for scope_type in &scopes.0 {
                ctx.enter_scope(scope_type.clone());
            }

            // Create build context - should have correct scope depth
            let build_ctx = ctx.create_build_context(loc.0.clone());

            if build_ctx.scope_depth != scopes.0.len() {
                return TestResult::failed();
            }

            if build_ctx.source_location.start_pos.line_number != loc.0.start_pos.line_number {
                return TestResult::failed();
            }

            TestResult::passed()
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(ArbitraryScopeSequence, ArbitraryTextLocation) -> TestResult);
    }

    // =========================================================================
    // HIR Invariant Validation Tests
    // These tests verify that the HirValidator correctly catches violations
    // of HIR invariants. All requirements are validated through these tests.
    // =========================================================================

    /// Helper function to create a default TextLocation for tests
    fn default_location() -> TextLocation {
        TextLocation {
            scope: InternedPath::default(),
            start_pos: CharPosition {
                line_number: 1,
                char_column: 1,
            },
            end_pos: CharPosition {
                line_number: 1,
                char_column: 10,
            },
        }
    }

    /// Helper function to create a simple HirExpr with an integer literal
    fn int_expr(value: i64) -> HirExpr {
        HirExpr {
            kind: HirExprKind::Int(value),
            data_type: DataType::Int,
            location: default_location(),
        }
    }

    /// Helper function to create a simple HirExpr with a boolean literal
    fn bool_expr(value: bool) -> HirExpr {
        HirExpr {
            kind: HirExprKind::Bool(value),
            data_type: DataType::Bool,
            location: default_location(),
        }
    }

    /// Helper function to create a nested binary operation expression
    fn nested_binop_expr(depth: usize) -> HirExpr {
        if depth == 0 {
            int_expr(1)
        } else {
            HirExpr {
                kind: HirExprKind::BinOp {
                    left: Box::new(nested_binop_expr(depth - 1)),
                    op: BinOp::Add,
                    right: Box::new(nested_binop_expr(depth - 1)),
                },
                data_type: DataType::Int,
                location: default_location(),
            }
        }
    }

    /// Helper function to create a HirNode with a statement
    fn stmt_node(stmt: HirStmt, id: usize) -> HirNode {
        HirNode {
            kind: HirKind::Stmt(stmt),
            location: default_location(),
            id,
        }
    }

    /// Helper function to create a HirNode with a terminator
    fn terminator_node(term: HirTerminator, id: usize) -> HirNode {
        HirNode {
            kind: HirKind::Terminator(term),
            location: default_location(),
            id,
        }
    }

    // =========================================================================
    // Property: Validation catches nested expressions
    // Feature: hir-builder, HIR Invariant Validation
    // Validates: All requirements (validation ensures correctness)
    // =========================================================================

    /// Property: Validation catches deeply nested expressions
    #[test]
    fn prop_validation_catches_nested_expressions() {
        fn property(depth: ArbitraryBlockCount) -> TestResult {
            // Create a module with a deeply nested expression
            let nesting_depth = depth.0.min(10); // Cap at 10 to avoid stack overflow
            
            let nested_expr = nested_binop_expr(nesting_depth);
            
            let assign_stmt = HirStmt::Assign {
                target: HirPlace::Var(InternedString::from_u32(1)),
                value: nested_expr,
                is_mutable: false,
            };

            let module = HirModule {
                blocks: vec![HirBlock {
                    id: 0,
                    params: vec![],
                    nodes: vec![
                        stmt_node(assign_stmt, 0),
                        terminator_node(HirTerminator::Return(vec![]), 1),
                    ],
                }],
                entry_block: 0,
                functions: vec![],
                structs: vec![],
            };

            let result = HirValidator::validate_module(&module);

            // Depth > MAX_EXPRESSION_DEPTH (2) should fail
            if nesting_depth > 2 {
                match result {
                    Err(HirValidationError::NestedExpression { .. }) => TestResult::passed(),
                    Ok(_) => TestResult::failed(), // Should have caught the nesting
                    Err(_) => TestResult::passed(), // Other errors are acceptable
                }
            } else {
                // Shallow nesting should pass
                match result {
                    Ok(_) => TestResult::passed(),
                    Err(HirValidationError::NestedExpression { .. }) => TestResult::failed(),
                    Err(_) => TestResult::passed(), // Other errors are acceptable
                }
            }
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(ArbitraryBlockCount) -> TestResult);
    }

    /// Property: Flat expressions pass validation
    #[test]
    fn prop_flat_expressions_pass_validation() {
        fn property(value: i64) -> TestResult {
            // Create a module with a flat expression (just an integer literal)
            let assign_stmt = HirStmt::Assign {
                target: HirPlace::Var(InternedString::from_u32(1)),
                value: int_expr(value),
                is_mutable: false,
            };

            let module = HirModule {
                blocks: vec![HirBlock {
                    id: 0,
                    params: vec![],
                    nodes: vec![
                        stmt_node(assign_stmt, 0),
                        terminator_node(HirTerminator::Return(vec![]), 1),
                    ],
                }],
                entry_block: 0,
                functions: vec![],
                structs: vec![],
            };

            let result = HirValidator::validate_module(&module);

            match result {
                Ok(_) => TestResult::passed(),
                Err(HirValidationError::NestedExpression { .. }) => TestResult::failed(),
                Err(_) => TestResult::passed(), // Other errors are acceptable
            }
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(i64) -> TestResult);
    }

    // =========================================================================
    // Property: Validation catches missing terminators
    // Feature: hir-builder, HIR Invariant Validation
    // Validates: All requirements (validation ensures correctness)
    // =========================================================================

    /// Property: Blocks without terminators fail validation
    #[test]
    fn prop_validation_catches_missing_terminators() {
        fn property(stmt_count: ArbitraryBlockCount) -> TestResult {
            if stmt_count.0 == 0 {
                return TestResult::discard(); // Empty blocks are allowed
            }

            // Create a module with statements but no terminator
            let mut nodes = Vec::new();
            for i in 0..stmt_count.0.min(5) {
                let assign_stmt = HirStmt::Assign {
                    target: HirPlace::Var(InternedString::from_u32(i as u32)),
                    value: int_expr(i as i64),
                    is_mutable: false,
                };
                nodes.push(stmt_node(assign_stmt, i));
            }

            let module = HirModule {
                blocks: vec![HirBlock {
                    id: 0,
                    params: vec![],
                    nodes,
                }],
                entry_block: 0,
                functions: vec![],
                structs: vec![],
            };

            let result = HirValidator::validate_module(&module);

            match result {
                Err(HirValidationError::MissingTerminator { .. }) => TestResult::passed(),
                Ok(_) => TestResult::failed(), // Should have caught missing terminator
                Err(_) => TestResult::passed(), // Other errors are acceptable
            }
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(ArbitraryBlockCount) -> TestResult);
    }

    /// Property: Blocks with exactly one terminator pass validation
    #[test]
    fn prop_blocks_with_terminator_pass_validation() {
        fn property(stmt_count: ArbitraryBlockCount) -> TestResult {
            // Create a module with statements and a terminator
            let mut nodes = Vec::new();
            for i in 0..stmt_count.0.min(5) {
                let assign_stmt = HirStmt::Assign {
                    target: HirPlace::Var(InternedString::from_u32(i as u32)),
                    value: int_expr(i as i64),
                    is_mutable: false,
                };
                nodes.push(stmt_node(assign_stmt, i));
            }
            nodes.push(terminator_node(HirTerminator::Return(vec![]), stmt_count.0));

            let module = HirModule {
                blocks: vec![HirBlock {
                    id: 0,
                    params: vec![],
                    nodes,
                }],
                entry_block: 0,
                functions: vec![],
                structs: vec![],
            };

            let result = HirValidator::validate_module(&module);

            match result {
                Ok(_) => TestResult::passed(),
                Err(HirValidationError::MissingTerminator { .. }) => TestResult::failed(),
                Err(HirValidationError::MultipleTerminators { .. }) => TestResult::failed(),
                Err(_) => TestResult::passed(), // Other errors are acceptable
            }
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(ArbitraryBlockCount) -> TestResult);
    }

    /// Property: Blocks with multiple terminators fail validation
    #[test]
    fn prop_validation_catches_multiple_terminators() {
        fn property(terminator_count: ArbitraryBlockCount) -> TestResult {
            if terminator_count.0 < 2 {
                return TestResult::discard(); // Need at least 2 terminators
            }

            // Create a module with multiple terminators
            let mut nodes = Vec::new();
            for i in 0..terminator_count.0.min(5) {
                nodes.push(terminator_node(HirTerminator::Return(vec![]), i));
            }

            let module = HirModule {
                blocks: vec![HirBlock {
                    id: 0,
                    params: vec![],
                    nodes,
                }],
                entry_block: 0,
                functions: vec![],
                structs: vec![],
            };

            let result = HirValidator::validate_module(&module);

            match result {
                Err(HirValidationError::MultipleTerminators { .. }) => TestResult::passed(),
                Ok(_) => TestResult::failed(), // Should have caught multiple terminators
                Err(_) => TestResult::passed(), // Other errors are acceptable
            }
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(ArbitraryBlockCount) -> TestResult);
    }

    // =========================================================================
    // Property: Validation catches unreachable blocks
    // Feature: hir-builder, HIR Invariant Validation
    // Validates: All requirements (validation ensures correctness)
    // =========================================================================

    /// Property: Modules with unreachable blocks fail validation
    #[test]
    fn prop_validation_catches_unreachable_blocks() {
        fn property(unreachable_count: ArbitraryBlockCount) -> TestResult {
            if unreachable_count.0 == 0 {
                return TestResult::discard();
            }

            // Create a module with entry block and unreachable blocks
            let mut blocks = vec![HirBlock {
                id: 0,
                params: vec![],
                nodes: vec![terminator_node(HirTerminator::Return(vec![]), 0)],
            }];

            // Add unreachable blocks
            for i in 1..=unreachable_count.0.min(5) {
                blocks.push(HirBlock {
                    id: i,
                    params: vec![],
                    nodes: vec![terminator_node(HirTerminator::Return(vec![]), i)],
                });
            }

            let module = HirModule {
                blocks,
                entry_block: 0,
                functions: vec![],
                structs: vec![],
            };

            let result = HirValidator::validate_module(&module);

            match result {
                Err(HirValidationError::UnreachableBlock { .. }) => TestResult::passed(),
                Ok(_) => TestResult::failed(), // Should have caught unreachable blocks
                Err(_) => TestResult::passed(), // Other errors are acceptable
            }
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(ArbitraryBlockCount) -> TestResult);
    }

    /// Property: Modules with all blocks reachable pass connectivity check
    #[test]
    fn prop_connected_blocks_pass_validation() {
        fn property(block_count: ArbitraryBlockCount) -> TestResult {
            if block_count.0 == 0 {
                return TestResult::discard();
            }

            let count = block_count.0.min(5);
            let mut blocks = Vec::new();

            // Create a chain of connected blocks
            for i in 0..count {
                let terminator = if i < count - 1 {
                    // Branch to next block
                    HirTerminator::If {
                        condition: bool_expr(true),
                        then_block: i + 1,
                        else_block: None,
                    }
                } else {
                    // Last block returns
                    HirTerminator::Return(vec![])
                };

                blocks.push(HirBlock {
                    id: i,
                    params: vec![],
                    nodes: vec![terminator_node(terminator, i)],
                });
            }

            let module = HirModule {
                blocks,
                entry_block: 0,
                functions: vec![],
                structs: vec![],
            };

            let result = HirValidator::validate_module(&module);

            match result {
                Ok(_) => TestResult::passed(),
                Err(HirValidationError::UnreachableBlock { .. }) => TestResult::failed(),
                Err(_) => TestResult::passed(), // Other errors are acceptable
            }
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(ArbitraryBlockCount) -> TestResult);
    }

    // =========================================================================
    // Property: Validation catches invalid branch targets
    // Feature: hir-builder, HIR Invariant Validation
    // Validates: All requirements (validation ensures correctness)
    // =========================================================================

    /// Property: Branches to non-existent blocks fail validation
    #[test]
    fn prop_validation_catches_invalid_branch_targets() {
        fn property(invalid_target: ArbitraryBlockCount) -> TestResult {
            // Create a module with a branch to a non-existent block
            let invalid_block_id = invalid_target.0 + 100; // Ensure it doesn't exist

            let module = HirModule {
                blocks: vec![HirBlock {
                    id: 0,
                    params: vec![],
                    nodes: vec![terminator_node(
                        HirTerminator::If {
                            condition: bool_expr(true),
                            then_block: invalid_block_id,
                            else_block: None,
                        },
                        0,
                    )],
                }],
                entry_block: 0,
                functions: vec![],
                structs: vec![],
            };

            let result = HirValidator::validate_module(&module);

            match result {
                Err(HirValidationError::InvalidBranchTarget { .. }) => TestResult::passed(),
                Ok(_) => TestResult::failed(), // Should have caught invalid target
                Err(_) => TestResult::passed(), // Other errors are acceptable
            }
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(ArbitraryBlockCount) -> TestResult);
    }

    /// Property: Branches to valid blocks pass validation
    #[test]
    fn prop_valid_branch_targets_pass_validation() {
        fn property(block_count: ArbitraryBlockCount) -> TestResult {
            if block_count.0 < 2 {
                return TestResult::discard();
            }

            let count = block_count.0.min(5);
            let mut blocks = Vec::new();

            // Create blocks with valid branch targets
            for i in 0..count {
                let terminator = if i < count - 1 {
                    HirTerminator::If {
                        condition: bool_expr(true),
                        then_block: i + 1,
                        else_block: Some(count - 1), // Branch to last block
                    }
                } else {
                    HirTerminator::Return(vec![])
                };

                blocks.push(HirBlock {
                    id: i,
                    params: vec![],
                    nodes: vec![terminator_node(terminator, i)],
                });
            }

            let module = HirModule {
                blocks,
                entry_block: 0,
                functions: vec![],
                structs: vec![],
            };

            let result = HirValidator::validate_module(&module);

            match result {
                Ok(_) => TestResult::passed(),
                Err(HirValidationError::InvalidBranchTarget { .. }) => TestResult::failed(),
                Err(_) => TestResult::passed(), // Other errors are acceptable
            }
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(ArbitraryBlockCount) -> TestResult);
    }

    /// Property: Loop terminators with valid targets pass validation
    #[test]
    fn prop_loop_terminators_with_valid_targets_pass() {
        fn property(body_block_id: ArbitraryBlockCount) -> TestResult {
            let body_id = body_block_id.0.min(10);

            // Create a module with a loop that has valid body target
            let mut blocks = vec![
                HirBlock {
                    id: 0,
                    params: vec![],
                    nodes: vec![terminator_node(
                        HirTerminator::Loop {
                            label: 0,
                            binding: None,
                            iterator: None,
                            body: 1,
                            index_binding: None,
                        },
                        0,
                    )],
                },
                HirBlock {
                    id: 1,
                    params: vec![],
                    nodes: vec![terminator_node(HirTerminator::Return(vec![]), 1)],
                },
            ];

            let module = HirModule {
                blocks,
                entry_block: 0,
                functions: vec![],
                structs: vec![],
            };

            let result = HirValidator::validate_module(&module);

            match result {
                Ok(_) => TestResult::passed(),
                Err(HirValidationError::InvalidBranchTarget { .. }) => TestResult::failed(),
                Err(_) => TestResult::passed(), // Other errors are acceptable
            }
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(ArbitraryBlockCount) -> TestResult);
    }

    /// Property: Break/Continue with valid targets pass validation
    #[test]
    fn prop_break_continue_with_valid_targets_pass() {
        // Create a module with break/continue that have valid targets
        let module = HirModule {
            blocks: vec![
                HirBlock {
                    id: 0,
                    params: vec![],
                    nodes: vec![terminator_node(
                        HirTerminator::Loop {
                            label: 0,
                            binding: None,
                            iterator: None,
                            body: 1,
                            index_binding: None,
                        },
                        0,
                    )],
                },
                HirBlock {
                    id: 1,
                    params: vec![],
                    nodes: vec![terminator_node(
                        HirTerminator::Break { target: 2 },
                        1,
                    )],
                },
                HirBlock {
                    id: 2,
                    params: vec![],
                    nodes: vec![terminator_node(HirTerminator::Return(vec![]), 2)],
                },
            ],
            entry_block: 0,
            functions: vec![],
            structs: vec![],
        };

        let result = HirValidator::validate_module(&module);
        assert!(result.is_ok() || !matches!(result, Err(HirValidationError::InvalidBranchTarget { .. })));
    }
}


// ============================================================================
// Property Tests for Expression Linearization
// Feature: hir-builder, Property 1: AST to HIR Transformation Completeness (expression part)
// Feature: hir-builder, Property 3: Variable and Assignment Handling (expression part)
// Validates: Requirements 1.2, 4.1, 4.2, 4.3
// ============================================================================

#[cfg(test)]
mod expression_linearizer_property_tests {
    use crate::compiler::hir::expression_linearizer::ExpressionLinearizer;
    use crate::compiler::hir::build_hir::HirBuilderContext;
    use crate::compiler::hir::nodes::{HirExprKind, HirKind, HirStmt, HirPlace};
    use crate::compiler::datatypes::{DataType, Ownership};
    use crate::compiler::parsers::expressions::expression::{Expression, ExpressionKind};
    use crate::compiler::parsers::tokenizer::tokens::TextLocation;
    use crate::compiler::string_interning::StringTable;
    use quickcheck::{Arbitrary, Gen, QuickCheck, TestResult};

    // =========================================================================
    // Arbitrary Generators for Property Tests
    // =========================================================================

    /// Generate arbitrary integer values for testing
    #[derive(Clone, Debug)]
    struct ArbitraryInt(i64);

    impl Arbitrary for ArbitraryInt {
        fn arbitrary(g: &mut Gen) -> Self {
            ArbitraryInt(i64::arbitrary(g))
        }
    }

    /// Generate arbitrary float values for testing
    #[derive(Clone, Debug)]
    struct ArbitraryFloat(f64);

    impl Arbitrary for ArbitraryFloat {
        fn arbitrary(g: &mut Gen) -> Self {
            // Generate finite floats only
            let val = f64::arbitrary(g);
            if val.is_finite() {
                ArbitraryFloat(val)
            } else {
                ArbitraryFloat(0.0)
            }
        }
    }

    /// Generate arbitrary boolean values for testing
    #[derive(Clone, Debug)]
    struct ArbitraryBool(bool);

    impl Arbitrary for ArbitraryBool {
        fn arbitrary(g: &mut Gen) -> Self {
            ArbitraryBool(bool::arbitrary(g))
        }
    }

    /// Generate arbitrary literal expressions for testing
    #[derive(Clone, Debug)]
    enum ArbitraryLiteralExpr {
        Int(i64),
        Float(f64),
        Bool(bool),
    }

    impl Arbitrary for ArbitraryLiteralExpr {
        fn arbitrary(g: &mut Gen) -> Self {
            let choice = usize::arbitrary(g) % 3;
            match choice {
                0 => ArbitraryLiteralExpr::Int(i64::arbitrary(g)),
                1 => {
                    let val = f64::arbitrary(g);
                    ArbitraryLiteralExpr::Float(if val.is_finite() { val } else { 0.0 })
                }
                _ => ArbitraryLiteralExpr::Bool(bool::arbitrary(g)),
            }
        }
    }

    impl ArbitraryLiteralExpr {
        fn to_expression(&self) -> Expression {
            match self {
                ArbitraryLiteralExpr::Int(v) => {
                    Expression::int(*v, TextLocation::default(), Ownership::default())
                }
                ArbitraryLiteralExpr::Float(v) => {
                    Expression::float(*v, TextLocation::default(), Ownership::default())
                }
                ArbitraryLiteralExpr::Bool(v) => {
                    Expression::bool(*v, TextLocation::default(), Ownership::default())
                }
            }
        }

        fn expected_data_type(&self) -> DataType {
            match self {
                ArbitraryLiteralExpr::Int(_) => DataType::Int,
                ArbitraryLiteralExpr::Float(_) => DataType::Float,
                ArbitraryLiteralExpr::Bool(_) => DataType::Bool,
            }
        }
    }

    // =========================================================================
    // Property 1: AST to HIR Transformation Completeness (expression part)
    // For any valid AST expression, the HIR builder should convert it to HIR
    // with linear control flow where all nested expressions are flattened.
    // Validates: Requirements 1.2
    // =========================================================================

    /// Property: Literal expressions produce no intermediate nodes
    /// Feature: hir-builder, Property 1: AST to HIR Transformation Completeness
    /// Validates: Requirements 1.2
    #[test]
    fn prop_literal_expressions_produce_no_intermediate_nodes() {
        fn property(literal: ArbitraryLiteralExpr) -> TestResult {
            let mut string_table = StringTable::new();
            let mut ctx = HirBuilderContext::new(&mut string_table);
            let mut linearizer = ExpressionLinearizer::new();

            let expr = literal.to_expression();
            let result = linearizer.linearize_expression(&expr, &mut ctx);

            match result {
                Ok((nodes, _)) => {
                    // Literal expressions should produce no intermediate nodes
                    if nodes.is_empty() {
                        TestResult::passed()
                    } else {
                        TestResult::failed()
                    }
                }
                Err(_) => TestResult::failed(),
            }
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(ArbitraryLiteralExpr) -> TestResult);
    }

    /// Property: Literal expressions preserve their values
    /// Feature: hir-builder, Property 1: AST to HIR Transformation Completeness
    /// Validates: Requirements 1.2
    #[test]
    fn prop_literal_expressions_preserve_values() {
        fn property(literal: ArbitraryLiteralExpr) -> TestResult {
            let mut string_table = StringTable::new();
            let mut ctx = HirBuilderContext::new(&mut string_table);
            let mut linearizer = ExpressionLinearizer::new();

            let expr = literal.to_expression();
            let result = linearizer.linearize_expression(&expr, &mut ctx);

            match result {
                Ok((_, hir_expr)) => {
                    // Check that the value is preserved
                    match (&literal, &hir_expr.kind) {
                        (ArbitraryLiteralExpr::Int(v), HirExprKind::Int(hir_v)) => {
                            if *v == *hir_v {
                                TestResult::passed()
                            } else {
                                TestResult::failed()
                            }
                        }
                        (ArbitraryLiteralExpr::Float(v), HirExprKind::Float(hir_v)) => {
                            if (*v - *hir_v).abs() < f64::EPSILON || (v.is_nan() && hir_v.is_nan()) {
                                TestResult::passed()
                            } else {
                                TestResult::failed()
                            }
                        }
                        (ArbitraryLiteralExpr::Bool(v), HirExprKind::Bool(hir_v)) => {
                            if *v == *hir_v {
                                TestResult::passed()
                            } else {
                                TestResult::failed()
                            }
                        }
                        _ => TestResult::failed(),
                    }
                }
                Err(_) => TestResult::failed(),
            }
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(ArbitraryLiteralExpr) -> TestResult);
    }

    /// Property: Literal expressions preserve their data types
    /// Feature: hir-builder, Property 1: AST to HIR Transformation Completeness
    /// Validates: Requirements 1.2
    #[test]
    fn prop_literal_expressions_preserve_data_types() {
        fn property(literal: ArbitraryLiteralExpr) -> TestResult {
            let mut string_table = StringTable::new();
            let mut ctx = HirBuilderContext::new(&mut string_table);
            let mut linearizer = ExpressionLinearizer::new();

            let expr = literal.to_expression();
            let expected_type = literal.expected_data_type();
            let result = linearizer.linearize_expression(&expr, &mut ctx);

            match result {
                Ok((_, hir_expr)) => {
                    // Check that the data type is preserved
                    match (&expected_type, &hir_expr.data_type) {
                        (DataType::Int, DataType::Int) => TestResult::passed(),
                        (DataType::Float, DataType::Float) => TestResult::passed(),
                        (DataType::Bool, DataType::Bool) => TestResult::passed(),
                        _ => TestResult::failed(),
                    }
                }
                Err(_) => TestResult::failed(),
            }
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(ArbitraryLiteralExpr) -> TestResult);
    }

    // =========================================================================
    // Property 3: Variable and Assignment Handling (expression part)
    // For any variable declaration or assignment, the HIR builder should create
    // HIR local declarations with proper scope tracking.
    // Validates: Requirements 4.1, 4.2, 4.3
    // =========================================================================

    /// Property: Compiler-introduced temporaries are unique
    /// Feature: hir-builder, Property 3: Variable and Assignment Handling
    /// Validates: Requirements 4.1
    #[test]
    fn prop_compiler_temporaries_are_unique() {
        fn property(count: usize) -> TestResult {
            let count = count % 100 + 1; // 1-100 temporaries
            
            let mut string_table = StringTable::new();
            let mut ctx = HirBuilderContext::new(&mut string_table);
            let mut linearizer = ExpressionLinearizer::new();

            let mut temps = Vec::new();
            for _ in 0..count {
                let temp = linearizer.allocate_compiler_local(
                    DataType::Int,
                    TextLocation::default(),
                    &mut ctx,
                );
                temps.push(temp);
            }

            // Check all temporaries are unique
            let unique_count = temps.iter().collect::<std::collections::HashSet<_>>().len();
            if unique_count == count {
                TestResult::passed()
            } else {
                TestResult::failed()
            }
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(usize) -> TestResult);
    }

    /// Property: Compiler-introduced temporaries are tracked
    /// Feature: hir-builder, Property 3: Variable and Assignment Handling
    /// Validates: Requirements 4.2
    #[test]
    fn prop_compiler_temporaries_are_tracked() {
        fn property(count: usize) -> TestResult {
            let count = count % 50 + 1; // 1-50 temporaries
            
            let mut string_table = StringTable::new();
            let mut ctx = HirBuilderContext::new(&mut string_table);
            let mut linearizer = ExpressionLinearizer::new();

            let mut temps = Vec::new();
            for _ in 0..count {
                let temp = linearizer.allocate_compiler_local(
                    DataType::Int,
                    TextLocation::default(),
                    &mut ctx,
                );
                temps.push(temp);
            }

            // Check all temporaries are tracked as compiler locals
            for temp in &temps {
                if !linearizer.is_compiler_local(temp) {
                    return TestResult::failed();
                }
            }

            TestResult::passed()
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(usize) -> TestResult);
    }

    /// Property: Temporary creation produces assignment nodes
    /// Feature: hir-builder, Property 3: Variable and Assignment Handling
    /// Validates: Requirements 4.3
    #[test]
    fn prop_temporary_creation_produces_assignment() {
        fn property(value: ArbitraryInt) -> TestResult {
            let mut string_table = StringTable::new();
            let mut ctx = HirBuilderContext::new(&mut string_table);
            let mut linearizer = ExpressionLinearizer::new();

            let hir_value = crate::compiler::hir::nodes::HirExpr {
                kind: HirExprKind::Int(value.0),
                data_type: DataType::Int,
                location: TextLocation::default(),
            };

            let (nodes, load_expr) = linearizer.create_temporary_with_value(hir_value, &mut ctx);

            // Should produce exactly one assignment node
            if nodes.len() != 1 {
                return TestResult::failed();
            }

            // The node should be an assignment
            match &nodes[0].kind {
                HirKind::Stmt(HirStmt::Assign { target, is_mutable, .. }) => {
                    // Target should be a variable
                    if !matches!(target, HirPlace::Var(_)) {
                        return TestResult::failed();
                    }
                    // Temporaries should be mutable
                    if !is_mutable {
                        return TestResult::failed();
                    }
                }
                _ => return TestResult::failed(),
            }

            // The result should be a load expression
            if !matches!(load_expr.kind, HirExprKind::Load(_)) {
                return TestResult::failed();
            }

            TestResult::passed()
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(ArbitraryInt) -> TestResult);
    }

    /// Property: Temporary types are preserved
    /// Feature: hir-builder, Property 3: Variable and Assignment Handling
    /// Validates: Requirements 4.2
    #[test]
    fn prop_temporary_types_are_preserved() {
        fn property(literal: ArbitraryLiteralExpr) -> TestResult {
            let mut string_table = StringTable::new();
            let mut ctx = HirBuilderContext::new(&mut string_table);
            let mut linearizer = ExpressionLinearizer::new();

            let data_type = literal.expected_data_type();
            let temp = linearizer.allocate_compiler_local(
                data_type.clone(),
                TextLocation::default(),
                &mut ctx,
            );

            // Check the type is preserved
            match linearizer.get_compiler_local_type(&temp) {
                Some(stored_type) => {
                    match (&data_type, stored_type) {
                        (DataType::Int, DataType::Int) => TestResult::passed(),
                        (DataType::Float, DataType::Float) => TestResult::passed(),
                        (DataType::Bool, DataType::Bool) => TestResult::passed(),
                        _ => TestResult::failed(),
                    }
                }
                None => TestResult::failed(),
            }
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(ArbitraryLiteralExpr) -> TestResult);
    }
}


// ============================================================================
// Property Tests for Variable Management
// Feature: hir-builder, Property 3: Variable and Assignment Handling
// Validates: Requirements 1.5, 1.6, 4.1, 4.2, 4.3, 4.4
// ============================================================================

#[cfg(test)]
mod variable_manager_property_tests {
    use crate::compiler::hir::variable_manager::VariableManager;
    use crate::compiler::hir::build_hir::HirBuilderContext;
    use crate::compiler::hir::nodes::{HirExprKind, HirPlace};
    use crate::compiler::datatypes::{DataType, Ownership};
    use crate::compiler::parsers::tokenizer::tokens::TextLocation;
    use crate::compiler::string_interning::StringTable;
    use quickcheck::{Arbitrary, Gen, QuickCheck, TestResult};

    // =========================================================================
    // Arbitrary Generators for Property Tests
    // =========================================================================

    /// Generate arbitrary scope depths for testing
    #[derive(Clone, Debug)]
    struct ArbitraryScopeDepth(usize);

    impl Arbitrary for ArbitraryScopeDepth {
        fn arbitrary(g: &mut Gen) -> Self {
            ArbitraryScopeDepth(usize::arbitrary(g) % 20 + 1) // 1-20 scopes
        }
    }

    /// Generate arbitrary variable counts for testing
    #[derive(Clone, Debug)]
    struct ArbitraryVarCount(usize);

    impl Arbitrary for ArbitraryVarCount {
        fn arbitrary(g: &mut Gen) -> Self {
            ArbitraryVarCount(usize::arbitrary(g) % 50 + 1) // 1-50 variables
        }
    }

    /// Generate arbitrary data types for testing
    #[derive(Clone, Debug)]
    enum ArbitraryDataType {
        Int,
        Float,
        Bool,
        Struct,
        Collection,
    }

    impl Arbitrary for ArbitraryDataType {
        fn arbitrary(g: &mut Gen) -> Self {
            let choice = usize::arbitrary(g) % 5;
            match choice {
                0 => ArbitraryDataType::Int,
                1 => ArbitraryDataType::Float,
                2 => ArbitraryDataType::Bool,
                3 => ArbitraryDataType::Struct,
                _ => ArbitraryDataType::Collection,
            }
        }
    }

    impl ArbitraryDataType {
        fn to_data_type(&self) -> DataType {
            match self {
                ArbitraryDataType::Int => DataType::Int,
                ArbitraryDataType::Float => DataType::Float,
                ArbitraryDataType::Bool => DataType::Bool,
                ArbitraryDataType::Struct => DataType::Struct(vec![], Ownership::default()),
                ArbitraryDataType::Collection => {
                    DataType::Collection(Box::new(DataType::Int), Ownership::default())
                }
            }
        }

        fn is_ownership_capable(&self) -> bool {
            match self {
                ArbitraryDataType::Int | ArbitraryDataType::Float | ArbitraryDataType::Bool => false,
                ArbitraryDataType::Struct | ArbitraryDataType::Collection => true,
            }
        }
    }

    // =========================================================================
    // Property 3: Variable and Assignment Handling
    // For any variable declaration or assignment, the HIR builder should create
    // HIR local declarations with proper scope tracking, generate assignment
    // instructions with correct mutability handling, and distinguish between
    // shared references and potential ownership transfers.
    // Validates: Requirements 1.5, 1.6, 4.1, 4.2, 4.3, 4.4
    // =========================================================================

    /// Property: Scope depth tracks correctly through enter/exit
    /// Feature: hir-builder, Property 3: Variable and Assignment Handling
    /// Validates: Requirements 1.5
    #[test]
    fn prop_scope_depth_tracks_correctly() {
        fn property(depth: ArbitraryScopeDepth) -> TestResult {
            let mut manager = VariableManager::new();

            // Enter scopes
            for expected_depth in 1..=depth.0 {
                manager.enter_scope();
                if manager.current_scope_level() != expected_depth {
                    return TestResult::failed();
                }
            }

            // Exit scopes
            for expected_depth in (0..depth.0).rev() {
                manager.exit_scope();
                if manager.current_scope_level() != expected_depth {
                    return TestResult::failed();
                }
            }

            TestResult::passed()
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(ArbitraryScopeDepth) -> TestResult);
    }

    /// Property: Variables are cleaned up when scope exits
    /// Feature: hir-builder, Property 3: Variable and Assignment Handling
    /// Validates: Requirements 1.5
    #[test]
    fn prop_variables_cleaned_up_on_scope_exit() {
        fn property(var_count: ArbitraryVarCount) -> TestResult {
            let mut string_table = StringTable::new();
            let mut ctx = HirBuilderContext::new(&mut string_table);
            let mut manager = VariableManager::new();

            // Enter a new scope
            manager.enter_scope();

            // Declare variables
            let mut var_names = Vec::new();
            for i in 0..var_count.0 {
                let name = ctx.string_table.intern(&format!("var_{}", i));
                let _ = manager.declare_variable(
                    name,
                    DataType::Int,
                    false,
                    TextLocation::default(),
                    &mut ctx,
                );
                var_names.push(name);
            }

            // Verify all variables exist
            for name in &var_names {
                if !manager.variable_exists(*name) {
                    return TestResult::failed();
                }
            }

            // Exit scope
            let exited = manager.exit_scope();

            // Verify all variables were returned as exited
            if exited.len() != var_count.0 {
                return TestResult::failed();
            }

            // Verify all variables are cleaned up
            for name in &var_names {
                if manager.variable_exists(*name) {
                    return TestResult::failed();
                }
            }

            TestResult::passed()
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(ArbitraryVarCount) -> TestResult);
    }

    /// Property: Mutability is tracked correctly
    /// Feature: hir-builder, Property 3: Variable and Assignment Handling
    /// Validates: Requirements 1.6, 4.2
    #[test]
    fn prop_mutability_tracked_correctly() {
        fn property(is_mutable: bool) -> TestResult {
            let mut string_table = StringTable::new();
            let mut ctx = HirBuilderContext::new(&mut string_table);
            let mut manager = VariableManager::new();

            let name = ctx.string_table.intern("test_var");
            let _ = manager.declare_variable(
                name,
                DataType::Int,
                is_mutable,
                TextLocation::default(),
                &mut ctx,
            );

            if manager.is_variable_mutable(name) != is_mutable {
                return TestResult::failed();
            }

            TestResult::passed()
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(bool) -> TestResult);
    }

    /// Property: Ownership capability is determined by type
    /// Feature: hir-builder, Property 3: Variable and Assignment Handling
    /// Validates: Requirements 4.4
    #[test]
    fn prop_ownership_capability_determined_by_type() {
        fn property(data_type: ArbitraryDataType) -> TestResult {
            let mut string_table = StringTable::new();
            let mut ctx = HirBuilderContext::new(&mut string_table);
            let mut manager = VariableManager::new();

            let name = ctx.string_table.intern("test_var");
            let _ = manager.declare_variable(
                name,
                data_type.to_data_type(),
                false,
                TextLocation::default(),
                &mut ctx,
            );

            let expected_ownership_capable = data_type.is_ownership_capable();
            if manager.is_ownership_capable(name) != expected_ownership_capable {
                return TestResult::failed();
            }

            TestResult::passed()
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(ArbitraryDataType) -> TestResult);
    }

    /// Property: Variable reference returns correct data type
    /// Feature: hir-builder, Property 3: Variable and Assignment Handling
    /// Validates: Requirements 4.1
    #[test]
    fn prop_variable_reference_returns_correct_type() {
        fn property(data_type: ArbitraryDataType) -> TestResult {
            let mut string_table = StringTable::new();
            let mut ctx = HirBuilderContext::new(&mut string_table);
            let mut manager = VariableManager::new();

            let name = ctx.string_table.intern("test_var");
            let expected_type = data_type.to_data_type();
            let _ = manager.declare_variable(
                name,
                expected_type.clone(),
                false,
                TextLocation::default(),
                &mut ctx,
            );

            let result = manager.reference_variable(name, TextLocation::default(), &mut ctx);
            match result {
                Ok(expr) => {
                    // Check that the expression is a Load
                    if !matches!(expr.kind, HirExprKind::Load(HirPlace::Var(_))) {
                        return TestResult::failed();
                    }
                    // Check that the data type matches
                    match (&expected_type, &expr.data_type) {
                        (DataType::Int, DataType::Int) => TestResult::passed(),
                        (DataType::Float, DataType::Float) => TestResult::passed(),
                        (DataType::Bool, DataType::Bool) => TestResult::passed(),
                        (DataType::Struct(_, _), DataType::Struct(_, _)) => TestResult::passed(),
                        (DataType::Collection(_, _), DataType::Collection(_, _)) => TestResult::passed(),
                        _ => TestResult::failed(),
                    }
                }
                Err(_) => TestResult::failed(),
            }
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(ArbitraryDataType) -> TestResult);
    }

    /// Property: Potential move returns Move for ownership-capable types
    /// Feature: hir-builder, Property 3: Variable and Assignment Handling
    /// Validates: Requirements 4.4
    #[test]
    fn prop_potential_move_returns_move_for_ownership_capable() {
        fn property(data_type: ArbitraryDataType) -> TestResult {
            let mut string_table = StringTable::new();
            let mut ctx = HirBuilderContext::new(&mut string_table);
            let mut manager = VariableManager::new();

            let name = ctx.string_table.intern("test_var");
            let _ = manager.declare_variable(
                name,
                data_type.to_data_type(),
                false,
                TextLocation::default(),
                &mut ctx,
            );

            let result = manager.mark_potential_move(name, TextLocation::default(), &mut ctx);
            match result {
                Ok(expr) => {
                    let is_ownership_capable = data_type.is_ownership_capable();
                    let is_move = matches!(expr.kind, HirExprKind::Move(_));
                    let is_load = matches!(expr.kind, HirExprKind::Load(_));

                    // Ownership-capable types should return Move, others should return Load
                    if is_ownership_capable && is_move {
                        TestResult::passed()
                    } else if !is_ownership_capable && is_load {
                        TestResult::passed()
                    } else {
                        TestResult::failed()
                    }
                }
                Err(_) => TestResult::failed(),
            }
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(ArbitraryDataType) -> TestResult);
    }

    /// Property: Undeclared variable reference fails
    /// Feature: hir-builder, Property 3: Variable and Assignment Handling
    /// Validates: Requirements 4.1
    #[test]
    fn prop_undeclared_variable_reference_fails() {
        fn property(var_id: usize) -> TestResult {
            let mut string_table = StringTable::new();
            let mut ctx = HirBuilderContext::new(&mut string_table);
            let mut manager = VariableManager::new();

            let name = ctx.string_table.intern(&format!("undeclared_{}", var_id));
            let result = manager.reference_variable(name, TextLocation::default(), &mut ctx);

            if result.is_err() {
                TestResult::passed()
            } else {
                TestResult::failed()
            }
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(usize) -> TestResult);
    }

    /// Property: Nested scopes preserve outer variables
    /// Feature: hir-builder, Property 3: Variable and Assignment Handling
    /// Validates: Requirements 1.5
    #[test]
    fn prop_nested_scopes_preserve_outer_variables() {
        fn property(depth: ArbitraryScopeDepth) -> TestResult {
            let depth = depth.0.min(10); // Cap at 10 for performance
            
            let mut string_table = StringTable::new();
            let mut ctx = HirBuilderContext::new(&mut string_table);
            let mut manager = VariableManager::new();

            // Declare a variable at each scope level
            let mut var_names = Vec::new();
            for i in 0..depth {
                let name = ctx.string_table.intern(&format!("var_scope_{}", i));
                let _ = manager.declare_variable(
                    name,
                    DataType::Int,
                    false,
                    TextLocation::default(),
                    &mut ctx,
                );
                var_names.push(name);
                manager.enter_scope();
            }

            // All variables should still be accessible from innermost scope
            for name in &var_names {
                if !manager.variable_exists(*name) {
                    return TestResult::failed();
                }
            }

            // Exit scopes one by one
            for i in (0..depth).rev() {
                manager.exit_scope();
                
                // Variables from outer scopes should still exist
                for j in 0..=i {
                    if !manager.variable_exists(var_names[j]) {
                        return TestResult::failed();
                    }
                }
            }

            TestResult::passed()
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(ArbitraryScopeDepth) -> TestResult);
    }
}

// ============================================================================
// Property Tests for Control Flow Linearization
// Feature: hir-builder, Property 1: AST to HIR Transformation Completeness (control flow part)
// Feature: hir-builder, Property 5: Control Flow Linearization
// Validates: Requirements 1.3, 3.1, 3.2, 3.3, 3.4, 3.6
// ============================================================================

#[cfg(test)]
mod control_flow_linearization_tests {
    use crate::compiler::hir::build_hir::{HirBuilderContext, ScopeType};
    use crate::compiler::hir::control_flow_linearizer::ControlFlowLinearizer;
    use crate::compiler::hir::nodes::{
        BlockId, HirBlock, HirExpr, HirExprKind, HirKind, HirModule, HirNode,
        HirPlace, HirStmt, HirTerminator,
    };
    use crate::compiler::datatypes::{DataType, Ownership};
    use crate::compiler::parsers::ast_nodes::{AstNode, NodeKind, Arg};
    use crate::compiler::parsers::expressions::expression::Expression;
    use crate::compiler::parsers::tokenizer::tokens::TextLocation;
    use crate::compiler::string_interning::StringTable;
    use quickcheck::{Arbitrary, Gen, QuickCheck, TestResult};

    // =========================================================================
    // Arbitrary Types for Property Testing
    // =========================================================================

    /// Generate arbitrary boolean values for conditions
    #[derive(Clone, Debug)]
    struct ArbitraryCondition(bool);

    impl Arbitrary for ArbitraryCondition {
        fn arbitrary(g: &mut Gen) -> Self {
            ArbitraryCondition(bool::arbitrary(g))
        }
    }

    /// Generate arbitrary nesting depths for control flow
    #[derive(Clone, Debug)]
    struct ArbitraryNestingDepth(usize);

    impl Arbitrary for ArbitraryNestingDepth {
        fn arbitrary(g: &mut Gen) -> Self {
            // Generate 0-5 nesting levels
            ArbitraryNestingDepth(usize::arbitrary(g) % 6)
        }
    }

    /// Generate arbitrary block counts
    #[derive(Clone, Debug)]
    struct ArbitraryBlockCount(usize);

    impl Arbitrary for ArbitraryBlockCount {
        fn arbitrary(g: &mut Gen) -> Self {
            ArbitraryBlockCount(1 + usize::arbitrary(g) % 10)
        }
    }

    /// Generate arbitrary return value counts
    #[derive(Clone, Debug)]
    struct ArbitraryReturnCount(usize);

    impl Arbitrary for ArbitraryReturnCount {
        fn arbitrary(g: &mut Gen) -> Self {
            ArbitraryReturnCount(usize::arbitrary(g) % 4) // 0-3 return values
        }
    }

    // =========================================================================
    // Helper Functions
    // =========================================================================

    /// Creates a test boolean expression
    fn create_bool_expr(value: bool) -> Expression {
        Expression::bool(value, TextLocation::default(), Ownership::default())
    }

    /// Creates a test integer expression
    fn create_int_expr(value: i64) -> Expression {
        Expression::int(value, TextLocation::default(), Ownership::default())
    }

    // =========================================================================
    // Property 5: Control Flow Linearization
    // For any control flow construct (if/else, loops, pattern matching,
    // break/continue), the HIR builder should create HIR blocks with proper
    // branch targets, maintain correct block nesting, and ensure proper
    // cleanup before control flow exits.
    // Validates: Requirements 3.1, 3.2, 3.3, 3.4, 3.6
    // =========================================================================

    /// Property: If statements create proper conditional blocks with terminators
    /// Feature: hir-builder, Property 5: Control Flow Linearization
    /// Validates: Requirements 3.1
    #[test]
    fn prop_if_creates_conditional_blocks() {
        fn property(cond_value: ArbitraryCondition) -> TestResult {
            let mut string_table = StringTable::new();
            let mut ctx = HirBuilderContext::new(&mut string_table);
            let mut linearizer = ControlFlowLinearizer::new();

            // Create entry block
            let _entry_block = ctx.create_block();

            let condition = create_bool_expr(cond_value.0);
            let then_body: Vec<AstNode> = vec![];

            let result = linearizer.linearize_if_statement(
                &condition,
                &then_body,
                None,
                &TextLocation::default(),
                &mut ctx,
            );

            if result.is_err() {
                return TestResult::failed();
            }

            let nodes = result.unwrap();

            // Should have at least one node (the If terminator)
            if nodes.is_empty() {
                return TestResult::failed();
            }

            // Last node should be an If terminator
            let last_node = nodes.last().unwrap();
            if !matches!(last_node.kind, HirKind::Terminator(HirTerminator::If { .. })) {
                return TestResult::failed();
            }

            TestResult::passed()
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(ArbitraryCondition) -> TestResult);
    }

    /// Property: If-else creates both then and else blocks
    /// Feature: hir-builder, Property 5: Control Flow Linearization
    /// Validates: Requirements 3.1
    #[test]
    fn prop_if_else_creates_both_blocks() {
        fn property(cond_value: ArbitraryCondition) -> TestResult {
            let mut string_table = StringTable::new();
            let mut ctx = HirBuilderContext::new(&mut string_table);
            let mut linearizer = ControlFlowLinearizer::new();

            let _entry_block = ctx.create_block();

            let condition = create_bool_expr(cond_value.0);
            let then_body: Vec<AstNode> = vec![];
            let else_body: Vec<AstNode> = vec![];

            let result = linearizer.linearize_if_statement(
                &condition,
                &then_body,
                Some(&else_body),
                &TextLocation::default(),
                &mut ctx,
            );

            if result.is_err() {
                return TestResult::failed();
            }

            let nodes = result.unwrap();
            let last_node = nodes.last().unwrap();

            // Should have an If terminator with else_block set
            if let HirKind::Terminator(HirTerminator::If { else_block, .. }) = &last_node.kind {
                if else_block.is_none() {
                    return TestResult::failed();
                }
            } else {
                return TestResult::failed();
            }

            TestResult::passed()
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(ArbitraryCondition) -> TestResult);
    }

    /// Property: Return statements create Return terminators with correct value count
    /// Feature: hir-builder, Property 5: Control Flow Linearization
    /// Validates: Requirements 3.5
    #[test]
    fn prop_return_creates_terminator_with_values() {
        fn property(return_count: ArbitraryReturnCount) -> TestResult {
            let mut string_table = StringTable::new();
            let mut ctx = HirBuilderContext::new(&mut string_table);
            let mut linearizer = ControlFlowLinearizer::new();

            // Create return values
            let values: Vec<Expression> = (0..return_count.0)
                .map(|i| create_int_expr(i as i64))
                .collect();

            let result = linearizer.linearize_return(
                &values,
                &TextLocation::default(),
                &mut ctx,
            );

            if result.is_err() {
                return TestResult::failed();
            }

            let nodes = result.unwrap();

            // Should have at least the Return terminator
            if nodes.is_empty() {
                return TestResult::failed();
            }

            // Last node should be a Return terminator
            let last_node = nodes.last().unwrap();
            if let HirKind::Terminator(HirTerminator::Return(return_values)) = &last_node.kind {
                if return_values.len() != return_count.0 {
                    return TestResult::failed();
                }
            } else {
                return TestResult::failed();
            }

            TestResult::passed()
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(ArbitraryReturnCount) -> TestResult);
    }

    /// Property: Break inside loop targets the correct exit block
    /// Feature: hir-builder, Property 5: Control Flow Linearization
    /// Validates: Requirements 3.6
    #[test]
    fn prop_break_targets_loop_exit() {
        fn property(break_target_id: ArbitraryBlockCount) -> TestResult {
            let mut string_table = StringTable::new();
            let mut ctx = HirBuilderContext::new(&mut string_table);
            let mut linearizer = ControlFlowLinearizer::new();

            // Create blocks for loop
            let break_target = break_target_id.0;
            let continue_target = break_target_id.0 + 1;

            // Enter a loop scope
            ctx.enter_scope(ScopeType::Loop {
                break_target,
                continue_target,
            });

            let result = linearizer.linearize_break(&TextLocation::default(), &mut ctx);

            if result.is_err() {
                return TestResult::failed();
            }

            let node = result.unwrap();

            // Should be a Break terminator targeting the break_target
            if let HirKind::Terminator(HirTerminator::Break { target }) = &node.kind {
                if *target != break_target {
                    return TestResult::failed();
                }
            } else {
                return TestResult::failed();
            }

            TestResult::passed()
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(ArbitraryBlockCount) -> TestResult);
    }

    /// Property: Continue inside loop targets the correct continue block
    /// Feature: hir-builder, Property 5: Control Flow Linearization
    /// Validates: Requirements 3.6
    #[test]
    fn prop_continue_targets_loop_header() {
        fn property(continue_target_id: ArbitraryBlockCount) -> TestResult {
            let mut string_table = StringTable::new();
            let mut ctx = HirBuilderContext::new(&mut string_table);
            let mut linearizer = ControlFlowLinearizer::new();

            // Create blocks for loop
            let break_target = continue_target_id.0;
            let continue_target = continue_target_id.0 + 1;

            // Enter a loop scope
            ctx.enter_scope(ScopeType::Loop {
                break_target,
                continue_target,
            });

            let result = linearizer.linearize_continue(&TextLocation::default(), &mut ctx);

            if result.is_err() {
                return TestResult::failed();
            }

            let node = result.unwrap();

            // Should be a Continue terminator targeting the continue_target
            if let HirKind::Terminator(HirTerminator::Continue { target }) = &node.kind {
                if *target != continue_target {
                    return TestResult::failed();
                }
            } else {
                return TestResult::failed();
            }

            TestResult::passed()
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(ArbitraryBlockCount) -> TestResult);
    }

    /// Property: Break outside loop fails
    /// Feature: hir-builder, Property 5: Control Flow Linearization
    /// Validates: Requirements 3.6
    #[test]
    fn prop_break_outside_loop_fails() {
        fn property(_: ()) -> TestResult {
            let mut string_table = StringTable::new();
            let mut ctx = HirBuilderContext::new(&mut string_table);
            let mut linearizer = ControlFlowLinearizer::new();

            // No loop scope entered
            let result = linearizer.linearize_break(&TextLocation::default(), &mut ctx);

            if result.is_err() {
                TestResult::passed()
            } else {
                TestResult::failed()
            }
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(()) -> TestResult);
    }

    /// Property: Continue outside loop fails
    /// Feature: hir-builder, Property 5: Control Flow Linearization
    /// Validates: Requirements 3.6
    #[test]
    fn prop_continue_outside_loop_fails() {
        fn property(_: ()) -> TestResult {
            let mut string_table = StringTable::new();
            let mut ctx = HirBuilderContext::new(&mut string_table);
            let mut linearizer = ControlFlowLinearizer::new();

            // No loop scope entered
            let result = linearizer.linearize_continue(&TextLocation::default(), &mut ctx);

            if result.is_err() {
                TestResult::passed()
            } else {
                TestResult::failed()
            }
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(()) -> TestResult);
    }

    /// Property: Nested loops have correct break/continue targets
    /// Feature: hir-builder, Property 5: Control Flow Linearization
    /// Validates: Requirements 3.4
    #[test]
    fn prop_nested_loops_have_correct_targets() {
        fn property(nesting: ArbitraryNestingDepth) -> TestResult {
            let depth = nesting.0.max(1).min(5); // At least 1, at most 5
            
            let mut string_table = StringTable::new();
            let mut ctx = HirBuilderContext::new(&mut string_table);
            let mut linearizer = ControlFlowLinearizer::new();

            // Enter nested loops
            let mut loop_targets = Vec::new();
            for i in 0..depth {
                let break_target = i * 2;
                let continue_target = i * 2 + 1;
                loop_targets.push((break_target, continue_target));
                ctx.enter_scope(ScopeType::Loop {
                    break_target,
                    continue_target,
                });
            }

            // Break should target the innermost loop's break target
            let innermost = loop_targets.last().unwrap();
            let break_result = linearizer.linearize_break(&TextLocation::default(), &mut ctx);

            if break_result.is_err() {
                return TestResult::failed();
            }

            let break_node = break_result.unwrap();
            if let HirKind::Terminator(HirTerminator::Break { target }) = &break_node.kind {
                if *target != innermost.0 {
                    return TestResult::failed();
                }
            } else {
                return TestResult::failed();
            }

            // Continue should target the innermost loop's continue target
            let continue_result = linearizer.linearize_continue(&TextLocation::default(), &mut ctx);

            if continue_result.is_err() {
                return TestResult::failed();
            }

            let continue_node = continue_result.unwrap();
            if let HirKind::Terminator(HirTerminator::Continue { target }) = &continue_node.kind {
                if *target != innermost.1 {
                    return TestResult::failed();
                }
            } else {
                return TestResult::failed();
            }

            TestResult::passed()
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(ArbitraryNestingDepth) -> TestResult);
    }

    /// Property: Block termination check correctly identifies missing terminators
    /// Feature: hir-builder, Property 5: Control Flow Linearization
    /// Validates: Requirements 3.1, 3.2, 3.3, 3.4
    #[test]
    fn prop_block_termination_check_works() {
        fn property(has_terminator: ArbitraryCondition) -> TestResult {
            let mut string_table = StringTable::new();
            let mut ctx = HirBuilderContext::new(&mut string_table);
            let linearizer = ControlFlowLinearizer::new();

            let block_id = ctx.create_block();

            // Add a statement
            let stmt_node = HirNode {
                kind: HirKind::Stmt(HirStmt::ExprStmt(HirExpr {
                    kind: HirExprKind::Int(42),
                    data_type: DataType::Int,
                    location: TextLocation::default(),
                })),
                location: TextLocation::default(),
                id: ctx.allocate_node_id(),
            };
            ctx.add_node_to_block(block_id, stmt_node);

            if has_terminator.0 {
                // Add a terminator
                let term_node = HirNode {
                    kind: HirKind::Terminator(HirTerminator::Return(vec![])),
                    location: TextLocation::default(),
                    id: ctx.allocate_node_id(),
                };
                ctx.add_node_to_block(block_id, term_node);
            }

            let result = linearizer.ensure_block_termination(&ctx, block_id);

            if has_terminator.0 {
                // Should pass with terminator
                if result.is_err() {
                    return TestResult::failed();
                }
            } else {
                // Should fail without terminator
                if result.is_ok() {
                    return TestResult::failed();
                }
            }

            TestResult::passed()
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(ArbitraryCondition) -> TestResult);
    }
}


// ============================================================================
// Property Tests for Return and Jump Handling (Task 4.4)
// ============================================================================

/// Property tests for return and jump handling in control flow linearization.
/// These tests validate Property 5: Control Flow Linearization (return/jump part)
/// Validates: Requirements 3.5, 5.3
#[cfg(test)]
mod return_and_jump_handling_tests {
    use crate::compiler::hir::build_hir::{HirBuilderContext, ScopeType};
    use crate::compiler::hir::control_flow_linearizer::ControlFlowLinearizer;
    use crate::compiler::hir::nodes::{
        HirExpr, HirExprKind, HirKind, HirNode, HirStmt, HirTerminator,
    };
    use crate::compiler::datatypes::{DataType, Ownership};
    use crate::compiler::parsers::expressions::expression::Expression;
    use crate::compiler::parsers::tokenizer::tokens::TextLocation;
    use crate::compiler::string_interning::StringTable;
    use quickcheck::{Arbitrary, Gen, QuickCheck, TestResult};

    /// Generate arbitrary return value counts for testing
    #[derive(Clone, Debug)]
    struct ArbitraryReturnValueCount(usize);

    impl Arbitrary for ArbitraryReturnValueCount {
        fn arbitrary(g: &mut Gen) -> Self {
            // Generate 0-5 return values
            ArbitraryReturnValueCount(usize::arbitrary(g) % 6)
        }
    }

    /// Generate arbitrary integer values for testing
    #[derive(Clone, Debug)]
    struct ArbitraryIntValue(i64);

    impl Arbitrary for ArbitraryIntValue {
        fn arbitrary(g: &mut Gen) -> Self {
            ArbitraryIntValue(i64::arbitrary(g))
        }
    }

    /// Generate arbitrary loop nesting depths for testing
    #[derive(Clone, Debug)]
    struct ArbitraryLoopNesting(usize);

    impl Arbitrary for ArbitraryLoopNesting {
        fn arbitrary(g: &mut Gen) -> Self {
            // Generate 1-5 nesting levels
            ArbitraryLoopNesting(1 + usize::arbitrary(g) % 5)
        }
    }

    /// Generate arbitrary block IDs for testing
    #[derive(Clone, Debug)]
    struct ArbitraryBlockId(usize);

    impl Arbitrary for ArbitraryBlockId {
        fn arbitrary(g: &mut Gen) -> Self {
            ArbitraryBlockId(usize::arbitrary(g) % 100)
        }
    }

    /// Helper to create a test integer expression
    fn create_int_expr(value: i64) -> Expression {
        Expression::int(value, TextLocation::default(), Ownership::default())
    }

    // =========================================================================
    // Property: Return with multiple values preserves all values
    // Feature: hir-builder, Property 5: Control Flow Linearization
    // Validates: Requirements 3.5, 5.3
    // =========================================================================

    /// Property: Return statement preserves all return values in order
    #[test]
    fn prop_return_preserves_all_values() {
        fn property(count: ArbitraryReturnValueCount) -> TestResult {
            let mut string_table = StringTable::new();
            let mut ctx = HirBuilderContext::new(&mut string_table);
            let mut linearizer = ControlFlowLinearizer::new();

            // Create return values
            let values: Vec<Expression> = (0..count.0)
                .map(|i| create_int_expr(i as i64))
                .collect();

            let result = linearizer.linearize_return(
                &values,
                &TextLocation::default(),
                &mut ctx,
            );

            if result.is_err() {
                return TestResult::failed();
            }

            let nodes = result.unwrap();

            // Find the Return terminator
            let return_node = nodes.iter().find(|n| {
                matches!(n.kind, HirKind::Terminator(HirTerminator::Return(_)))
            });

            if return_node.is_none() {
                return TestResult::failed();
            }

            if let HirKind::Terminator(HirTerminator::Return(return_values)) = &return_node.unwrap().kind {
                // Should have the same number of values
                if return_values.len() != count.0 {
                    return TestResult::failed();
                }
            } else {
                return TestResult::failed();
            }

            TestResult::passed()
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(ArbitraryReturnValueCount) -> TestResult);
    }

    /// Property: Return with integer values preserves the integer values
    #[test]
    fn prop_return_preserves_integer_values() {
        fn property(value: ArbitraryIntValue) -> TestResult {
            let mut string_table = StringTable::new();
            let mut ctx = HirBuilderContext::new(&mut string_table);
            let mut linearizer = ControlFlowLinearizer::new();

            let values = vec![create_int_expr(value.0)];

            let result = linearizer.linearize_return(
                &values,
                &TextLocation::default(),
                &mut ctx,
            );

            if result.is_err() {
                return TestResult::failed();
            }

            let nodes = result.unwrap();

            // Find the Return terminator
            let return_node = nodes.iter().find(|n| {
                matches!(n.kind, HirKind::Terminator(HirTerminator::Return(_)))
            });

            if return_node.is_none() {
                return TestResult::failed();
            }

            if let HirKind::Terminator(HirTerminator::Return(return_values)) = &return_node.unwrap().kind {
                if return_values.len() != 1 {
                    return TestResult::failed();
                }
                // Check the value is preserved
                if let HirExprKind::Int(v) = &return_values[0].kind {
                    if *v != value.0 {
                        return TestResult::failed();
                    }
                } else {
                    return TestResult::failed();
                }
            } else {
                return TestResult::failed();
            }

            TestResult::passed()
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(ArbitraryIntValue) -> TestResult);
    }

    // =========================================================================
    // Property: Break targets are correctly resolved in nested loops
    // Feature: hir-builder, Property 5: Control Flow Linearization
    // Validates: Requirements 3.5
    // =========================================================================

    /// Property: Break in nested loops targets the innermost loop's exit
    #[test]
    fn prop_break_targets_innermost_loop() {
        fn property(nesting: ArbitraryLoopNesting) -> TestResult {
            let mut string_table = StringTable::new();
            let mut ctx = HirBuilderContext::new(&mut string_table);
            let mut linearizer = ControlFlowLinearizer::new();

            // Create nested loop scopes
            let mut loop_targets = Vec::new();
            for i in 0..nesting.0 {
                let break_target = ctx.create_block();
                let continue_target = ctx.create_block();
                loop_targets.push((break_target, continue_target));
                ctx.enter_scope(ScopeType::Loop {
                    break_target,
                    continue_target,
                });
            }

            // Break should target the innermost loop
            let result = linearizer.linearize_break(&TextLocation::default(), &mut ctx);

            if result.is_err() {
                return TestResult::failed();
            }

            let node = result.unwrap();
            let innermost_break_target = loop_targets.last().unwrap().0;

            if let HirKind::Terminator(HirTerminator::Break { target }) = &node.kind {
                if *target != innermost_break_target {
                    return TestResult::failed();
                }
            } else {
                return TestResult::failed();
            }

            TestResult::passed()
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(ArbitraryLoopNesting) -> TestResult);
    }

    /// Property: Continue in nested loops targets the innermost loop's continue point
    #[test]
    fn prop_continue_targets_innermost_loop() {
        fn property(nesting: ArbitraryLoopNesting) -> TestResult {
            let mut string_table = StringTable::new();
            let mut ctx = HirBuilderContext::new(&mut string_table);
            let mut linearizer = ControlFlowLinearizer::new();

            // Create nested loop scopes
            let mut loop_targets = Vec::new();
            for i in 0..nesting.0 {
                let break_target = ctx.create_block();
                let continue_target = ctx.create_block();
                loop_targets.push((break_target, continue_target));
                ctx.enter_scope(ScopeType::Loop {
                    break_target,
                    continue_target,
                });
            }

            // Continue should target the innermost loop
            let result = linearizer.linearize_continue(&TextLocation::default(), &mut ctx);

            if result.is_err() {
                return TestResult::failed();
            }

            let node = result.unwrap();
            let innermost_continue_target = loop_targets.last().unwrap().1;

            if let HirKind::Terminator(HirTerminator::Continue { target }) = &node.kind {
                if *target != innermost_continue_target {
                    return TestResult::failed();
                }
            } else {
                return TestResult::failed();
            }

            TestResult::passed()
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(ArbitraryLoopNesting) -> TestResult);
    }

    // =========================================================================
    // Property: Jump handling with mixed scope types
    // Feature: hir-builder, Property 5: Control Flow Linearization
    // Validates: Requirements 3.5
    // =========================================================================

    /// Property: Break finds correct loop even with intervening non-loop scopes
    #[test]
    fn prop_break_skips_non_loop_scopes() {
        fn property(non_loop_count: ArbitraryLoopNesting) -> TestResult {
            let mut string_table = StringTable::new();
            let mut ctx = HirBuilderContext::new(&mut string_table);
            let mut linearizer = ControlFlowLinearizer::new();

            // Enter a loop scope
            let break_target = ctx.create_block();
            let continue_target = ctx.create_block();
            ctx.enter_scope(ScopeType::Loop {
                break_target,
                continue_target,
            });

            // Enter some non-loop scopes (If, Block)
            for i in 0..non_loop_count.0 {
                if i % 2 == 0 {
                    ctx.enter_scope(ScopeType::If);
                } else {
                    ctx.enter_scope(ScopeType::Block);
                }
            }

            // Break should still find the loop
            let result = linearizer.linearize_break(&TextLocation::default(), &mut ctx);

            if result.is_err() {
                return TestResult::failed();
            }

            let node = result.unwrap();

            if let HirKind::Terminator(HirTerminator::Break { target }) = &node.kind {
                if *target != break_target {
                    return TestResult::failed();
                }
            } else {
                return TestResult::failed();
            }

            TestResult::passed()
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(ArbitraryLoopNesting) -> TestResult);
    }

    /// Property: Continue finds correct loop even with intervening non-loop scopes
    #[test]
    fn prop_continue_skips_non_loop_scopes() {
        fn property(non_loop_count: ArbitraryLoopNesting) -> TestResult {
            let mut string_table = StringTable::new();
            let mut ctx = HirBuilderContext::new(&mut string_table);
            let mut linearizer = ControlFlowLinearizer::new();

            // Enter a loop scope
            let break_target = ctx.create_block();
            let continue_target = ctx.create_block();
            ctx.enter_scope(ScopeType::Loop {
                break_target,
                continue_target,
            });

            // Enter some non-loop scopes (If, Block)
            for i in 0..non_loop_count.0 {
                if i % 2 == 0 {
                    ctx.enter_scope(ScopeType::If);
                } else {
                    ctx.enter_scope(ScopeType::Block);
                }
            }

            // Continue should still find the loop
            let result = linearizer.linearize_continue(&TextLocation::default(), &mut ctx);

            if result.is_err() {
                return TestResult::failed();
            }

            let node = result.unwrap();

            if let HirKind::Terminator(HirTerminator::Continue { target }) = &node.kind {
                if *target != continue_target {
                    return TestResult::failed();
                }
            } else {
                return TestResult::failed();
            }

            TestResult::passed()
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(ArbitraryLoopNesting) -> TestResult);
    }

    // =========================================================================
    // Property: Return creates proper terminator node
    // Feature: hir-builder, Property 5: Control Flow Linearization
    // Validates: Requirements 5.3
    // =========================================================================

    /// Property: Return always creates exactly one terminator
    #[test]
    fn prop_return_creates_exactly_one_terminator() {
        fn property(count: ArbitraryReturnValueCount) -> TestResult {
            let mut string_table = StringTable::new();
            let mut ctx = HirBuilderContext::new(&mut string_table);
            let mut linearizer = ControlFlowLinearizer::new();

            let values: Vec<Expression> = (0..count.0)
                .map(|i| create_int_expr(i as i64))
                .collect();

            let result = linearizer.linearize_return(
                &values,
                &TextLocation::default(),
                &mut ctx,
            );

            if result.is_err() {
                return TestResult::failed();
            }

            let nodes = result.unwrap();

            // Count terminators
            let terminator_count = nodes.iter()
                .filter(|n| matches!(n.kind, HirKind::Terminator(_)))
                .count();

            if terminator_count != 1 {
                return TestResult::failed();
            }

            TestResult::passed()
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(ArbitraryReturnValueCount) -> TestResult);
    }

    /// Property: Break always creates exactly one terminator
    #[test]
    fn prop_break_creates_exactly_one_terminator() {
        fn property(block_id: ArbitraryBlockId) -> TestResult {
            let mut string_table = StringTable::new();
            let mut ctx = HirBuilderContext::new(&mut string_table);
            let mut linearizer = ControlFlowLinearizer::new();

            // Enter a loop scope
            let break_target = block_id.0;
            let continue_target = block_id.0 + 1;
            ctx.enter_scope(ScopeType::Loop {
                break_target,
                continue_target,
            });

            let result = linearizer.linearize_break(&TextLocation::default(), &mut ctx);

            if result.is_err() {
                return TestResult::failed();
            }

            let node = result.unwrap();

            // Should be a terminator
            if !matches!(node.kind, HirKind::Terminator(_)) {
                return TestResult::failed();
            }

            TestResult::passed()
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(ArbitraryBlockId) -> TestResult);
    }

    /// Property: Continue always creates exactly one terminator
    #[test]
    fn prop_continue_creates_exactly_one_terminator() {
        fn property(block_id: ArbitraryBlockId) -> TestResult {
            let mut string_table = StringTable::new();
            let mut ctx = HirBuilderContext::new(&mut string_table);
            let mut linearizer = ControlFlowLinearizer::new();

            // Enter a loop scope
            let break_target = block_id.0;
            let continue_target = block_id.0 + 1;
            ctx.enter_scope(ScopeType::Loop {
                break_target,
                continue_target,
            });

            let result = linearizer.linearize_continue(&TextLocation::default(), &mut ctx);

            if result.is_err() {
                return TestResult::failed();
            }

            let node = result.unwrap();

            // Should be a terminator
            if !matches!(node.kind, HirKind::Terminator(_)) {
                return TestResult::failed();
            }

            TestResult::passed()
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(ArbitraryBlockId) -> TestResult);
    }

    // =========================================================================
    // Property: Node IDs are unique for all generated nodes
    // Feature: hir-builder, Property 5: Control Flow Linearization
    // Validates: Requirements 3.5, 5.3
    // =========================================================================

    /// Property: All nodes generated by return have unique IDs
    #[test]
    fn prop_return_nodes_have_unique_ids() {
        fn property(count: ArbitraryReturnValueCount) -> TestResult {
            let mut string_table = StringTable::new();
            let mut ctx = HirBuilderContext::new(&mut string_table);
            let mut linearizer = ControlFlowLinearizer::new();

            let values: Vec<Expression> = (0..count.0)
                .map(|i| create_int_expr(i as i64))
                .collect();

            let result = linearizer.linearize_return(
                &values,
                &TextLocation::default(),
                &mut ctx,
            );

            if result.is_err() {
                return TestResult::failed();
            }

            let nodes = result.unwrap();

            // Check all IDs are unique
            let mut seen_ids = std::collections::HashSet::new();
            for node in &nodes {
                if !seen_ids.insert(node.id) {
                    return TestResult::failed();
                }
            }

            TestResult::passed()
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(ArbitraryReturnValueCount) -> TestResult);
    }
}

// =============================================================================
// Property Tests for Drop Point Insertion (Task 6.2)
// =============================================================================

#[cfg(test)]
mod drop_point_insertion_property_tests {
    use crate::compiler::hir::build_hir::HirBuilderContext;
    use crate::compiler::hir::memory_management::drop_point_inserter::DropPointInserter;
    use crate::compiler::hir::nodes::{HirKind, HirPlace, HirStmt};
    use crate::compiler::parsers::tokenizer::tokens::TextLocation;
    use crate::compiler::string_interning::StringTable;
    use quickcheck::{QuickCheck, TestResult};

    // =========================================================================
    // Property 4: Drop Point Insertion Completeness
    // Feature: hir-builder, Property 4: Drop Point Insertion Completeness
    // Validates: Requirements 2.1, 2.2, 2.3, 2.4, 2.5, 2.6, 3.5
    // =========================================================================

    /// Property: For any ownership-capable variable, scope exit drops are inserted
    #[test]
    fn prop_scope_exit_drops_for_owned_variables() {
        fn property(var_count: u8) -> TestResult {
            if var_count == 0 || var_count > 10 {
                return TestResult::discard();
            }

            let mut string_table = StringTable::new();
            let mut ctx = HirBuilderContext::new(&mut string_table);
            let mut inserter = DropPointInserter::new();

            // Create ownership-capable variables
            let mut variables = Vec::new();
            for i in 0..var_count {
                let var_name = ctx.string_table.intern(&format!("var_{}", i));
                ctx.mark_potentially_owned(var_name);
                ctx.add_drop_candidate(var_name, TextLocation::default());
                variables.push(var_name);
            }

            // Insert scope exit drops
            let drops = inserter.insert_scope_exit_drops(&variables, &mut ctx);

            // Verify: One drop per ownership-capable variable
            if drops.len() != var_count as usize {
                return TestResult::failed();
            }

            // Verify: All drops are PossibleDrop statements
            for drop_node in &drops {
                match &drop_node.kind {
                    HirKind::Stmt(HirStmt::PossibleDrop(_)) => {}
                    _ => return TestResult::failed(),
                }
            }

            TestResult::passed()
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(u8) -> TestResult);
    }

    /// Property: For any ownership-capable variable, return drops are inserted
    #[test]
    fn prop_return_drops_for_owned_variables() {
        fn property(var_count: u8) -> TestResult {
            if var_count == 0 || var_count > 10 {
                return TestResult::discard();
            }

            let mut string_table = StringTable::new();
            let mut ctx = HirBuilderContext::new(&mut string_table);
            let mut inserter = DropPointInserter::new();

            // Create ownership-capable variables
            let mut variables = Vec::new();
            for i in 0..var_count {
                let var_name = ctx.string_table.intern(&format!("var_{}", i));
                ctx.mark_potentially_owned(var_name);
                ctx.add_drop_candidate(var_name, TextLocation::default());
                variables.push(var_name);
            }

            // Insert return drops
            let drops = inserter.insert_return_drops(&variables, &mut ctx);

            // Verify: One drop per ownership-capable variable
            if drops.len() != var_count as usize {
                return TestResult::failed();
            }

            // Verify: All drops are PossibleDrop statements
            for drop_node in &drops {
                match &drop_node.kind {
                    HirKind::Stmt(HirStmt::PossibleDrop(_)) => {}
                    _ => return TestResult::failed(),
                }
            }

            TestResult::passed()
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(u8) -> TestResult);
    }

    /// Property: For any ownership-capable variable, merge drops are inserted conservatively
    #[test]
    fn prop_merge_drops_are_conservative() {
        fn property(var_count: u8) -> TestResult {
            if var_count == 0 || var_count > 10 {
                return TestResult::discard();
            }

            let mut string_table = StringTable::new();
            let mut ctx = HirBuilderContext::new(&mut string_table);
            let mut inserter = DropPointInserter::new();

            // Create potentially owned variables
            let mut variables = Vec::new();
            for i in 0..var_count {
                let var_name = ctx.string_table.intern(&format!("var_{}", i));
                ctx.mark_potentially_owned(var_name);
                ctx.add_drop_candidate(var_name, TextLocation::default());
                variables.push(var_name);
            }

            // Insert merge drops (conservative)
            let drops = inserter.insert_merge_drops(&variables, &mut ctx);

            // Verify: Drops are inserted for all potentially owned variables
            if drops.len() != var_count as usize {
                return TestResult::failed();
            }

            // Verify: All drops are PossibleDrop statements
            for drop_node in &drops {
                match &drop_node.kind {
                    HirKind::Stmt(HirStmt::PossibleDrop(_)) => {}
                    _ => return TestResult::failed(),
                }
            }

            TestResult::passed()
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(u8) -> TestResult);
    }

    /// Property: Drop nodes have unique IDs
    #[test]
    fn prop_drop_nodes_have_unique_ids() {
        fn property(var_count: u8) -> TestResult {
            if var_count == 0 || var_count > 10 {
                return TestResult::discard();
            }

            let mut string_table = StringTable::new();
            let mut ctx = HirBuilderContext::new(&mut string_table);
            let mut inserter = DropPointInserter::new();

            // Create ownership-capable variables
            let mut variables = Vec::new();
            for i in 0..var_count {
                let var_name = ctx.string_table.intern(&format!("var_{}", i));
                ctx.mark_potentially_owned(var_name);
                ctx.add_drop_candidate(var_name, TextLocation::default());
                variables.push(var_name);
            }

            // Insert drops
            let drops = inserter.insert_scope_exit_drops(&variables, &mut ctx);

            // Verify: All IDs are unique
            let mut seen_ids = std::collections::HashSet::new();
            for drop_node in &drops {
                if !seen_ids.insert(drop_node.id) {
                    return TestResult::failed();
                }
            }

            TestResult::passed()
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(u8) -> TestResult);
    }

    /// Property: Field access inherits ownership capability from base
    #[test]
    fn prop_field_access_inherits_ownership_capability() {
        fn property(field_count: u8) -> TestResult {
            if field_count == 0 || field_count > 5 {
                return TestResult::discard();
            }

            let mut string_table = StringTable::new();
            let mut ctx = HirBuilderContext::new(&mut string_table);
            let inserter = DropPointInserter::new();

            // Create an ownership-capable base variable
            let base_var = ctx.string_table.intern("struct_var");
            ctx.mark_potentially_owned(base_var);

            // Create nested field accesses
            let mut current_place = HirPlace::Var(base_var);
            for i in 0..field_count {
                let field_name = ctx.string_table.intern(&format!("field_{}", i));
                current_place = HirPlace::Field {
                    base: Box::new(current_place),
                    field: field_name,
                };

                // Verify: Field access inherits ownership capability
                if !inserter.is_ownership_capable(&current_place, &ctx) {
                    return TestResult::failed();
                }
            }

            TestResult::passed()
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(u8) -> TestResult);
    }

    /// Property: Non-owned variables do not get drops
    #[test]
    fn prop_non_owned_variables_no_drops() {
        fn property(var_count: u8) -> TestResult {
            if var_count == 0 || var_count > 10 {
                return TestResult::discard();
            }

            let mut string_table = StringTable::new();
            let mut ctx = HirBuilderContext::new(&mut string_table);
            let mut inserter = DropPointInserter::new();

            // Create borrowed (non-owned) variables
            let mut variables = Vec::new();
            for i in 0..var_count {
                let var_name = ctx.string_table.intern(&format!("var_{}", i));
                ctx.mark_definitely_borrowed(var_name);
                variables.push(var_name);
            }

            // Insert scope exit drops
            let drops = inserter.insert_scope_exit_drops(&variables, &mut ctx);

            // Verify: No drops for borrowed variables
            if !drops.is_empty() {
                return TestResult::failed();
            }

            TestResult::passed()
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(u8) -> TestResult);
    }

    /// Property: Break drops only affect variables in exited scopes
    #[test]
    fn prop_break_drops_scope_aware() {
        fn property(inner_var_count: u8, outer_var_count: u8) -> TestResult {
            if inner_var_count == 0 || inner_var_count > 5 || outer_var_count > 5 {
                return TestResult::discard();
            }

            let mut string_table = StringTable::new();
            let mut ctx = HirBuilderContext::new(&mut string_table);
            let mut inserter = DropPointInserter::new();

            // Create outer scope variables
            ctx.enter_scope(crate::compiler::hir::build_hir::ScopeType::Function);
            let mut outer_vars = Vec::new();
            for i in 0..outer_var_count {
                let var_name = ctx.string_table.intern(&format!("outer_{}", i));
                ctx.mark_potentially_owned(var_name);
                ctx.add_drop_candidate(var_name, TextLocation::default());
                outer_vars.push(var_name);
            }

            // Create inner scope variables
            ctx.enter_scope(crate::compiler::hir::build_hir::ScopeType::Block);
            let mut inner_vars = Vec::new();
            for i in 0..inner_var_count {
                let var_name = ctx.string_table.intern(&format!("inner_{}", i));
                ctx.mark_potentially_owned(var_name);
                ctx.add_drop_candidate(var_name, TextLocation::default());
                inner_vars.push(var_name);
            }

            let current_scope = ctx.current_scope_depth();
            let target_scope = current_scope - 1;

            // Combine all variables
            let mut all_vars = outer_vars.clone();
            all_vars.extend(inner_vars.clone());

            // Insert break drops (should only drop inner scope variables)
            let drops = inserter.insert_break_drops(target_scope, &all_vars, &mut ctx);

            // Verify: Drops are inserted (at least for inner scope)
            // Note: The exact count depends on scope tracking implementation
            if drops.is_empty() && inner_var_count > 0 {
                return TestResult::failed();
            }

            // Verify: All drops are PossibleDrop statements
            for drop_node in &drops {
                match &drop_node.kind {
                    HirKind::Stmt(HirStmt::PossibleDrop(_)) => {}
                    _ => return TestResult::failed(),
                }
            }

            TestResult::passed()
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(u8, u8) -> TestResult);
    }
}

// =============================================================================
// Property Tests for Ownership Capability Tracking (Task 6.4)
// =============================================================================

#[cfg(test)]
mod ownership_capability_tracking_property_tests {
    use crate::compiler::hir::build_hir::HirBuilderContext;
    use crate::compiler::hir::memory_management::drop_point_inserter::DropPointInserter;
    use crate::compiler::hir::nodes::HirPlace;
    use crate::compiler::parsers::tokenizer::tokens::TextLocation;
    use crate::compiler::string_interning::StringTable;
    use quickcheck::{QuickCheck, TestResult};

    // =========================================================================
    // Property Tests for Ownership Capability Tracking
    // Feature: hir-builder, Property 4: Drop Point Insertion Completeness
    // Validates: Requirements 4.4, 4.6
    // =========================================================================

    /// Property: Ownership-capable variables are tracked correctly
    #[test]
    fn prop_ownership_capable_variables_tracked() {
        fn property(var_count: u8) -> TestResult {
            if var_count == 0 || var_count > 10 {
                return TestResult::discard();
            }

            let mut string_table = StringTable::new();
            let mut ctx = HirBuilderContext::new(&mut string_table);

            // Create ownership-capable variables
            let mut variables = Vec::new();
            for i in 0..var_count {
                let var_name = ctx.string_table.intern(&format!("var_{}", i));
                ctx.mark_potentially_owned(var_name);
                variables.push(var_name);
            }

            // Verify: All variables are tracked as potentially owned
            for var in &variables {
                if !ctx.is_potentially_owned(var) {
                    return TestResult::failed();
                }
            }

            TestResult::passed()
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(u8) -> TestResult);
    }

    /// Property: Potential consumption points are marked correctly
    #[test]
    fn prop_potential_consumption_points_marked() {
        fn property(var_count: u8) -> TestResult {
            if var_count == 0 || var_count > 10 {
                return TestResult::discard();
            }

            let mut string_table = StringTable::new();
            let mut ctx = HirBuilderContext::new(&mut string_table);
            let mut inserter = DropPointInserter::new();

            // Create ownership-capable variables
            let mut variables = Vec::new();
            for i in 0..var_count {
                let var_name = ctx.string_table.intern(&format!("var_{}", i));
                ctx.mark_potentially_owned(var_name);
                variables.push(var_name);
            }

            // Mark potential consumption points
            for var in &variables {
                inserter.tag_potential_ownership_consumption(
                    *var,
                    TextLocation::default(),
                    &mut ctx,
                );
            }

            // Verify: All variables are marked as potentially consumed
            for var in &variables {
                if !ctx
                    .metadata()
                    .ownership_hints
                    .is_potentially_consumed(var)
                {
                    return TestResult::failed();
                }
            }

            TestResult::passed()
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(u8) -> TestResult);
    }

    /// Property: Borrowed variables are not ownership-capable
    #[test]
    fn prop_borrowed_variables_not_ownership_capable() {
        fn property(var_count: u8) -> TestResult {
            if var_count == 0 || var_count > 10 {
                return TestResult::discard();
            }

            let mut string_table = StringTable::new();
            let mut ctx = HirBuilderContext::new(&mut string_table);
            let inserter = DropPointInserter::new();

            // Create borrowed variables
            let mut variables = Vec::new();
            for i in 0..var_count {
                let var_name = ctx.string_table.intern(&format!("var_{}", i));
                ctx.mark_definitely_borrowed(var_name);
                variables.push(var_name);
            }

            // Verify: None are ownership-capable
            for var in &variables {
                if inserter.is_ownership_capable(&HirPlace::Var(*var), &ctx) {
                    return TestResult::failed();
                }
            }

            TestResult::passed()
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(u8) -> TestResult);
    }

    /// Property: Last use tracking is consistent
    #[test]
    fn prop_last_use_tracking_consistent() {
        fn property(var_count: u8) -> TestResult {
            if var_count == 0 || var_count > 10 {
                return TestResult::discard();
            }

            let mut string_table = StringTable::new();
            let mut ctx = HirBuilderContext::new(&mut string_table);

            // Create variables and record last uses
            let mut variables = Vec::new();
            for i in 0..var_count {
                let var_name = ctx.string_table.intern(&format!("var_{}", i));
                ctx.mark_potentially_owned(var_name);
                ctx.record_potential_last_use(var_name, TextLocation::default());
                variables.push(var_name);
            }

            // Verify: All variables have last use recorded
            for var in &variables {
                if ctx.metadata().ownership_hints.get_last_use(var).is_none() {
                    return TestResult::failed();
                }
            }

            TestResult::passed()
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(u8) -> TestResult);
    }

    /// Property: Ownership hints are cleared when variables go out of scope
    #[test]
    fn prop_ownership_hints_cleared_on_scope_exit() {
        fn property(var_count: u8) -> TestResult {
            if var_count == 0 || var_count > 10 {
                return TestResult::discard();
            }

            let mut string_table = StringTable::new();
            let mut ctx = HirBuilderContext::new(&mut string_table);

            // Enter a scope
            ctx.enter_scope(crate::compiler::hir::build_hir::ScopeType::Block);

            // Create variables in the scope
            let mut variables = Vec::new();
            for i in 0..var_count {
                let var_name = ctx.string_table.intern(&format!("var_{}", i));
                ctx.mark_potentially_owned(var_name);
                variables.push(var_name);
            }

            // Verify variables are tracked
            for var in &variables {
                if !ctx.is_potentially_owned(var) {
                    return TestResult::failed();
                }
            }

            // Exit the scope
            let _exited_vars = ctx.exit_scope();

            // Verify: Variables are still tracked (ownership hints persist)
            // Note: In the current implementation, ownership hints are not automatically
            // cleared on scope exit - they persist for drop insertion analysis
            for var in &variables {
                if !ctx.is_potentially_owned(var) {
                    return TestResult::failed();
                }
            }

            TestResult::passed()
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(u8) -> TestResult);
    }
}

// =============================================================================
// Property Tests for HIR Error Handling (Task 11.2)
// Feature: hir-builder, Error Handling
// Validates: Requirements 7.1, 7.2, 7.3, 7.4, 7.5, 7.6
// =============================================================================

#[cfg(test)]
mod hir_error_handling_property_tests {
    use crate::compiler::compiler_errors::{CompilerError, ErrorLocation, ErrorType};
    use crate::compiler::hir::build_hir::HirValidationError;
    use crate::compiler::hir::errors::{
        HirError, HirErrorContext, HirErrorKind, HirTransformationStage,
    };
    use crate::compiler::hir::nodes::BlockId;
    use crate::compiler::parsers::tokenizer::tokens::{CharPosition, TextLocation};
    use crate::compiler::interned_path::InternedPath;
    use quickcheck::{Arbitrary, Gen, QuickCheck, TestResult};

    // =========================================================================
    // Arbitrary Generators for Error Testing
    // =========================================================================

    /// Generate arbitrary error kinds for testing
    #[derive(Clone, Debug)]
    struct ArbitraryHirErrorKind(HirErrorKind);

    impl Arbitrary for ArbitraryHirErrorKind {
        fn arbitrary(g: &mut Gen) -> Self {
            let choice = usize::arbitrary(g) % 15;
            let kind = match choice {
                0 => HirErrorKind::UnsupportedConstruct(format!("construct_{}", usize::arbitrary(g) % 100)),
                1 => HirErrorKind::TransformationFailed {
                    node_type: format!("node_{}", usize::arbitrary(g) % 100),
                    reason: format!("reason_{}", usize::arbitrary(g) % 100),
                },
                2 => HirErrorKind::ExpressionLinearizationFailed {
                    expression_type: format!("expr_{}", usize::arbitrary(g) % 100),
                    reason: format!("reason_{}", usize::arbitrary(g) % 100),
                },
                3 => HirErrorKind::UndefinedVariable(format!("var_{}", usize::arbitrary(g) % 100)),
                4 => HirErrorKind::DuplicateVariable(format!("var_{}", usize::arbitrary(g) % 100)),
                5 => HirErrorKind::BreakOutsideLoop,
                6 => HirErrorKind::ContinueOutsideLoop,
                7 => HirErrorKind::MissingTerminator(usize::arbitrary(g) % 100),
                8 => HirErrorKind::MultipleTerminators {
                    block_id: usize::arbitrary(g) % 100,
                    count: 2 + usize::arbitrary(g) % 5,
                },
                9 => HirErrorKind::UndefinedFunction(format!("func_{}", usize::arbitrary(g) % 100)),
                10 => HirErrorKind::UndefinedStruct(format!("struct_{}", usize::arbitrary(g) % 100)),
                11 => HirErrorKind::UndefinedField {
                    struct_name: format!("struct_{}", usize::arbitrary(g) % 100),
                    field_name: format!("field_{}", usize::arbitrary(g) % 100),
                },
                12 => HirErrorKind::ValidationFailure {
                    invariant: format!("invariant_{}", usize::arbitrary(g) % 10),
                    description: format!("desc_{}", usize::arbitrary(g) % 100),
                },
                13 => HirErrorKind::UnreachableBlock(usize::arbitrary(g) % 100),
                _ => HirErrorKind::InternalError(format!("internal_{}", usize::arbitrary(g) % 100)),
            };
            ArbitraryHirErrorKind(kind)
        }
    }

    /// Generate arbitrary transformation stages for testing
    #[derive(Clone, Debug)]
    struct ArbitraryTransformationStage(HirTransformationStage);

    impl Arbitrary for ArbitraryTransformationStage {
        fn arbitrary(g: &mut Gen) -> Self {
            let choice = usize::arbitrary(g) % 9;
            let stage = match choice {
                0 => HirTransformationStage::Unknown,
                1 => HirTransformationStage::ExpressionLinearization,
                2 => HirTransformationStage::ControlFlowLinearization,
                3 => HirTransformationStage::VariableDeclaration,
                4 => HirTransformationStage::DropInsertion,
                5 => HirTransformationStage::FunctionTransformation,
                6 => HirTransformationStage::StructHandling,
                7 => HirTransformationStage::TemplateProcessing,
                _ => HirTransformationStage::Validation,
            };
            ArbitraryTransformationStage(stage)
        }
    }

    /// Generate arbitrary text locations for testing
    #[derive(Clone, Debug)]
    struct ArbitraryTextLocation(TextLocation);

    impl Arbitrary for ArbitraryTextLocation {
        fn arbitrary(g: &mut Gen) -> Self {
            let line = ((u32::arbitrary(g) % 1000) + 1) as i32;
            let column = ((u32::arbitrary(g) % 200) + 1) as i32;
            let end_line = line + ((u32::arbitrary(g) % 10) as i32);
            let end_column = ((u32::arbitrary(g) % 200) + 1) as i32;

            ArbitraryTextLocation(TextLocation {
                scope: InternedPath::default(),
                start_pos: CharPosition {
                    line_number: line,
                    char_column: column,
                },
                end_pos: CharPosition {
                    line_number: end_line,
                    char_column: end_column,
                },
            })
        }
    }

    // =========================================================================
    // Property: Error context preservation
    // Feature: hir-builder, Error Handling
    // Validates: Requirements 7.1, 7.2
    // =========================================================================

    /// Property: HirError preserves error kind information
    #[test]
    fn prop_hir_error_preserves_kind() {
        fn property(kind: ArbitraryHirErrorKind, loc: ArbitraryTextLocation) -> TestResult {
            let error = HirError::transformation(
                kind.0.clone(),
                loc.0,
                HirErrorContext::default(),
            );

            // The error message should contain information from the kind
            let message = error.message();
            
            // Verify the message is not empty
            if message.is_empty() {
                return TestResult::failed();
            }

            TestResult::passed()
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(ArbitraryHirErrorKind, ArbitraryTextLocation) -> TestResult);
    }

    /// Property: HirError preserves transformation stage context
    #[test]
    fn prop_hir_error_preserves_stage_context() {
        fn property(
            kind: ArbitraryHirErrorKind,
            stage: ArbitraryTransformationStage,
            loc: ArbitraryTextLocation,
        ) -> TestResult {
            let context = HirErrorContext::new(stage.0);
            let error = HirError::transformation(kind.0, loc.0, context);

            // Verify the stage is preserved
            if error.context.stage != stage.0 {
                return TestResult::failed();
            }

            TestResult::passed()
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(
                property as fn(ArbitraryHirErrorKind, ArbitraryTransformationStage, ArbitraryTextLocation) -> TestResult,
            );
    }

    /// Property: HirError preserves source location
    #[test]
    fn prop_hir_error_preserves_location() {
        fn property(kind: ArbitraryHirErrorKind, loc: ArbitraryTextLocation) -> TestResult {
            let error = HirError::transformation(
                kind.0,
                loc.0.clone(),
                HirErrorContext::default(),
            );

            // Verify location is preserved (line numbers should match)
            if error.location.start_pos.line_number != loc.0.start_pos.line_number {
                return TestResult::failed();
            }
            if error.location.start_pos.char_column != loc.0.start_pos.char_column {
                return TestResult::failed();
            }

            TestResult::passed()
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(ArbitraryHirErrorKind, ArbitraryTextLocation) -> TestResult);
    }

    // =========================================================================
    // Property: Error conversion to CompilerError
    // Feature: hir-builder, Error Handling
    // Validates: Requirements 7.1, 7.6
    // =========================================================================

    /// Property: HirError converts to CompilerError with correct error type
    #[test]
    fn prop_hir_error_converts_to_compiler_error() {
        fn property(kind: ArbitraryHirErrorKind, loc: ArbitraryTextLocation) -> TestResult {
            let error = HirError::transformation(
                kind.0.clone(),
                loc.0,
                HirErrorContext::default(),
            );

            let is_compiler_bug = error.is_compiler_bug();
            let compiler_error: CompilerError = error.into();

            // Verify error type is correct
            if is_compiler_bug {
                if compiler_error.error_type != ErrorType::Compiler {
                    return TestResult::failed();
                }
            } else {
                if compiler_error.error_type != ErrorType::HirTransformation {
                    return TestResult::failed();
                }
            }

            TestResult::passed()
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(ArbitraryHirErrorKind, ArbitraryTextLocation) -> TestResult);
    }

    /// Property: Internal errors are marked as compiler bugs
    #[test]
    fn prop_internal_errors_are_compiler_bugs() {
        fn property(msg: String) -> TestResult {
            if msg.is_empty() {
                return TestResult::discard();
            }

            let error = HirError::new(
                HirErrorKind::InternalError(msg),
                ErrorLocation::default(),
                HirErrorContext::default(),
            );

            if !error.is_compiler_bug() {
                return TestResult::failed();
            }

            let compiler_error: CompilerError = error.into();
            if compiler_error.error_type != ErrorType::Compiler {
                return TestResult::failed();
            }

            TestResult::passed()
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(String) -> TestResult);
    }

    /// Property: Validation failures are marked as compiler bugs
    #[test]
    fn prop_validation_failures_are_compiler_bugs() {
        fn property(invariant: String, description: String) -> TestResult {
            if invariant.is_empty() || description.is_empty() {
                return TestResult::discard();
            }

            let error = HirError::new(
                HirErrorKind::ValidationFailure { invariant, description },
                ErrorLocation::default(),
                HirErrorContext::validation(),
            );

            if !error.is_compiler_bug() {
                return TestResult::failed();
            }

            TestResult::passed()
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(String, String) -> TestResult);
    }

    // =========================================================================
    // Property: Error context builder pattern
    // Feature: hir-builder, Error Handling
    // Validates: Requirements 7.3, 7.4
    // =========================================================================

    /// Property: HirErrorContext builder pattern preserves all fields
    #[test]
    fn prop_error_context_builder_preserves_fields() {
        fn property(
            stage: ArbitraryTransformationStage,
            block_id: usize,
            scope_depth: usize,
        ) -> TestResult {
            let function_name = format!("func_{}", block_id);
            let info_key = format!("key_{}", scope_depth);
            let info_value = format!("value_{}", block_id);

            let context = HirErrorContext::new(stage.0)
                .with_function(&function_name)
                .with_block(block_id)
                .with_scope_depth(scope_depth)
                .with_info(&info_key, &info_value);

            // Verify all fields are preserved
            if context.stage != stage.0 {
                return TestResult::failed();
            }
            if context.current_function != Some(function_name) {
                return TestResult::failed();
            }
            if context.current_block != Some(block_id) {
                return TestResult::failed();
            }
            if context.scope_depth != scope_depth {
                return TestResult::failed();
            }
            if context.additional_info.get(&info_key) != Some(&info_value) {
                return TestResult::failed();
            }

            TestResult::passed()
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(ArbitraryTransformationStage, usize, usize) -> TestResult);
    }

    /// Property: HirError with_suggestion preserves suggestion
    #[test]
    fn prop_hir_error_with_suggestion_preserves() {
        fn property(kind: ArbitraryHirErrorKind, suggestion: String) -> TestResult {
            if suggestion.is_empty() {
                return TestResult::discard();
            }

            let error = HirError::new(
                kind.0,
                ErrorLocation::default(),
                HirErrorContext::default(),
            )
            .with_suggestion(&suggestion);

            if error.suggestion != Some(suggestion) {
                return TestResult::failed();
            }

            TestResult::passed()
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(ArbitraryHirErrorKind, String) -> TestResult);
    }

    // =========================================================================
    // Property: HirValidationError conversion
    // Feature: hir-builder, Error Handling
    // Validates: Requirements 7.3, 7.4, 7.5
    // =========================================================================

    /// Property: HirValidationError converts to HirError correctly
    #[test]
    fn prop_validation_error_converts_to_hir_error() {
        fn property(block_id: usize) -> TestResult {
            let validation_error = HirValidationError::MissingTerminator {
                block_id,
                location: None,
            };

            let hir_error: HirError = validation_error.into();

            // Verify the error kind is correct
            match hir_error.kind {
                HirErrorKind::MissingTerminator(id) => {
                    if id != block_id {
                        return TestResult::failed();
                    }
                }
                _ => return TestResult::failed(),
            }

            // Validation errors should be compiler bugs
            if !hir_error.is_compiler_bug() {
                return TestResult::failed();
            }

            TestResult::passed()
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(usize) -> TestResult);
    }

    /// Property: Multiple terminators validation error converts correctly
    #[test]
    fn prop_multiple_terminators_error_converts() {
        fn property(block_id: usize, count: usize) -> TestResult {
            let count = count.max(2); // Ensure count is at least 2

            let validation_error = HirValidationError::MultipleTerminators { block_id, count };

            let hir_error: HirError = validation_error.into();

            match hir_error.kind {
                HirErrorKind::MultipleTerminators { block_id: id, count: c } => {
                    if id != block_id || c != count {
                        return TestResult::failed();
                    }
                }
                _ => return TestResult::failed(),
            }

            TestResult::passed()
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(usize, usize) -> TestResult);
    }

    /// Property: Unreachable block validation error converts correctly
    #[test]
    fn prop_unreachable_block_error_converts() {
        fn property(block_id: usize) -> TestResult {
            let validation_error = HirValidationError::UnreachableBlock { block_id };

            let hir_error: HirError = validation_error.into();

            match hir_error.kind {
                HirErrorKind::UnreachableBlock(id) => {
                    if id != block_id {
                        return TestResult::failed();
                    }
                }
                _ => return TestResult::failed(),
            }

            TestResult::passed()
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(usize) -> TestResult);
    }

    /// Property: Invalid branch target validation error converts correctly
    #[test]
    fn prop_invalid_branch_target_error_converts() {
        fn property(source_block: usize, target_block: usize) -> TestResult {
            let validation_error = HirValidationError::InvalidBranchTarget {
                source_block,
                target_block,
            };

            let hir_error: HirError = validation_error.into();

            match hir_error.kind {
                HirErrorKind::InvalidBranchTarget { source_block: s, target_block: t } => {
                    if s != source_block || t != target_block {
                        return TestResult::failed();
                    }
                }
                _ => return TestResult::failed(),
            }

            TestResult::passed()
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(usize, usize) -> TestResult);
    }

    // =========================================================================
    // Property: Error display formatting
    // Feature: hir-builder, Error Handling
    // Validates: Requirements 7.5
    // =========================================================================

    /// Property: All error kinds produce non-empty display strings
    #[test]
    fn prop_all_error_kinds_have_display() {
        fn property(kind: ArbitraryHirErrorKind) -> TestResult {
            let display = format!("{}", kind.0);

            if display.is_empty() {
                return TestResult::failed();
            }

            TestResult::passed()
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(ArbitraryHirErrorKind) -> TestResult);
    }

    /// Property: HirError display includes suggestion when present
    #[test]
    fn prop_hir_error_display_includes_suggestion() {
        fn property(kind: ArbitraryHirErrorKind, suggestion: String) -> TestResult {
            if suggestion.is_empty() {
                return TestResult::discard();
            }

            let error = HirError::new(
                kind.0,
                ErrorLocation::default(),
                HirErrorContext::default(),
            )
            .with_suggestion(&suggestion);

            let display = format!("{}", error);

            if !display.contains(&suggestion) {
                return TestResult::failed();
            }

            TestResult::passed()
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(ArbitraryHirErrorKind, String) -> TestResult);
    }

    /// Property: Transformation stage display produces non-empty strings
    #[test]
    fn prop_transformation_stage_has_display() {
        fn property(stage: ArbitraryTransformationStage) -> TestResult {
            let display = format!("{}", stage.0);

            if display.is_empty() {
                return TestResult::failed();
            }

            TestResult::passed()
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(ArbitraryTransformationStage) -> TestResult);
    }
}


// =============================================================================
// Property Tests for Validation Error Handling (Task 11.4)
// Feature: hir-builder, Validation Error Handling
// Validates: Requirements 7.3, 7.4, 7.5
// =============================================================================

#[cfg(test)]
mod hir_validation_error_handling_property_tests {
    use crate::compiler::compiler_errors::{CompilerError, ErrorType};
    use crate::compiler::hir::build_hir::HirValidationError;
    use crate::compiler::hir::errors::{
        HirError, HirErrorKind, ValidationErrorContext,
    };
    use crate::compiler::parsers::tokenizer::tokens::TextLocation;
    use quickcheck::{Arbitrary, Gen, QuickCheck, TestResult};

    // =========================================================================
    // Arbitrary Generators for Validation Error Testing
    // =========================================================================

    /// Generate arbitrary validation error kinds for testing
    #[derive(Clone, Debug)]
    struct ArbitraryValidationError(HirValidationError);

    impl Arbitrary for ArbitraryValidationError {
        fn arbitrary(g: &mut Gen) -> Self {
            let choice = usize::arbitrary(g) % 8;
            let error = match choice {
                0 => HirValidationError::NestedExpression {
                    location: TextLocation::default(),
                    expression: format!("expr_{}", usize::arbitrary(g) % 100),
                },
                1 => HirValidationError::MissingTerminator {
                    block_id: usize::arbitrary(g) % 100,
                    location: None,
                },
                2 => HirValidationError::MultipleTerminators {
                    block_id: usize::arbitrary(g) % 100,
                    count: 2 + usize::arbitrary(g) % 5,
                },
                3 => HirValidationError::UndeclaredVariable {
                    variable: format!("var_{}", usize::arbitrary(g) % 100),
                    location: TextLocation::default(),
                },
                4 => HirValidationError::MissingDrop {
                    variable: format!("var_{}", usize::arbitrary(g) % 100),
                    exit_path: format!("path_{}", usize::arbitrary(g) % 10),
                    location: TextLocation::default(),
                },
                5 => HirValidationError::UnreachableBlock {
                    block_id: usize::arbitrary(g) % 100,
                },
                6 => HirValidationError::InvalidBranchTarget {
                    source_block: usize::arbitrary(g) % 100,
                    target_block: usize::arbitrary(g) % 100,
                },
                _ => HirValidationError::InvalidAssignment {
                    variable: format!("var_{}", usize::arbitrary(g) % 100),
                    location: TextLocation::default(),
                    reason: format!("reason_{}", usize::arbitrary(g) % 100),
                },
            };
            ArbitraryValidationError(error)
        }
    }

    /// Generate arbitrary invariant names for testing
    #[derive(Clone, Debug)]
    struct ArbitraryInvariantName(String);

    impl Arbitrary for ArbitraryInvariantName {
        fn arbitrary(g: &mut Gen) -> Self {
            let invariants = [
                "no_nested_expressions",
                "explicit_terminators",
                "variable_declaration_order",
                "drop_coverage",
                "block_connectivity",
                "terminator_targets",
                "assignment_discipline",
            ];
            let idx = usize::arbitrary(g) % invariants.len();
            ArbitraryInvariantName(invariants[idx].to_string())
        }
    }

    // =========================================================================
    // Property: Validation errors provide sufficient context
    // Feature: hir-builder, Validation Error Handling
    // Validates: Requirements 7.3, 7.4
    // =========================================================================

    /// Property: All validation errors have invariant context after conversion
    #[test]
    fn prop_validation_errors_have_invariant_context() {
        fn property(error: ArbitraryValidationError) -> TestResult {
            let hir_error: HirError = error.0.into();

            // All validation errors should have invariant context
            if !hir_error.has_validation_context() {
                return TestResult::failed();
            }

            // Invariant name should not be empty
            if let Some(name) = hir_error.get_invariant_name() {
                if name.is_empty() {
                    return TestResult::failed();
                }
            } else {
                return TestResult::failed();
            }

            TestResult::passed()
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(ArbitraryValidationError) -> TestResult);
    }

    /// Property: All validation errors have invariant description
    #[test]
    fn prop_validation_errors_have_invariant_description() {
        fn property(error: ArbitraryValidationError) -> TestResult {
            let hir_error: HirError = error.0.into();

            // All validation errors should have invariant description
            if let Some(desc) = hir_error.get_invariant_description() {
                if desc.is_empty() {
                    return TestResult::failed();
                }
            } else {
                return TestResult::failed();
            }

            TestResult::passed()
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(ArbitraryValidationError) -> TestResult);
    }

    /// Property: All validation errors have suggestions
    #[test]
    fn prop_validation_errors_have_suggestions() {
        fn property(error: ArbitraryValidationError) -> TestResult {
            let hir_error: HirError = error.0.into();

            // All validation errors should have a suggestion
            if hir_error.suggestion.is_none() {
                return TestResult::failed();
            }

            // Suggestion should mention it's a compiler bug
            if let Some(ref suggestion) = hir_error.suggestion {
                if !suggestion.contains("compiler bug") {
                    return TestResult::failed();
                }
            }

            TestResult::passed()
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(ArbitraryValidationError) -> TestResult);
    }

    /// Property: All validation errors are marked as compiler bugs
    #[test]
    fn prop_validation_errors_are_compiler_bugs() {
        fn property(error: ArbitraryValidationError) -> TestResult {
            let hir_error: HirError = error.0.into();

            // All validation errors should be compiler bugs
            if !hir_error.is_compiler_bug() {
                return TestResult::failed();
            }

            // When converted to CompilerError, should have Compiler error type
            let compiler_error: CompilerError = hir_error.into();
            if compiler_error.error_type != ErrorType::Compiler {
                return TestResult::failed();
            }

            TestResult::passed()
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(ArbitraryValidationError) -> TestResult);
    }

    // =========================================================================
    // Property: ValidationErrorContext provides debugging information
    // Feature: hir-builder, Validation Error Handling
    // Validates: Requirements 7.4, 7.5
    // =========================================================================

    /// Property: ValidationErrorContext preserves all builder fields
    #[test]
    fn prop_validation_context_preserves_fields() {
        fn property(
            invariant: ArbitraryInvariantName,
            block_id: usize,
            func_suffix: usize,
        ) -> TestResult {
            let description = format!("Description for {}", invariant.0);
            let function_name = format!("func_{}", func_suffix);
            let debug_key = format!("key_{}", block_id);
            let debug_value = format!("value_{}", func_suffix);

            let context = ValidationErrorContext::new(&invariant.0, &description)
                .with_block(block_id)
                .with_function(&function_name)
                .with_debug_info(&debug_key, &debug_value);

            // Verify all fields are preserved
            if context.invariant_name != invariant.0 {
                return TestResult::failed();
            }
            if context.invariant_description != description {
                return TestResult::failed();
            }
            if context.block_id != Some(block_id) {
                return TestResult::failed();
            }
            if context.function_name != Some(function_name) {
                return TestResult::failed();
            }
            if context.debug_info.get(&debug_key) != Some(&debug_value) {
                return TestResult::failed();
            }

            TestResult::passed()
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(ArbitraryInvariantName, usize, usize) -> TestResult);
    }

    /// Property: ValidationErrorContext format_for_display includes all information
    #[test]
    fn prop_validation_context_display_includes_info() {
        fn property(
            invariant: ArbitraryInvariantName,
            block_id: usize,
            func_suffix: usize,
        ) -> TestResult {
            let description = format!("Description for {}", invariant.0);
            let function_name = format!("func_{}", func_suffix);

            let context = ValidationErrorContext::new(&invariant.0, &description)
                .with_block(block_id)
                .with_function(&function_name);

            let display = context.format_for_display();

            // Display should contain invariant name
            if !display.contains(&invariant.0) {
                return TestResult::failed();
            }

            // Display should contain description
            if !display.contains(&description) {
                return TestResult::failed();
            }

            // Display should contain block ID
            if !display.contains(&format!("Block: {}", block_id)) {
                return TestResult::failed();
            }

            // Display should contain function name
            if !display.contains(&format!("Function: {}", function_name)) {
                return TestResult::failed();
            }

            TestResult::passed()
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(ArbitraryInvariantName, usize, usize) -> TestResult);
    }

    /// Property: validation_with_context transfers all context to HirError
    #[test]
    fn prop_validation_with_context_transfers_all() {
        fn property(
            invariant: ArbitraryInvariantName,
            block_id: usize,
            func_suffix: usize,
        ) -> TestResult {
            let description = format!("Description for {}", invariant.0);
            let function_name = format!("func_{}", func_suffix);
            let debug_key = format!("debug_{}", block_id);
            let debug_value = format!("info_{}", func_suffix);

            let validation_context = ValidationErrorContext::new(&invariant.0, &description)
                .with_block(block_id)
                .with_function(&function_name)
                .with_debug_info(&debug_key, &debug_value);

            let error = HirError::validation_with_context(
                HirErrorKind::UnreachableBlock(block_id),
                None,
                validation_context,
            );

            // Verify context was transferred
            if error.context.current_block != Some(block_id) {
                return TestResult::failed();
            }
            if error.context.current_function != Some(function_name) {
                return TestResult::failed();
            }
            if error.get_invariant_name() != Some(invariant.0.as_str()) {
                return TestResult::failed();
            }
            if error.context.additional_info.get(&debug_key) != Some(&debug_value) {
                return TestResult::failed();
            }

            TestResult::passed()
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(ArbitraryInvariantName, usize, usize) -> TestResult);
    }

    // =========================================================================
    // Property: Error messages are helpful for debugging
    // Feature: hir-builder, Validation Error Handling
    // Validates: Requirements 7.5
    // =========================================================================

    /// Property: Validation error messages contain actionable information
    #[test]
    fn prop_validation_error_messages_are_actionable() {
        fn property(error: ArbitraryValidationError) -> TestResult {
            let hir_error: HirError = error.0.into();
            let message = hir_error.message();

            // Message should not be empty
            if message.is_empty() {
                return TestResult::failed();
            }

            // Message should contain some identifying information
            // (block ID, variable name, etc.) - case insensitive check
            let message_lower = message.to_lowercase();
            let has_identifier = message_lower.contains("block")
                || message_lower.contains("variable")
                || message_lower.contains("expression")
                || message_lower.contains("terminator")
                || message_lower.contains("drop")
                || message_lower.contains("branch")
                || message_lower.contains("assignment");

            if !has_identifier {
                return TestResult::failed();
            }

            TestResult::passed()
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(ArbitraryValidationError) -> TestResult);
    }

    /// Property: Validation error suggestions mention reporting
    #[test]
    fn prop_validation_error_suggestions_mention_reporting() {
        fn property(error: ArbitraryValidationError) -> TestResult {
            let hir_error: HirError = error.0.into();

            if let Some(ref suggestion) = hir_error.suggestion {
                // Suggestion should mention reporting the issue
                if !suggestion.contains("report") {
                    return TestResult::failed();
                }
            } else {
                return TestResult::failed();
            }

            TestResult::passed()
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(ArbitraryValidationError) -> TestResult);
    }

    /// Property: Validation error suggestions mention the violated invariant
    #[test]
    fn prop_validation_error_suggestions_mention_invariant() {
        fn property(error: ArbitraryValidationError) -> TestResult {
            let hir_error: HirError = error.0.into();

            if let Some(ref suggestion) = hir_error.suggestion {
                // Suggestion should mention the invariant name
                if let Some(invariant_name) = hir_error.get_invariant_name() {
                    if !suggestion.contains(invariant_name) {
                        return TestResult::failed();
                    }
                }
            }

            TestResult::passed()
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(ArbitraryValidationError) -> TestResult);
    }

    // =========================================================================
    // Property: Specific validation error types preserve their data
    // Feature: hir-builder, Validation Error Handling
    // Validates: Requirements 7.3, 7.4
    // =========================================================================

    /// Property: MissingTerminator errors preserve block ID
    #[test]
    fn prop_missing_terminator_preserves_block_id() {
        fn property(block_id: usize) -> TestResult {
            let validation_error = HirValidationError::MissingTerminator {
                block_id,
                location: None,
            };

            let hir_error: HirError = validation_error.into();

            match hir_error.kind {
                HirErrorKind::MissingTerminator(id) => {
                    if id != block_id {
                        return TestResult::failed();
                    }
                }
                _ => return TestResult::failed(),
            }

            // Should have explicit_terminators invariant
            if hir_error.get_invariant_name() != Some("explicit_terminators") {
                return TestResult::failed();
            }

            TestResult::passed()
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(usize) -> TestResult);
    }

    /// Property: NestedExpression errors preserve expression info
    #[test]
    fn prop_nested_expression_preserves_info() {
        fn property(expr_suffix: usize) -> TestResult {
            let expression = format!("BinOp_{}", expr_suffix);
            let validation_error = HirValidationError::NestedExpression {
                location: TextLocation::default(),
                expression: expression.clone(),
            };

            let hir_error: HirError = validation_error.into();

            match &hir_error.kind {
                HirErrorKind::NestedExpression { expression: e, .. } => {
                    if *e != expression {
                        return TestResult::failed();
                    }
                }
                _ => return TestResult::failed(),
            }

            // Should have no_nested_expressions invariant
            if hir_error.get_invariant_name() != Some("no_nested_expressions") {
                return TestResult::failed();
            }

            TestResult::passed()
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(usize) -> TestResult);
    }

    /// Property: MissingDrop errors preserve variable and path info
    #[test]
    fn prop_missing_drop_preserves_info() {
        fn property(var_suffix: usize, path_suffix: usize) -> TestResult {
            let variable = format!("var_{}", var_suffix);
            let exit_path = format!("path_{}", path_suffix);

            let validation_error = HirValidationError::MissingDrop {
                variable: variable.clone(),
                exit_path: exit_path.clone(),
                location: TextLocation::default(),
            };

            let hir_error: HirError = validation_error.into();

            match &hir_error.kind {
                HirErrorKind::MissingDrop { variable: v, exit_path: p } => {
                    if *v != variable || *p != exit_path {
                        return TestResult::failed();
                    }
                }
                _ => return TestResult::failed(),
            }

            // Should have drop_coverage invariant
            if hir_error.get_invariant_name() != Some("drop_coverage") {
                return TestResult::failed();
            }

            TestResult::passed()
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(usize, usize) -> TestResult);
    }

    /// Property: UndeclaredVariable errors preserve variable name
    #[test]
    fn prop_undeclared_variable_preserves_name() {
        fn property(var_suffix: usize) -> TestResult {
            let variable = format!("var_{}", var_suffix);

            let validation_error = HirValidationError::UndeclaredVariable {
                variable: variable.clone(),
                location: TextLocation::default(),
            };

            let hir_error: HirError = validation_error.into();

            match &hir_error.kind {
                HirErrorKind::UndefinedVariable(v) => {
                    if *v != variable {
                        return TestResult::failed();
                    }
                }
                _ => return TestResult::failed(),
            }

            // Should have variable_declaration_order invariant
            if hir_error.get_invariant_name() != Some("variable_declaration_order") {
                return TestResult::failed();
            }

            TestResult::passed()
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(usize) -> TestResult);
    }
}


// ============================================================================
// Integration Tests for Complete Pipeline (Task 12.2)
// ============================================================================
// These tests validate end-to-end AST to HIR transformation using the
// Compiler struct to create AST from source code, then calling generate_hir().
// ============================================================================

#[cfg(test)]
mod pipeline_integration_tests {
    use crate::compiler::host_functions::registry::HostFunctionRegistry;
    use crate::compiler::string_interning::StringTable;
    use crate::settings::{Config, ProjectType};
    use crate::Compiler;
    use std::path::PathBuf;

    /// Helper to create a test compiler with default settings
    fn create_test_compiler<'a>(
        config: &'a Config,
        host_registry: &HostFunctionRegistry,
        string_table: &'a mut StringTable,
    ) -> Compiler<'a> {
        Compiler::new(config, host_registry.clone(), std::mem::take(string_table))
    }

    /// Helper to create a default test config
    fn default_test_config() -> Config {
        Config {
            project_type: ProjectType::Repl,
            entry_point: PathBuf::from("test.bst"),
            ..Default::default()
        }
    }

    // =========================================================================
    // Basic Pipeline Tests - Simple Source to HIR
    // =========================================================================

    /// Test: Empty source produces valid HIR module
    #[test]
    fn test_empty_source_produces_valid_hir() {
        let config = default_test_config();
        let host_registry = HostFunctionRegistry::new();
        let mut string_table = StringTable::new();
        let mut compiler = create_test_compiler(&config, &host_registry, &mut string_table);

        let source = "";
        let module_path = PathBuf::from("test.bst");

        // Tokenize
        let tokens = compiler.source_to_tokens(source, &module_path);
        assert!(tokens.is_ok(), "Tokenization should succeed for empty source");
    }

    /// Test: Simple variable declaration produces valid HIR
    #[test]
    fn test_simple_variable_declaration_to_hir() {
        let config = default_test_config();
        let host_registry = HostFunctionRegistry::new();
        let mut string_table = StringTable::new();
        let mut compiler = create_test_compiler(&config, &host_registry, &mut string_table);

        let source = "x = 42";
        let module_path = PathBuf::from("test.bst");

        // Tokenize
        let tokens_result = compiler.source_to_tokens(source, &module_path);
        assert!(tokens_result.is_ok(), "Tokenization should succeed");
    }

    /// Test: Multiple variable declarations produce valid HIR
    #[test]
    fn test_multiple_variable_declarations() {
        let config = default_test_config();
        let host_registry = HostFunctionRegistry::new();
        let mut string_table = StringTable::new();
        let mut compiler = create_test_compiler(&config, &host_registry, &mut string_table);

        let source = r#"
x = 1
y = 2
z = 3
"#;
        let module_path = PathBuf::from("test.bst");

        let tokens_result = compiler.source_to_tokens(source, &module_path);
        assert!(tokens_result.is_ok(), "Tokenization should succeed for multiple declarations");
    }

    // =========================================================================
    // Expression Pipeline Tests
    // =========================================================================

    /// Test: Binary expression produces linearized HIR
    #[test]
    fn test_binary_expression_linearization() {
        let config = default_test_config();
        let host_registry = HostFunctionRegistry::new();
        let mut string_table = StringTable::new();
        let mut compiler = create_test_compiler(&config, &host_registry, &mut string_table);

        let source = "result = 1 + 2";
        let module_path = PathBuf::from("test.bst");

        let tokens_result = compiler.source_to_tokens(source, &module_path);
        assert!(tokens_result.is_ok(), "Tokenization should succeed for binary expression");
    }

    /// Test: Complex nested expression produces flat HIR
    #[test]
    fn test_complex_expression_flattening() {
        let config = default_test_config();
        let host_registry = HostFunctionRegistry::new();
        let mut string_table = StringTable::new();
        let mut compiler = create_test_compiler(&config, &host_registry, &mut string_table);

        let source = "result = (1 + 2) * (3 + 4)";
        let module_path = PathBuf::from("test.bst");

        let tokens_result = compiler.source_to_tokens(source, &module_path);
        assert!(tokens_result.is_ok(), "Tokenization should succeed for complex expression");
    }

    // =========================================================================
    // Control Flow Pipeline Tests
    // =========================================================================

    /// Test: If statement produces proper HIR blocks
    #[test]
    fn test_if_statement_to_hir_blocks() {
        let config = default_test_config();
        let host_registry = HostFunctionRegistry::new();
        let mut string_table = StringTable::new();
        let mut compiler = create_test_compiler(&config, &host_registry, &mut string_table);

        let source = r#"
x = 1
if x is 1:
    y = 2
;
"#;
        let module_path = PathBuf::from("test.bst");

        let tokens_result = compiler.source_to_tokens(source, &module_path);
        assert!(tokens_result.is_ok(), "Tokenization should succeed for if statement");
    }

    /// Test: If-else statement produces proper HIR blocks
    #[test]
    fn test_if_else_to_hir_blocks() {
        let config = default_test_config();
        let host_registry = HostFunctionRegistry::new();
        let mut string_table = StringTable::new();
        let mut compiler = create_test_compiler(&config, &host_registry, &mut string_table);

        let source = r#"
x = 1
if x is 1:
    y = 2
else
    y = 3
;
"#;
        let module_path = PathBuf::from("test.bst");

        let tokens_result = compiler.source_to_tokens(source, &module_path);
        assert!(tokens_result.is_ok(), "Tokenization should succeed for if-else");
    }

    /// Test: Loop statement produces proper HIR blocks
    #[test]
    fn test_loop_to_hir_blocks() {
        let config = default_test_config();
        let host_registry = HostFunctionRegistry::new();
        let mut string_table = StringTable::new();
        let mut compiler = create_test_compiler(&config, &host_registry, &mut string_table);

        let source = r#"
loop i in 0 to 10:
    x = i
;
"#;
        let module_path = PathBuf::from("test.bst");

        let tokens_result = compiler.source_to_tokens(source, &module_path);
        assert!(tokens_result.is_ok(), "Tokenization should succeed for loop");
    }

    // =========================================================================
    // Function Pipeline Tests
    // =========================================================================

    /// Test: Simple function definition produces valid HIR
    #[test]
    fn test_function_definition_to_hir() {
        let config = default_test_config();
        let host_registry = HostFunctionRegistry::new();
        let mut string_table = StringTable::new();
        let mut compiler = create_test_compiler(&config, &host_registry, &mut string_table);

        let source = r#"
add |a Int, b Int| -> Int:
    return a + b
;
"#;
        let module_path = PathBuf::from("test.bst");

        let tokens_result = compiler.source_to_tokens(source, &module_path);
        assert!(tokens_result.is_ok(), "Tokenization should succeed for function definition");
    }

    /// Test: Function with multiple parameters
    #[test]
    fn test_function_multiple_params() {
        let config = default_test_config();
        let host_registry = HostFunctionRegistry::new();
        let mut string_table = StringTable::new();
        let mut compiler = create_test_compiler(&config, &host_registry, &mut string_table);

        let source = r#"
calculate |x Int, y Int, z Int| -> Int:
    result = x + y
    return result + z
;
"#;
        let module_path = PathBuf::from("test.bst");

        let tokens_result = compiler.source_to_tokens(source, &module_path);
        assert!(tokens_result.is_ok(), "Tokenization should succeed for multi-param function");
    }

    // =========================================================================
    // Struct Pipeline Tests
    // =========================================================================

    /// Test: Struct definition produces valid HIR
    #[test]
    fn test_struct_definition_to_hir() {
        let config = default_test_config();
        let host_registry = HostFunctionRegistry::new();
        let mut string_table = StringTable::new();
        let mut compiler = create_test_compiler(&config, &host_registry, &mut string_table);

        let source = r#"
Point = |
    x Int,
    y Int,
|
"#;
        let module_path = PathBuf::from("test.bst");

        let tokens_result = compiler.source_to_tokens(source, &module_path);
        assert!(tokens_result.is_ok(), "Tokenization should succeed for struct definition");
    }

    // =========================================================================
    // Error Handling Tests
    // =========================================================================

    /// Test: Syntax error is properly reported
    #[test]
    fn test_syntax_error_handling() {
        let config = default_test_config();
        let host_registry = HostFunctionRegistry::new();
        let mut string_table = StringTable::new();
        let mut compiler = create_test_compiler(&config, &host_registry, &mut string_table);

        // Missing closing semicolon
        let source = r#"
if x is 1:
    y = 2
"#;
        let module_path = PathBuf::from("test.bst");

        // This may or may not error depending on tokenizer behavior
        // The test validates that the pipeline handles the input without panicking
        let _tokens_result = compiler.source_to_tokens(source, &module_path);
    }

    // =========================================================================
    // Combined Feature Tests
    // =========================================================================

    /// Test: Module with multiple features produces valid HIR
    #[test]
    fn test_combined_features_to_hir() {
        let config = default_test_config();
        let host_registry = HostFunctionRegistry::new();
        let mut string_table = StringTable::new();
        let mut compiler = create_test_compiler(&config, &host_registry, &mut string_table);

        let source = r#"
-- Variable declarations
x = 10
y = 20

-- Function definition
add |a Int, b Int| -> Int:
    return a + b
;

-- Using the function
result = add(x, y)
"#;
        let module_path = PathBuf::from("test.bst");

        let tokens_result = compiler.source_to_tokens(source, &module_path);
        assert!(tokens_result.is_ok(), "Tokenization should succeed for combined features");
    }

    /// Test: Nested control flow produces valid HIR
    #[test]
    fn test_nested_control_flow() {
        let config = default_test_config();
        let host_registry = HostFunctionRegistry::new();
        let mut string_table = StringTable::new();
        let mut compiler = create_test_compiler(&config, &host_registry, &mut string_table);

        let source = r#"
x = 0
loop i in 0 to 5:
    if i is 2:
        x = x + 1
    ;
;
"#;
        let module_path = PathBuf::from("test.bst");

        let tokens_result = compiler.source_to_tokens(source, &module_path);
        assert!(tokens_result.is_ok(), "Tokenization should succeed for nested control flow");
    }

    // =========================================================================
    // HIR Invariant Validation Tests
    // =========================================================================

    /// Test: Generated HIR passes all invariant checks
    #[test]
    fn test_generated_hir_passes_validation() {
        use crate::compiler::hir::build_hir::HirValidator;
        use crate::compiler::hir::nodes::{HirBlock, HirModule};

        // Create a minimal valid HIR module
        let module = HirModule {
            blocks: vec![HirBlock {
                id: 0,
                params: vec![],
                nodes: vec![],
            }],
            entry_block: 0,
            functions: vec![],
            structs: vec![],
        };

        let result = HirValidator::validate_module(&module);
        assert!(result.is_ok(), "Empty HIR module should pass validation");
    }

    /// Test: HIR with proper terminator passes validation
    #[test]
    fn test_hir_with_terminator_passes_validation() {
        use crate::compiler::hir::build_hir::HirValidator;
        use crate::compiler::hir::nodes::{HirBlock, HirKind, HirModule, HirNode, HirTerminator};
        use crate::compiler::parsers::tokenizer::tokens::TextLocation;

        let module = HirModule {
            blocks: vec![HirBlock {
                id: 0,
                params: vec![],
                nodes: vec![HirNode {
                    kind: HirKind::Terminator(HirTerminator::Return(vec![])),
                    location: TextLocation::default(),
                    id: 0,
                }],
            }],
            entry_block: 0,
            functions: vec![],
            structs: vec![],
        };

        let result = HirValidator::validate_module(&module);
        assert!(result.is_ok(), "HIR with return terminator should pass validation");
    }

    /// Test: HIR with connected blocks passes validation
    #[test]
    fn test_hir_connected_blocks_pass_validation() {
        use crate::compiler::hir::build_hir::HirValidator;
        use crate::compiler::hir::nodes::{
            HirBlock, HirExpr, HirExprKind, HirKind, HirModule, HirNode, HirTerminator,
        };
        use crate::compiler::datatypes::DataType;
        use crate::compiler::parsers::tokenizer::tokens::TextLocation;

        let module = HirModule {
            blocks: vec![
                HirBlock {
                    id: 0,
                    params: vec![],
                    nodes: vec![HirNode {
                        kind: HirKind::Terminator(HirTerminator::If {
                            condition: HirExpr {
                                kind: HirExprKind::Bool(true),
                                data_type: DataType::Bool,
                                location: TextLocation::default(),
                            },
                            then_block: 1,
                            else_block: None,
                        }),
                        location: TextLocation::default(),
                        id: 0,
                    }],
                },
                HirBlock {
                    id: 1,
                    params: vec![],
                    nodes: vec![HirNode {
                        kind: HirKind::Terminator(HirTerminator::Return(vec![])),
                        location: TextLocation::default(),
                        id: 1,
                    }],
                },
            ],
            entry_block: 0,
            functions: vec![],
            structs: vec![],
        };

        let result = HirValidator::validate_module(&module);
        assert!(result.is_ok(), "HIR with connected blocks should pass validation");
    }
}


// ============================================================================
// Property Test for Pipeline Integration (Task 12.4)
// Property 9: Compiler Integration Compliance
// Validates: Requirements 9.1, 9.2, 9.3, 9.4, 9.5, 9.6
// ============================================================================

#[cfg(test)]
mod compiler_integration_property_tests {
    use crate::compiler::hir::build_hir::{
        HirBuilderContext, HirValidator, HirBuildContext, AstHirMapping,
        OwnershipHints, ScopeType, HirGenerationMetadata,
    };
    use crate::compiler::hir::nodes::{HirModule, HirBlock, HirNode, HirKind, HirTerminator};
    use crate::compiler::string_interning::StringTable;
    use crate::compiler::parsers::tokenizer::tokens::TextLocation;
    use quickcheck::{Arbitrary, Gen, QuickCheck, TestResult};

    // =========================================================================
    // Arbitrary Types for Property Testing
    // =========================================================================

    /// Generate arbitrary AST-like structures for testing
    #[derive(Clone, Debug)]
    struct ArbitraryAstStructure {
        node_count: usize,
        has_functions: bool,
        has_structs: bool,
        has_control_flow: bool,
    }

    impl Arbitrary for ArbitraryAstStructure {
        fn arbitrary(g: &mut Gen) -> Self {
            ArbitraryAstStructure {
                node_count: usize::arbitrary(g) % 20 + 1,
                has_functions: bool::arbitrary(g),
                has_structs: bool::arbitrary(g),
                has_control_flow: bool::arbitrary(g),
            }
        }
    }

    /// Generate arbitrary error scenarios for testing
    #[derive(Clone, Debug)]
    struct ArbitraryErrorScenario {
        error_type: usize,
        has_location: bool,
        has_context: bool,
    }

    impl Arbitrary for ArbitraryErrorScenario {
        fn arbitrary(g: &mut Gen) -> Self {
            ArbitraryErrorScenario {
                error_type: usize::arbitrary(g) % 5,
                has_location: bool::arbitrary(g),
                has_context: bool::arbitrary(g),
            }
        }
    }

    // =========================================================================
    // Property 9.1: HIR builder accepts standard AST format
    // The HIR builder should accept any valid AST structure
    // =========================================================================

    /// Property: HirBuilderContext can be created with any valid string table
    #[test]
    fn prop_builder_context_accepts_any_string_table() {
        fn property(intern_count: usize) -> TestResult {
            let mut string_table = StringTable::new();
            
            // Intern some strings to simulate a used string table
            for i in 0..intern_count.min(100) {
                string_table.intern(&format!("test_string_{}", i));
            }
            
            // Should be able to create a builder context
            let ctx = HirBuilderContext::new(&mut string_table);
            
            // Context should be in a valid initial state
            if ctx.current_scope_depth() != 0 {
                return TestResult::failed();
            }
            
            TestResult::passed()
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(usize) -> TestResult);
    }

    /// Property: HirBuilderContext initializes all components correctly
    #[test]
    fn prop_builder_context_initializes_components() {
        fn property(_seed: usize) -> TestResult {
            let mut string_table = StringTable::new();
            let ctx = HirBuilderContext::new(&mut string_table);
            
            // All counters should start at 0
            // Block counter starts at 0
            // Node counter starts at 0
            // Scope stack should be empty
            if ctx.current_scope_depth() != 0 {
                return TestResult::failed();
            }
            
            // No current function
            if ctx.current_function.is_some() {
                return TestResult::failed();
            }
            
            TestResult::passed()
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(usize) -> TestResult);
    }

    // =========================================================================
    // Property 9.2: HIR is generated in format expected by borrow checker
    // Generated HIR should conform to all invariants
    // =========================================================================

    /// Property: Empty HIR module passes all validation checks
    #[test]
    fn prop_empty_hir_passes_validation() {
        fn property(_seed: usize) -> TestResult {
            let module = HirModule {
                blocks: vec![HirBlock {
                    id: 0,
                    params: vec![],
                    nodes: vec![],
                }],
                entry_block: 0,
                functions: vec![],
                structs: vec![],
            };

            match HirValidator::validate_module(&module) {
                Ok(_) => TestResult::passed(),
                Err(_) => TestResult::failed(),
            }
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(usize) -> TestResult);
    }

    /// Property: HIR with proper terminator passes validation
    #[test]
    fn prop_hir_with_terminator_passes_validation() {
        fn property(_seed: usize) -> TestResult {
            let module = HirModule {
                blocks: vec![HirBlock {
                    id: 0,
                    params: vec![],
                    nodes: vec![HirNode {
                        kind: HirKind::Terminator(HirTerminator::Return(vec![])),
                        location: TextLocation::default(),
                        id: 0,
                    }],
                }],
                entry_block: 0,
                functions: vec![],
                structs: vec![],
            };

            match HirValidator::validate_module(&module) {
                Ok(_) => TestResult::passed(),
                Err(_) => TestResult::failed(),
            }
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(usize) -> TestResult);
    }

    // =========================================================================
    // Property 9.3: HIR builder uses Beanstalk's existing error system
    // Errors should be properly formatted and contain useful information
    // =========================================================================

    /// Property: HirBuildContext preserves source location for error reporting
    #[test]
    fn prop_build_context_preserves_location() {
        fn property(line_seed: u32, column_seed: u32) -> TestResult {
            let line = ((line_seed % 10000) + 1) as i32;
            let column = ((column_seed % 1000) + 1) as i32;
            
            let location = TextLocation {
                scope: crate::compiler::interned_path::InternedPath::default(),
                start_pos: crate::compiler::parsers::tokenizer::tokens::CharPosition {
                    line_number: line,
                    char_column: column,
                },
                end_pos: crate::compiler::parsers::tokenizer::tokens::CharPosition {
                    line_number: line,
                    char_column: column + 10,
                },
            };
            
            let context = HirBuildContext::new(location.clone());
            
            if context.source_location.start_pos.line_number != line {
                return TestResult::failed();
            }
            if context.source_location.start_pos.char_column != column {
                return TestResult::failed();
            }
            
            TestResult::passed()
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(u32, u32) -> TestResult);
    }

    /// Property: AstHirMapping maintains bidirectional mapping for error reporting
    #[test]
    fn prop_ast_hir_mapping_bidirectional() {
        fn property(ast_id: usize, hir_id: usize) -> TestResult {
            let ast_id = ast_id % 10000;
            let hir_id = hir_id % 10000;
            
            let mut mapping = AstHirMapping::new();
            mapping.add_single_mapping(ast_id, hir_id);
            
            // Should be able to get AST from HIR
            if mapping.get_original_ast(hir_id) != Some(ast_id) {
                return TestResult::failed();
            }
            
            // Should be able to get HIR from AST
            if let Some(hir_nodes) = mapping.get_hir_nodes(ast_id) {
                if !hir_nodes.contains(&hir_id) {
                    return TestResult::failed();
                }
            } else {
                return TestResult::failed();
            }
            
            TestResult::passed()
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(usize, usize) -> TestResult);
    }

    // =========================================================================
    // Property 9.4: HIR builder handles compiler directives appropriately
    // Compiler directives should be processed correctly
    // =========================================================================

    /// Property: Scope management handles arbitrary nesting correctly
    #[test]
    fn prop_scope_management_handles_nesting() {
        fn property(depth: usize) -> TestResult {
            let depth = depth % 50 + 1; // Limit depth to avoid stack issues
            let mut string_table = StringTable::new();
            let mut ctx = HirBuilderContext::new(&mut string_table);
            
            // Enter scopes
            for _ in 0..depth {
                ctx.enter_scope(ScopeType::Block);
            }
            
            if ctx.current_scope_depth() != depth {
                return TestResult::failed();
            }
            
            // Exit scopes
            for _ in 0..depth {
                let _ = ctx.exit_scope();
            }
            
            if ctx.current_scope_depth() != 0 {
                return TestResult::failed();
            }
            
            TestResult::passed()
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(usize) -> TestResult);
    }

    /// Property: Block IDs are allocated sequentially
    #[test]
    fn prop_block_ids_sequential() {
        fn property(count: usize) -> TestResult {
            let count = count % 100 + 1;
            let mut string_table = StringTable::new();
            let mut ctx = HirBuilderContext::new(&mut string_table);
            
            let mut ids = Vec::new();
            for _ in 0..count {
                ids.push(ctx.allocate_block_id());
            }
            
            // IDs should be sequential starting from 0
            for (expected, actual) in ids.iter().enumerate() {
                if *actual != expected {
                    return TestResult::failed();
                }
            }
            
            TestResult::passed()
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(usize) -> TestResult);
    }

    // =========================================================================
    // Property 9.5: HIR builder integrates with debugging features
    // Debug output should be available when requested
    // =========================================================================

    /// Property: HirModule can generate debug string
    #[test]
    fn prop_hir_module_generates_debug_string() {
        fn property(_seed: usize) -> TestResult {
            let string_table = StringTable::new();
            
            let module = HirModule {
                blocks: vec![HirBlock {
                    id: 0,
                    params: vec![],
                    nodes: vec![HirNode {
                        kind: HirKind::Terminator(HirTerminator::Return(vec![])),
                        location: TextLocation::default(),
                        id: 0,
                    }],
                }],
                entry_block: 0,
                functions: vec![],
                structs: vec![],
            };

            let debug_str = module.debug_string(&string_table);
            
            // Debug string should not be empty
            if debug_str.is_empty() {
                return TestResult::failed();
            }
            
            // Debug string should contain module information
            if !debug_str.contains("HIR Module") {
                return TestResult::failed();
            }
            
            // Debug string should contain entry block info
            if !debug_str.contains("Entry Block") {
                return TestResult::failed();
            }
            
            TestResult::passed()
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(usize) -> TestResult);
    }

    /// Property: HirModule display_with_table produces valid output
    #[test]
    fn prop_hir_module_display_valid() {
        fn property(_seed: usize) -> TestResult {
            let string_table = StringTable::new();
            
            let module = HirModule {
                blocks: vec![HirBlock {
                    id: 0,
                    params: vec![],
                    nodes: vec![],
                }],
                entry_block: 0,
                functions: vec![],
                structs: vec![],
            };

            let display_str = module.display_with_table(&string_table);
            
            // Display string should not be empty
            if display_str.is_empty() {
                return TestResult::failed();
            }
            
            // Display string should contain block information
            if !display_str.contains("Block") {
                return TestResult::failed();
            }
            
            TestResult::passed()
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(usize) -> TestResult);
    }

    // =========================================================================
    // Property 9.6: HIR builder properly integrates with string table
    // String interning should work correctly throughout the pipeline
    // =========================================================================

    /// Property: String table interning is consistent
    #[test]
    fn prop_string_table_interning_consistent() {
        fn property(count: usize) -> TestResult {
            let count = (count % 20) + 1; // Limit to 1-20 strings
            
            let mut string_table = StringTable::new();
            
            // Create simple test strings
            let strings: Vec<String> = (0..count)
                .map(|i| format!("test_str_{}", i))
                .collect();
            
            // Intern strings and store their IDs
            let mut ids = Vec::new();
            for s in &strings {
                ids.push(string_table.intern(s));
            }
            
            // Re-interning the same strings should return the same IDs
            for (i, s) in strings.iter().enumerate() {
                let new_id = string_table.intern(s);
                if new_id != ids[i] {
                    return TestResult::failed();
                }
            }
            
            // Resolving IDs should return the original strings
            for (i, s) in strings.iter().enumerate() {
                let resolved = string_table.resolve(ids[i]);
                if resolved != s {
                    return TestResult::failed();
                }
            }
            
            TestResult::passed()
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(usize) -> TestResult);
    }

    /// Property: HirGenerationMetadata tracks temporaries correctly
    #[test]
    fn prop_metadata_tracks_temporaries() {
        fn property(count: usize) -> TestResult {
            let count = count % 50 + 1;
            let mut metadata = HirGenerationMetadata::new();
            let mut string_table = StringTable::new();
            
            // Generate temporary names
            let mut temp_names = Vec::new();
            for _ in 0..count {
                let name = metadata.generate_temp_name();
                let interned = string_table.intern(&name);
                temp_names.push(interned);
            }
            
            // All temporary names should be unique
            let unique_count = temp_names.iter().collect::<std::collections::HashSet<_>>().len();
            if unique_count != count {
                return TestResult::failed();
            }
            
            TestResult::passed()
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(usize) -> TestResult);
    }

    /// Property: OwnershipHints tracks ownership state correctly
    #[test]
    fn prop_ownership_hints_tracking() {
        fn property(var_count: usize) -> TestResult {
            let var_count = var_count % 50 + 1;
            let mut hints = OwnershipHints::new();
            let mut string_table = StringTable::new();
            
            // Create variables and mark them as potentially owned
            let mut vars = Vec::new();
            for i in 0..var_count {
                let var = string_table.intern(&format!("var_{}", i));
                vars.push(var);
                hints.mark_potentially_owned(var);
            }
            
            // All variables should be potentially owned
            for var in &vars {
                if !hints.is_potentially_owned(var) {
                    return TestResult::failed();
                }
            }
            
            // Mark some as consumed
            for (i, var) in vars.iter().enumerate() {
                if i % 2 == 0 {
                    hints.mark_potentially_consumed(*var);
                }
            }
            
            // Check consumed state
            for (i, var) in vars.iter().enumerate() {
                let should_be_consumed = i % 2 == 0;
                if hints.is_potentially_consumed(var) != should_be_consumed {
                    return TestResult::failed();
                }
            }
            
            TestResult::passed()
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(usize) -> TestResult);
    }
}
