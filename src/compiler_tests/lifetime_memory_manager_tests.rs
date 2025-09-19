use crate::compiler::codegen::lifetime_memory_manager::*;
use crate::compiler::mir::extract::BorrowFactExtractor;
use crate::compiler::mir::mir_nodes::*;
use crate::compiler::mir::place::*;
use crate::compiler::mir::unified_borrow_checker::UnifiedBorrowCheckResults;

#[test]
fn test_lifetime_memory_manager_creation() {
    let manager = LifetimeMemoryManager::new();
    let stats = manager.get_statistics();
    
    assert_eq!(stats.places_analyzed, 0);
    assert_eq!(stats.single_ownership_optimizations, 0);
    assert_eq!(stats.arc_operations_eliminated, 0);
    assert_eq!(stats.move_optimizations_applied, 0);
    assert_eq!(stats.drop_operations_optimized, 0);
}

#[test]
fn test_single_ownership_detection() {
    let mut manager = LifetimeMemoryManager::new();
    let place = Place::Local {
        index: 0,
        wasm_type: WasmType::I32,
    };

    // Create empty extractor (no loans = single ownership)
    let extractor = BorrowFactExtractor::new();
    
    assert!(!manager.place_has_shared_ownership(&place, &extractor));
}

#[test]
fn test_wasm_value_type_optimization() {
    let manager = LifetimeMemoryManager::new();
    
    let i32_place = Place::Local {
        index: 0,
        wasm_type: WasmType::I32,
    };
    
    let memory_place = Place::Memory {
        base: MemoryBase::LinearMemory,
        offset: ByteOffset(0),
        size: TypeSize::Word,
    };

    assert!(manager.can_optimize_as_wasm_value_type(&i32_place));
    assert!(!manager.can_optimize_as_wasm_value_type(&memory_place));
}

#[test]
fn test_place_size_calculation() {
    let manager = LifetimeMemoryManager::new();
    
    let i32_place = Place::Local {
        index: 0,
        wasm_type: WasmType::I32,
    };
    
    let i64_place = Place::Local {
        index: 1,
        wasm_type: WasmType::I64,
    };

    assert_eq!(manager.calculate_place_size(&i32_place), 4);
    assert_eq!(manager.calculate_place_size(&i64_place), 8);
}

#[test]
fn test_arc_info_creation() {
    let manager = LifetimeMemoryManager::new();
    let place = Place::Local {
        index: 0,
        wasm_type: WasmType::I32,
    };

    let arc_info = manager.create_arc_info_for_place(&place).unwrap();
    assert_eq!(arc_info.data_size, 4); // i32 size
    assert_eq!(arc_info.data_type, WasmType::I32);
    assert!(!arc_info.is_optimized_away);
}

#[test]
fn test_value_type_optimization_creation() {
    let manager = LifetimeMemoryManager::new();
    let place = Place::Local {
        index: 0,
        wasm_type: WasmType::I32,
    };

    let optimization = manager.create_value_type_optimization(&place).unwrap();
    assert_eq!(optimization.performance_benefit.instruction_reduction, 2);
    assert_eq!(optimization.performance_benefit.memory_reduction, 4);
    assert_eq!(optimization.performance_benefit.arc_elimination_count, 1);
}

#[test]
fn test_memory_management_integration() {
    let mut manager = LifetimeMemoryManager::new();
    
    // Create a simple test function
    let mut function = MirFunction::new(0, "test".to_string(), vec![], vec![]);
    
    // Add a local place
    let place = Place::Local {
        index: 0,
        wasm_type: WasmType::I32,
    };
    function.add_local("test_var".to_string(), place.clone());
    
    // Create empty borrow checking results
    let borrow_results = UnifiedBorrowCheckResults {
        errors: vec![],
        warnings: vec![],
        statistics: crate::compiler::mir::unified_borrow_checker::UnifiedStatistics::default(),
    };
    
    // Create empty extractor
    let extractor = BorrowFactExtractor::new();
    
    // Test the analysis
    let result = manager.analyze_function(&function, &borrow_results, &extractor);
    assert!(result.is_ok());
    
    // Check that the place was analyzed
    let stats = manager.get_statistics();
    assert_eq!(stats.places_analyzed, 1);
    
    // Check that single ownership was detected
    assert!(manager.uses_single_ownership(&place));
    assert!(!manager.requires_arc(&place));
}

#[test]
fn test_optimization_queries() {
    let mut manager = LifetimeMemoryManager::new();
    let place = Place::Local {
        index: 0,
        wasm_type: WasmType::I32,
    };
    
    // Initially no optimizations
    assert!(!manager.uses_single_ownership(&place));
    assert!(!manager.requires_arc(&place));
    assert!(manager.get_value_type_optimization(&place).is_none());
    
    // Add to single ownership
    manager.single_ownership_places.insert(place.clone());
    
    // Now should be detected
    assert!(manager.uses_single_ownership(&place));
    assert!(!manager.requires_arc(&place));
}