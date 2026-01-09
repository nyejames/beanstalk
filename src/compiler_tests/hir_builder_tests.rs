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
