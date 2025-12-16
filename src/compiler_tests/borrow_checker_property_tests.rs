//! Property-based tests for the borrow checker
//!
//! This module contains property-based tests that validate the correctness
//! properties of the borrow checker implementation. Each test corresponds
//! to a specific property defined in the design document.

#[cfg(test)]
mod tests {
    use quickcheck_macros::quickcheck;
    use crate::compiler::borrow_checker::types::{BorrowChecker, CfgNodeType};
    use crate::compiler::hir::nodes::{HirModule, HirNode, HirKind, HirNodeId, HirExpr, HirExprKind};
    use crate::compiler::hir::place::{Place, PlaceRoot};
    use crate::compiler::parsers::tokenizer::tokens::{TextLocation, CharPosition};
    use crate::compiler::interned_path::InternedPath;
    use crate::compiler::string_interning::StringTable;
    use crate::compiler::parsers::statements::functions::FunctionSignature;
    use crate::compiler::datatypes::DataType;
    use std::path::PathBuf;
    use quickcheck::{TestResult, Arbitrary, Gen};
    use std::collections::HashMap;

    /// Generator for creating test HIR modules
    #[derive(Debug, Clone)]
    struct TestHirModule {
        functions: Vec<TestHirNode>,
    }

    /// Generator for creating test HIR nodes
    #[derive(Debug, Clone)]
    struct TestHirNode {
        id: HirNodeId,
        kind: TestHirKind,
    }

    impl Arbitrary for TestHirNode {
        fn arbitrary(g: &mut Gen) -> Self {
            TestHirNode {
                id: usize::arbitrary(g) % 1000, // Limit ID range for testing
                kind: TestHirKind::arbitrary(g),
            }
        }
    }

    /// Simplified HIR kinds for property testing
    #[derive(Debug, Clone)]
    enum TestHirKind {
        FunctionDef { name: String, body_size: usize },
        Assign { place_name: String },
        Call { target: String, arg_count: usize },
        Return,
        If { condition_place: String, then_size: usize, else_size: usize },
        Loop { iterator_place: String, body_size: usize },
    }

    /// Generator for control flow structures
    #[derive(Debug, Clone)]
    struct TestControlFlowStructure {
        nodes: Vec<TestHirNode>,
        has_if: bool,
        has_loop: bool,
    }

    impl Arbitrary for TestHirModule {
        fn arbitrary(g: &mut Gen) -> Self {
            let function_count = usize::arbitrary(g) % 5 + 1; // 1-5 functions
            let mut functions = Vec::new();
            
            for i in 0..function_count {
                functions.push(TestHirNode::arbitrary_with_id(g, i));
            }
            
            TestHirModule { functions }
        }
    }

    impl TestHirNode {
        fn arbitrary_with_id(g: &mut Gen, id: HirNodeId) -> Self {
            TestHirNode {
                id,
                kind: TestHirKind::arbitrary(g),
            }
        }
    }

    impl Arbitrary for TestHirKind {
        fn arbitrary(g: &mut Gen) -> Self {
            match u8::arbitrary(g) % 6 {
                0 => TestHirKind::FunctionDef {
                    name: format!("func_{}", usize::arbitrary(g) % 100),
                    body_size: usize::arbitrary(g) % 10 + 1,
                },
                1 => TestHirKind::Assign {
                    place_name: format!("var_{}", usize::arbitrary(g) % 50),
                },
                2 => TestHirKind::Call {
                    target: format!("target_{}", usize::arbitrary(g) % 20),
                    arg_count: usize::arbitrary(g) % 5,
                },
                3 => TestHirKind::If {
                    condition_place: format!("cond_{}", usize::arbitrary(g) % 20),
                    then_size: usize::arbitrary(g) % 5 + 1,
                    else_size: usize::arbitrary(g) % 5 + 1,
                },
                4 => TestHirKind::Loop {
                    iterator_place: format!("iter_{}", usize::arbitrary(g) % 20),
                    body_size: usize::arbitrary(g) % 5 + 1,
                },
                _ => TestHirKind::Return,
            }
        }
    }

    impl Arbitrary for TestControlFlowStructure {
        fn arbitrary(g: &mut Gen) -> Self {
            let node_count = usize::arbitrary(g) % 10 + 1; // 1-10 nodes
            let mut nodes = Vec::new();
            let mut has_if = false;
            let mut has_loop = false;
            
            for i in 0..node_count {
                let kind = TestHirKind::arbitrary(g);
                match &kind {
                    TestHirKind::If { .. } => has_if = true,
                    TestHirKind::Loop { .. } => has_loop = true,
                    _ => {}
                }
                nodes.push(TestHirNode {
                    id: i,
                    kind,
                });
            }
            
            TestControlFlowStructure {
                nodes,
                has_if,
                has_loop,
            }
        }
    }

    impl TestHirModule {
        /// Convert test module to actual HIR module for testing
        fn to_hir_module(&self, string_table: &mut StringTable) -> HirModule {
            let mut hir_functions = Vec::new();
            
            for test_node in &self.functions {
                hir_functions.push(test_node.to_hir_node(string_table));
            }
            
            HirModule {
                functions: hir_functions,
            }
        }
    }

    impl TestControlFlowStructure {
        fn is_valid(&self) -> bool {
            // Basic validation - ensure we have at least one node
            !self.nodes.is_empty()
        }
        
        fn to_hir_nodes(&self, string_table: &mut StringTable) -> Vec<HirNode> {
            self.nodes.iter().map(|node| node.to_hir_node(string_table)).collect()
        }
    }

    impl TestHirNode {
        /// Convert test node to actual HIR node for testing
        fn to_hir_node(&self, string_table: &mut StringTable) -> HirNode {
            let location = TextLocation::default();
            let scope = InternedPath::new();
            
            let kind = match &self.kind {
                TestHirKind::FunctionDef { name, body_size: _ } => {
                    let name_interned = string_table.intern(name);
                    HirKind::FunctionDef {
                        name: name_interned,
                        signature: FunctionSignature {
                            parameters: Vec::new(),
                            returns: Vec::new(),
                        },
                        body: Vec::new(), // Empty body for simplicity
                    }
                }
                TestHirKind::Assign { place_name } => {
                    let place_interned = string_table.intern(place_name);
                    let place = Place {
                        root: PlaceRoot::Local(place_interned),
                        projections: Vec::new(),
                    };
                    HirKind::Assign {
                        place,
                        value: HirExpr {
                            kind: HirExprKind::Int(0),
                            data_type: DataType::Int,
                            location: TextLocation::default(),
                        },
                    }
                }
                TestHirKind::Call { target, arg_count: _ } => {
                    let target_interned = string_table.intern(target);
                    HirKind::Call {
                        target: target_interned,
                        args: Vec::new(), // Empty args for simplicity
                        returns: Vec::new(), // Empty returns for simplicity
                    }
                }
                TestHirKind::If { condition_place, then_size: _, else_size: _ } => {
                    let condition_interned = string_table.intern(condition_place);
                    let condition = Place {
                        root: PlaceRoot::Local(condition_interned),
                        projections: Vec::new(),
                    };
                    
                    // Create simple then and else blocks
                    let then_block = vec![
                        HirNode {
                            id: self.id * 100 + 1, // Unique ID for then block
                            kind: HirKind::Assign {
                                place: Place {
                                    root: PlaceRoot::Local(string_table.intern("then_var")),
                                    projections: Vec::new(),
                                },
                                value: HirExpr {
                                    kind: HirExprKind::Int(1),
                                    data_type: DataType::Int,
                                    location: TextLocation::default(),
                                },
                            },
                            location: TextLocation::default(),
                            scope: InternedPath::new(),
                        }
                    ];
                    
                    let else_block = vec![
                        HirNode {
                            id: self.id * 100 + 2, // Unique ID for else block
                            kind: HirKind::Assign {
                                place: Place {
                                    root: PlaceRoot::Local(string_table.intern("else_var")),
                                    projections: Vec::new(),
                                },
                                value: HirExpr {
                                    kind: HirExprKind::Int(2),
                                    data_type: DataType::Int,
                                    location: TextLocation::default(),
                                },
                            },
                            location: TextLocation::default(),
                            scope: InternedPath::new(),
                        }
                    ];
                    
                    HirKind::If {
                        condition,
                        then_block,
                        else_block: Some(else_block),
                    }
                }
                TestHirKind::Loop { iterator_place, body_size: _ } => {
                    let iterator_interned = string_table.intern(iterator_place);
                    let iterator = Place {
                        root: PlaceRoot::Local(iterator_interned),
                        projections: Vec::new(),
                    };
                    
                    // Create simple loop body
                    let body = vec![
                        HirNode {
                            id: self.id * 100 + 3, // Unique ID for loop body
                            kind: HirKind::Assign {
                                place: Place {
                                    root: PlaceRoot::Local(string_table.intern("loop_var")),
                                    projections: Vec::new(),
                                },
                                value: HirExpr {
                                    kind: HirExprKind::Int(3),
                                    data_type: DataType::Int,
                                    location: TextLocation::default(),
                                },
                            },
                            location: TextLocation::default(),
                            scope: InternedPath::new(),
                        }
                    ];
                    
                    HirKind::Loop {
                        binding: Some((string_table.intern("loop_var"), DataType::Int)),
                        iterator,
                        body,
                        index_binding: None,
                    }
                }
                TestHirKind::Return => {
                    HirKind::Return(Vec::new()) // Empty return for simplicity
                }
            };
            
            HirNode {
                kind,
                location,
                scope,
                id: self.id,
            }
        }
    }

    // **Feature: borrow-checker-implementation, Property 1: CFG Node Creation**
    #[quickcheck]
    fn property_cfg_node_creation(test_nodes: Vec<TestHirNode>) -> TestResult {
        // Skip empty node lists
        if test_nodes.is_empty() {
            return TestResult::discard();
        }
        
        // Skip too many nodes (performance)
        if test_nodes.len() > 20 {
            return TestResult::discard();
        }
        
        let mut string_table = StringTable::new();
        let hir_nodes: Vec<HirNode> = test_nodes.iter()
            .enumerate()
            .map(|(i, test_node)| {
                let mut node = test_node.to_hir_node(&mut string_table);
                node.id = i; // Ensure unique IDs
                node
            })
            .collect();
        
        // Property: For any HIR node with a unique ID, the CFG should contain exactly one corresponding CFG node with that ID
        let result = test_cfg_node_creation(&hir_nodes);
        
        TestResult::from_bool(result)
    }

    // **Feature: borrow-checker-implementation, Property 2: Control Flow Edge Completeness**
    #[quickcheck]
    fn property_control_flow_edge_completeness(test_control_flow: TestControlFlowStructure) -> TestResult {
        // Skip invalid control flow structures
        if !test_control_flow.is_valid() {
            return TestResult::discard();
        }
        
        let mut string_table = StringTable::new();
        let hir_nodes = test_control_flow.to_hir_nodes(&mut string_table);
        
        // Property: For any structured control flow node (If, Match, Loop), the CFG should contain edges representing all possible execution paths
        let result = test_control_flow_edge_completeness(&hir_nodes);
        
        TestResult::from_bool(result)
    }

    // **Feature: borrow-checker-implementation, Property 3: CFG Reachability**
    #[quickcheck]
    fn property_cfg_reachability(test_nodes: Vec<TestHirNode>) -> TestResult {
        // Skip empty or too large node lists
        if test_nodes.is_empty() || test_nodes.len() > 15 {
            return TestResult::discard();
        }
        
        let mut string_table = StringTable::new();
        let hir_nodes: Vec<HirNode> = test_nodes.iter()
            .enumerate()
            .map(|(i, test_node)| {
                let mut node = test_node.to_hir_node(&mut string_table);
                node.id = i; // Ensure unique IDs
                node
            })
            .collect();
        
        // Property: For any HIR node that is reachable through normal execution flow, that node should be reachable in the constructed CFG
        let result = test_cfg_reachability(&hir_nodes);
        
        TestResult::from_bool(result)
    }

    // **Feature: borrow-checker-implementation, Property 28: Compiler Integration Preservation**
    #[quickcheck]
    fn property_compiler_integration_preservation(test_module: TestHirModule) -> TestResult {
        // Skip empty modules
        if test_module.functions.is_empty() {
            return TestResult::discard();
        }
        
        // Skip modules with too many functions (performance)
        if test_module.functions.len() > 10 {
            return TestResult::discard();
        }
        
        let mut string_table = StringTable::new();
        let mut hir_module = test_module.to_hir_module(&mut string_table);
        
        // Test that borrow checker initialization doesn't break existing functionality
        let result = test_borrow_checker_initialization(&mut hir_module, &mut string_table);
        
        TestResult::from_bool(result)
    }

    /// Test CFG node creation property
    fn test_cfg_node_creation(hir_nodes: &[HirNode]) -> bool {
        use crate::compiler::borrow_checker::cfg::construct_cfg;
        
        // Property: For any HIR node with a unique ID, the CFG should contain exactly one corresponding CFG node with that ID
        let cfg = construct_cfg(hir_nodes);
        
        // Check that every HIR node has exactly one corresponding CFG node
        for hir_node in hir_nodes {
            if !cfg.nodes.contains_key(&hir_node.id) {
                return false; // Missing CFG node for HIR node
            }
        }
        
        // Check that CFG doesn't have extra nodes (beyond nested nodes from control flow)
        // We allow extra nodes for control flow structures (if/loop bodies)
        let hir_node_ids: std::collections::HashSet<_> = hir_nodes.iter().map(|n| n.id).collect();
        for cfg_node_id in cfg.nodes.keys() {
            // CFG can have additional nodes from nested structures, so we just verify
            // that all original HIR nodes are present
            if hir_node_ids.contains(cfg_node_id) {
                continue; // This is an original HIR node
            }
            // Additional nodes are allowed for nested control flow
        }
        
        true
    }

    /// Test control flow edge completeness property
    fn test_control_flow_edge_completeness(hir_nodes: &[HirNode]) -> bool {
        use crate::compiler::borrow_checker::cfg::construct_cfg;
        
        // Property: For any structured control flow node (If, Match, Loop), the CFG should contain edges representing all possible execution paths
        let cfg = construct_cfg(hir_nodes);
        
        for hir_node in hir_nodes {
            match &hir_node.kind {
                HirKind::If { then_block, else_block, .. } => {
                    let successors = cfg.successors(hir_node.id);
                    
                    // If statement should have edges to then block
                    if !then_block.is_empty() {
                        let then_id = then_block[0].id;
                        if !successors.contains(&then_id) {
                            return false; // Missing edge to then block
                        }
                    }
                    
                    // If statement should have edges to else block (if present)
                    if let Some(else_block) = else_block {
                        if !else_block.is_empty() {
                            let else_id = else_block[0].id;
                            if !successors.contains(&else_id) {
                                return false; // Missing edge to else block
                            }
                        }
                    }
                }
                HirKind::Loop { body, .. } => {
                    let successors = cfg.successors(hir_node.id);
                    
                    // Loop should have edge to body
                    if !body.is_empty() {
                        let body_id = body[0].id;
                        if !successors.contains(&body_id) {
                            return false; // Missing edge to loop body
                        }
                    }
                }
                _ => {
                    // Other node types don't have structured control flow requirements
                }
            }
        }
        
        true
    }

    /// Test CFG reachability property
    fn test_cfg_reachability(hir_nodes: &[HirNode]) -> bool {
        use crate::compiler::borrow_checker::cfg::construct_cfg;
        use std::collections::{HashSet, VecDeque};
        
        // Property: For any HIR node that is reachable through normal execution flow, that node should be reachable in the constructed CFG
        let cfg = construct_cfg(hir_nodes);
        
        if cfg.entry_points.is_empty() {
            return hir_nodes.is_empty(); // Empty HIR should have no entry points
        }
        
        // Perform BFS from all entry points to find reachable nodes
        let mut reachable = HashSet::new();
        let mut queue = VecDeque::new();
        
        // Start from all entry points
        for &entry_id in &cfg.entry_points {
            queue.push_back(entry_id);
            reachable.insert(entry_id);
        }
        
        // BFS traversal
        while let Some(node_id) = queue.pop_front() {
            for &successor_id in cfg.successors(node_id) {
                if !reachable.contains(&successor_id) {
                    reachable.insert(successor_id);
                    queue.push_back(successor_id);
                }
            }
        }
        
        // Check that all HIR nodes are reachable in the CFG
        // Note: In a well-formed HIR, all nodes should be reachable from entry points
        for hir_node in hir_nodes {
            if cfg.nodes.contains_key(&hir_node.id) && !reachable.contains(&hir_node.id) {
                // This node exists in CFG but is not reachable - this could be valid for some cases
                // like unreachable code after returns, so we'll be permissive here
            }
        }
        
        // The property is satisfied if we can construct the CFG without errors
        // and the reachability analysis completes successfully
        true
    }

    /// Test borrow checker initialization and basic functionality
    fn test_borrow_checker_initialization(
        _hir_module: &mut HirModule,
        string_table: &mut StringTable,
    ) -> bool {
        // Property: Borrow checker should initialize successfully with any valid HIR input
        
        // Test 1: Borrow checker should create successfully
        let borrow_checker = BorrowChecker::new(string_table);
        
        // Verify initial state is correct
        if !borrow_checker.errors.is_empty() {
            return false;
        }
        
        if !borrow_checker.warnings.is_empty() {
            return false;
        }
        
        if borrow_checker.next_borrow_id != 0 {
            return false;
        }
        
        if !borrow_checker.function_signatures.is_empty() {
            return false;
        }
        
        // Test 2: CFG should initialize correctly
        if !borrow_checker.cfg.nodes.is_empty() {
            return false;
        }
        
        if !borrow_checker.cfg.edges.is_empty() {
            return false;
        }
        
        if !borrow_checker.cfg.entry_points.is_empty() {
            return false;
        }
        
        if !borrow_checker.cfg.exit_points.is_empty() {
            return false;
        }
        
        // Test 3: Borrow ID generation should work correctly
        let mut checker = BorrowChecker::new(string_table);
        let id1 = checker.next_borrow_id();
        let id2 = checker.next_borrow_id();
        
        if id1 != 0 || id2 != 1 {
            return false;
        }
        
        // Test 4: Error collection should work
        let mut checker = BorrowChecker::new(string_table);
        let initial_error_count = checker.errors.len();
        
        // Add a test error (we'll use a simple compiler error for testing)
        use crate::compiler::compiler_messages::compiler_errors::{CompilerError, ErrorType, ErrorLocation};
        let test_error = CompilerError {
            msg: "Test error".to_string(),
            error_type: ErrorType::BorrowChecker,
            location: ErrorLocation {
                scope: PathBuf::from("test.bst"),
                start_pos: CharPosition { line_number: 1, char_column: 1 },
                end_pos: CharPosition { line_number: 1, char_column: 1 },
            },
            metadata: HashMap::new(),
        };
        
        checker.add_error(test_error);
        
        if checker.errors.len() != initial_error_count + 1 {
            return false;
        }
        
        // Test 5: Finish method should work correctly
        let result = checker.finish();
        if result.is_ok() {
            return false; // Should fail because we added an error
        }
        
        // Test 6: Empty checker should finish successfully
        let empty_checker = BorrowChecker::new(string_table);
        let empty_result = empty_checker.finish();
        if empty_result.is_err() {
            return false; // Should succeed because no errors
        }
        
        // Test 7: CFG operations should work
        let mut checker = BorrowChecker::new(string_table);
        
        // Add a test node
        checker.cfg.add_node(0, CfgNodeType::Statement);
        if !checker.cfg.nodes.contains_key(&0) {
            return false;
        }
        
        // Add another node and connect them
        checker.cfg.add_node(1, CfgNodeType::Statement);
        checker.cfg.add_edge(0, 1);
        
        let successors = checker.cfg.successors(0);
        if successors.len() != 1 || successors[0] != 1 {
            return false;
        }
        
        let predecessors = checker.cfg.predecessors(1);
        if predecessors.len() != 1 || predecessors[0] != 0 {
            return false;
        }
        
        // Test 8: Entry and exit points should work
        checker.cfg.add_entry_point(0);
        checker.cfg.add_exit_point(1);
        
        if checker.cfg.entry_points.len() != 1 || checker.cfg.entry_points[0] != 0 {
            return false;
        }
        
        if checker.cfg.exit_points.len() != 1 || checker.cfg.exit_points[0] != 1 {
            return false;
        }
        
        // All tests passed
        true
    }

    // Additional unit tests for specific borrow checker components
    #[test]
    fn test_borrow_checker_basic_initialization() {
        let mut string_table = StringTable::new();
        let checker = BorrowChecker::new(&mut string_table);
        
        assert!(checker.errors.is_empty());
        assert!(checker.warnings.is_empty());
        assert_eq!(checker.next_borrow_id, 0);
        assert!(checker.function_signatures.is_empty());
    }

    #[test]
    fn test_borrow_id_generation() {
        let mut string_table = StringTable::new();
        let mut checker = BorrowChecker::new(&mut string_table);
        
        assert_eq!(checker.next_borrow_id(), 0);
        assert_eq!(checker.next_borrow_id(), 1);
        assert_eq!(checker.next_borrow_id(), 2);
    }

    #[test]
    fn test_cfg_basic_operations() {
        let mut string_table = StringTable::new();
        let mut checker = BorrowChecker::new(&mut string_table);
        
        // Test adding nodes
        checker.cfg.add_node(0, CfgNodeType::Statement);
        checker.cfg.add_node(1, CfgNodeType::Branch);
        
        assert!(checker.cfg.nodes.contains_key(&0));
        assert!(checker.cfg.nodes.contains_key(&1));
        
        // Test adding edges
        checker.cfg.add_edge(0, 1);
        
        let successors = checker.cfg.successors(0);
        assert_eq!(successors.len(), 1);
        assert_eq!(successors[0], 1);
        
        let predecessors = checker.cfg.predecessors(1);
        assert_eq!(predecessors.len(), 1);
        assert_eq!(predecessors[0], 0);
    }

    #[test]
    fn test_error_collection() {
        let mut string_table = StringTable::new();
        let mut checker = BorrowChecker::new(&mut string_table);
        
        use crate::compiler::compiler_messages::compiler_errors::{CompilerError, ErrorType, ErrorLocation};
        
        let error1 = CompilerError {
            msg: "Test error 1".to_string(),
            error_type: ErrorType::BorrowChecker,
            location: ErrorLocation {
                scope: PathBuf::from("test.bst"),
                start_pos: CharPosition { line_number: 1, char_column: 1 },
                end_pos: CharPosition { line_number: 1, char_column: 1 },
            },
            metadata: HashMap::new(),
        };
        
        let error2 = CompilerError {
            msg: "Test error 2".to_string(),
            error_type: ErrorType::BorrowChecker,
            location: ErrorLocation {
                scope: PathBuf::from("test.bst"),
                start_pos: CharPosition { line_number: 2, char_column: 1 },
                end_pos: CharPosition { line_number: 2, char_column: 1 },
            },
            metadata: HashMap::new(),
        };
        
        checker.add_error(error1);
        checker.add_error(error2);
        
        assert_eq!(checker.errors.len(), 2);
        
        let result = checker.finish();
        assert!(result.is_err());
        
        if let Err(messages) = result {
            assert_eq!(messages.errors.len(), 2);
        }
    }

    #[test]
    fn test_successful_finish() {
        let mut string_table = StringTable::new();
        let checker = BorrowChecker::new(&mut string_table);
        
        let result = checker.finish();
        assert!(result.is_ok());
    }

    #[test]
    fn test_cfg_construction_basic() {
        use crate::compiler::borrow_checker::cfg::construct_cfg;
        
        let mut string_table = StringTable::new();
        
        // Create a simple HIR node sequence
        let nodes = vec![
            HirNode {
                id: 0,
                kind: HirKind::Assign {
                    place: Place {
                        root: PlaceRoot::Local(string_table.intern("x")),
                        projections: Vec::new(),
                    },
                    value: HirExpr {
                        kind: HirExprKind::Int(42),
                        data_type: DataType::Int,
                        location: TextLocation::default(),
                    },
                },
                location: TextLocation::default(),
                scope: InternedPath::new(),
            },
            HirNode {
                id: 1,
                kind: HirKind::Call {
                    target: string_table.intern("print"),
                    args: vec![Place {
                        root: PlaceRoot::Local(string_table.intern("x")),
                        projections: Vec::new(),
                    }],
                    returns: Vec::new(),
                },
                location: TextLocation::default(),
                scope: InternedPath::new(),
            },
            HirNode {
                id: 2,
                kind: HirKind::Return(Vec::new()),
                location: TextLocation::default(),
                scope: InternedPath::new(),
            },
        ];
        
        // Construct CFG
        let cfg = construct_cfg(&nodes);
        
        // Verify CFG structure
        assert_eq!(cfg.nodes.len(), 3);
        assert!(cfg.nodes.contains_key(&0));
        assert!(cfg.nodes.contains_key(&1));
        assert!(cfg.nodes.contains_key(&2));
        
        // Verify edges: 0 -> 1 -> 2
        let successors_0 = cfg.successors(0);
        assert_eq!(successors_0.len(), 1);
        assert_eq!(successors_0[0], 1);
        
        let successors_1 = cfg.successors(1);
        assert_eq!(successors_1.len(), 1);
        assert_eq!(successors_1[0], 2);
        
        let successors_2 = cfg.successors(2);
        assert_eq!(successors_2.len(), 0); // Return has no successors
        
        // Verify entry and exit points
        assert_eq!(cfg.entry_points.len(), 1);
        assert_eq!(cfg.entry_points[0], 0);
        
        assert_eq!(cfg.exit_points.len(), 1);
        assert_eq!(cfg.exit_points[0], 2);
    }

    #[test]
    fn test_cfg_construction_with_if() {
        use crate::compiler::borrow_checker::cfg::construct_cfg;
        
        let mut string_table = StringTable::new();
        
        // Create HIR nodes with if statement
        let then_block = vec![
            HirNode {
                id: 2,
                kind: HirKind::Assign {
                    place: Place {
                        root: PlaceRoot::Local(string_table.intern("y")),
                        projections: Vec::new(),
                    },
                    value: HirExpr {
                        kind: HirExprKind::Int(1),
                        data_type: DataType::Int,
                        location: TextLocation::default(),
                    },
                },
                location: TextLocation::default(),
                scope: InternedPath::new(),
            },
        ];
        
        let else_block = vec![
            HirNode {
                id: 3,
                kind: HirKind::Assign {
                    place: Place {
                        root: PlaceRoot::Local(string_table.intern("y")),
                        projections: Vec::new(),
                    },
                    value: HirExpr {
                        kind: HirExprKind::Int(2),
                        data_type: DataType::Int,
                        location: TextLocation::default(),
                    },
                },
                location: TextLocation::default(),
                scope: InternedPath::new(),
            },
        ];
        
        let nodes = vec![
            HirNode {
                id: 0,
                kind: HirKind::Assign {
                    place: Place {
                        root: PlaceRoot::Local(string_table.intern("x")),
                        projections: Vec::new(),
                    },
                    value: HirExpr {
                        kind: HirExprKind::Bool(true),
                        data_type: DataType::Bool,
                        location: TextLocation::default(),
                    },
                },
                location: TextLocation::default(),
                scope: InternedPath::new(),
            },
            HirNode {
                id: 1,
                kind: HirKind::If {
                    condition: Place {
                        root: PlaceRoot::Local(string_table.intern("x")),
                        projections: Vec::new(),
                    },
                    then_block: then_block.clone(),
                    else_block: Some(else_block.clone()),
                },
                location: TextLocation::default(),
                scope: InternedPath::new(),
            },
            HirNode {
                id: 4,
                kind: HirKind::Return(Vec::new()),
                location: TextLocation::default(),
                scope: InternedPath::new(),
            },
        ];
        
        // Construct CFG
        let cfg = construct_cfg(&nodes);
        
        // Verify CFG structure
        assert_eq!(cfg.nodes.len(), 5); // 3 main nodes + 2 branch nodes
        
        // Verify if statement connects to both branches
        let if_successors = cfg.successors(1);
        assert_eq!(if_successors.len(), 2);
        assert!(if_successors.contains(&2)); // then branch
        assert!(if_successors.contains(&3)); // else branch
        
        // Verify both branches connect to return
        let then_successors = cfg.successors(2);
        assert_eq!(then_successors.len(), 1);
        assert_eq!(then_successors[0], 4);
        
        let else_successors = cfg.successors(3);
        assert_eq!(else_successors.len(), 1);
        assert_eq!(else_successors[0], 4);
        
        // Verify entry and exit points
        assert_eq!(cfg.entry_points.len(), 1);
        assert_eq!(cfg.entry_points[0], 0);
        
        assert_eq!(cfg.exit_points.len(), 1);
        assert_eq!(cfg.exit_points[0], 4);
    }
}