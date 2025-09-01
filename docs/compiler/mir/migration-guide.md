# Migration Guide: Simplified MIR Borrow Checking

This guide helps developers migrate from the old Polonius-style MIR borrow checking system to the new simplified dataflow-based system. The migration is designed to be minimally disruptive while providing significant performance improvements.

## Table of Contents

1. [Migration Overview](#migration-overview)
2. [Breaking Changes](#breaking-changes)
3. [API Changes](#api-changes)
4. [Code Migration Steps](#code-migration-steps)
5. [Testing Migration](#testing-migration)
6. [Performance Validation](#performance-validation)
7. [Troubleshooting](#troubleshooting)
8. [Rollback Strategy](#rollback-strategy)

## Migration Overview

### What Changed

The MIR borrow checking system has been completely rewritten to use simple dataflow analysis instead of complex constraint solving:

**Old System**:
- Polonius-style fact generation
- Complex region and lifetime constraints
- Quadratic/cubic performance characteristics
- Heavy memory usage

**New System**:
- Simple event extraction
- Standard dataflow algorithms
- Linear performance characteristics  
- Efficient bitset operations

### What Stayed the Same

- **Beanstalk Language**: No changes to user-facing language syntax or semantics
- **MIR Place System**: The excellent WASM-optimized Place abstraction is unchanged
- **Error Quality**: Error messages are improved, not degraded
- **Correctness**: All borrow checking rules are preserved

### Migration Timeline

The migration can be done incrementally:

1. **Phase 1**: Update MIR construction to generate events
2. **Phase 2**: Replace constraint solving with dataflow analysis
3. **Phase 3**: Update error reporting and diagnostics
4. **Phase 4**: Remove old Polonius infrastructure
5. **Phase 5**: Testing and validation

## Breaking Changes

### Minimal Breaking Changes

The migration was designed to minimize breaking changes:

✅ **No Changes Required**:
- Beanstalk source code
- MIR statement structure (simplified but compatible)
- WASM codegen integration
- Place abstraction and aliasing rules
- Test case expectations (same borrow checking rules)

⚠️ **Minor Changes Required**:
- MIR construction code (event generation instead of fact generation)
- Borrow checker integration points
- Some internal APIs for accessing analysis results

❌ **Removed Features**:
- Complex Polonius fact types
- Region and lifetime constraint infrastructure
- Over-engineered memory safety tracking

### Compatibility Guarantees

1. **Backward Compatibility**: Existing Beanstalk programs compile identically
2. **Error Compatibility**: Same borrow checking errors are detected
3. **Improved Compilation**: Faster compilation with reduced memory usage
4. **Enhanced Maintainability**: Simpler, more understandable codebase

## API Changes

### MIR Construction Changes

**Old Event Generation**:
```rust
// Old: Complex Polonius fact generation
fn generate_borrow_facts(statement: &Statement, facts: &mut AllFacts) {
    match statement {
        Statement::Assign { place, rvalue } => {
            match rvalue {
                Rvalue::Ref { place: borrowed_place, kind } => {
                    let loan = facts.allocate_loan();
                    let region = facts.allocate_region();
                    
                    facts.loan_issued_at.push((point, loan, region));
                    facts.region_live_at.push((region, point));
                    facts.outlives.push((region, region_parent, point));
                    // ... dozens more fact types
                }
            }
        }
    }
}
```

**New Event Generation**:
```rust
// New: Simple event extraction
fn generate_statement_events(statement: &Statement, point: ProgramPoint, events: &mut Events) {
    match statement {
        Statement::Assign { place, rvalue } => {
            match rvalue {
                Rvalue::Ref { place: borrowed_place, kind } => {
                    let loan_id = allocate_loan_id();
                    events.start_loans.push(loan_id);
                    events.uses.push(borrowed_place.clone());
                    events.reassigns.push(place.clone());
                }
            }
        }
    }
}
```

### Borrow Checker Integration Changes

**Old Integration**:
```rust
// Old: Constraint solving integration
pub fn run_borrow_checker(mir: &MirFunction) -> Result<(), Vec<BorrowError>> {
    let facts = extract_polonius_facts(mir)?;
    let output = solve_constraints(facts)?;
    let errors = check_constraint_violations(output)?;
    
    if errors.is_empty() { Ok(()) } else { Err(errors) }
}
```

**New Integration**:
```rust
// New: Dataflow analysis integration
pub fn run_borrow_checker(mir: &MirFunction) -> Result<(), Vec<BorrowError>> {
    let extractor = extract_gen_kill_sets(mir)?;
    let dataflow = run_loan_liveness_dataflow(mir, &extractor)?;
    let conflicts = run_conflict_detection(mir, dataflow, extractor)?;
    
    if conflicts.errors.is_empty() { Ok(()) } else { Err(conflicts.errors) }
}
```

### Analysis Result Access Changes

**Old Result Access**:
```rust
// Old: Complex constraint output queries
fn is_loan_live_at_point(output: &PoloniusOutput, loan: Loan, point: Point) -> bool {
    output.loan_live_at.contains(&(loan, point))
}

fn get_region_outlives(output: &PoloniusOutput, region: Region) -> Vec<Region> {
    output.outlives.iter()
        .filter(|(r1, r2, _)| r1 == &region)
        .map(|(_, r2, _)| *r2)
        .collect()
}
```

**New Result Access**:
```rust
// New: Simple dataflow result queries
fn is_loan_live_at_point(dataflow: &LoanLivenessDataflow, loan_idx: usize, point: &ProgramPoint) -> bool {
    dataflow.is_loan_live_at(loan_idx, point)
}

fn get_live_loans_at_point(dataflow: &LoanLivenessDataflow, point: &ProgramPoint) -> Vec<usize> {
    dataflow.get_live_loan_indices_at(point)
}
```

## Code Migration Steps

### Step 1: Update MIR Construction

Replace fact generation with event generation:

```rust
// 1. Replace AllFacts with Events in MirTransformContext
pub struct MirTransformContext {
    // Remove:
    // facts: AllFacts,
    
    // Add:
    events_map: HashMap<ProgramPoint, Events>,
    loans: Vec<Loan>,
    next_loan_id: u32,
}

// 2. Update statement transformation
impl MirTransformContext {
    fn transform_statement(&mut self, stmt: &Statement) -> Result<Vec<MirStatement>, CompileError> {
        let program_point = self.allocate_program_point();
        let mir_stmt = self.lower_statement_to_mir(stmt)?;
        
        // Generate events instead of facts
        let events = self.generate_statement_events(&mir_stmt, program_point);
        self.store_events(program_point, events);
        
        Ok(vec![mir_stmt])
    }
}
```

### Step 2: Replace Constraint Solving with Dataflow

Update the borrow checking pipeline:

```rust
// 1. Remove Polonius integration
// Remove: use polonius_engine::{Algorithm, Output};

// Add dataflow modules
use crate::compiler::mir::liveness::run_liveness_analysis;
use crate::compiler::mir::dataflow::run_loan_liveness_dataflow;
use crate::compiler::mir::check::run_conflict_detection;

// 2. Update borrow checking function
pub fn borrow_check_pipeline(ast: AstBlock) -> Result<MIR, Vec<CompileError>> {
    // Step 1: Lower AST to MIR with event generation
    let mir = ast_to_mir_with_events(ast)?;
    
    // Step 2: Run borrow checking on each function
    let mut all_errors = Vec::new();
    
    for function in &mir.functions {
        match run_borrow_checking_on_function(function) {
            Ok(_) => {}, // Success
            Err(errors) => all_errors.extend(errors),
        }
    }
    
    if !all_errors.is_empty() {
        return Err(all_errors);
    }
    
    Ok(mir)
}

fn run_borrow_checking_on_function(function: &MirFunction) -> Result<(), Vec<CompileError>> {
    // Extract gen/kill sets from events
    let extractor = extract_gen_kill_sets(function)?;
    
    // Run forward loan-liveness dataflow
    let dataflow = run_loan_liveness_dataflow(function, &extractor)?;
    
    // Detect conflicts using live loan sets
    let conflicts = run_conflict_detection(function, dataflow, extractor)?;
    
    // Convert conflicts to compile errors
    if !conflicts.errors.is_empty() {
        let diagnostics = diagnose_borrow_errors(function, &conflicts.errors, function.get_loans())?;
        return Err(diagnostics_to_compile_errors(&diagnostics));
    }
    
    Ok(())
}
```

### Step 3: Update Error Reporting

Migrate error reporting to use new diagnostic system:

```rust
// 1. Update error types (minimal changes needed)
// BorrowError structure remains largely the same

// 2. Update error generation
fn create_borrow_error(
    point: ProgramPoint,
    error_type: BorrowErrorType,
    message: String,
) -> BorrowError {
    BorrowError {
        point,
        error_type,
        message,
        location: get_source_location_for_program_point(point),
    }
}

// 3. Update diagnostic formatting
impl BorrowDiagnostics {
    pub fn format_error(&self, error: &BorrowError) -> String {
        match &error.error_type {
            BorrowErrorType::ConflictingBorrows { existing_borrow, new_borrow, place } => {
                format!(
                    "cannot borrow `{}` as {} because it is already borrowed as {}",
                    place_name(place),
                    borrow_kind_name(new_borrow),
                    borrow_kind_name(existing_borrow)
                )
            }
            // ... other error types
        }
    }
}
```

### Step 4: Remove Old Infrastructure

Clean up Polonius-related code:

```rust
// 1. Remove Polonius dependencies from Cargo.toml
// [dependencies]
// polonius-engine = "0.13.0"  // Remove this

// 2. Remove fact generation code
// Delete or comment out:
// - AllFacts struct
// - Complex fact generation functions
// - Region and lifetime constraint code
// - Polonius integration functions

// 3. Update imports
// Remove:
// use polonius_engine::*;

// Add:
use crate::compiler::mir::{
    liveness::LivenessAnalysis,
    dataflow::LoanLivenessDataflow,
    check::BorrowConflictChecker,
};
```

### Step 5: Update Tests

Migrate test infrastructure:

```rust
// 1. Update test helper functions
fn create_test_mir_function() -> MirFunction {
    let mut function = MirFunction::new(0, "test".to_string(), vec![], vec![]);
    
    // Add program points and events instead of facts
    let pp1 = ProgramPoint::new(0);
    let events = Events {
        start_loans: vec![LoanId::new(0)],
        uses: vec![Place::Local { index: 0, wasm_type: WasmType::I32 }],
        reassigns: vec![Place::Local { index: 1, wasm_type: WasmType::Ptr }],
        candidate_last_uses: vec![],
    };
    
    function.add_program_point(pp1, 0, 0);
    function.store_events(pp1, events);
    
    function
}

// 2. Update test assertions
#[test]
fn test_borrow_checking() {
    let function = create_test_mir_function();
    
    // Old assertion:
    // let facts = extract_facts(&function);
    // assert!(facts.loan_issued_at.len() > 0);
    
    // New assertion:
    let result = run_borrow_checking_on_function(&function);
    assert!(result.is_ok());
}
```

## Testing Migration

### Test Strategy

1. **Regression Tests**: Ensure all existing tests pass
2. **Performance Tests**: Validate 2-3x speedup
3. **Memory Tests**: Confirm memory usage reduction
4. **Error Tests**: Verify error message quality

### Test Migration Checklist

```rust
// 1. Update unit tests
□ Update MIR construction tests
□ Update borrow checking tests  
□ Update error reporting tests
□ Update place analysis tests

// 2. Update integration tests
□ Migrate .bst test files (no changes needed)
□ Update test harness for new API
□ Validate error message formats
□ Check performance benchmarks

// 3. Add new tests
□ Dataflow analysis tests
□ Bitset operation tests
□ Program point generation tests
□ Event extraction tests
```

### Test Execution

```bash
# Run full test suite
cargo test --features "verbose_ast_logging,verbose_eval_logging,verbose_codegen_logging"

# Run performance benchmarks
cargo bench --features "detailed_timers"

# Run specific borrow checking tests
cargo test borrow_check --features "detailed_timers"

# Validate against test cases
cargo run --features "detailed_timers" -- build tests/cases/
```

## Validation

### Testing Validation

Ensure all tests pass with the new system:

```rust
// 1. Update test functions
#[test]
fn test_borrow_checking_new() {
    let test_functions = generate_test_functions(100, 0.3); // 100 stmts, 30% borrows
    
    for function in &test_functions {
        let result = run_borrow_checking_on_function(function);
        assert!(result.is_ok());
    }
}

// 2. Compare with baseline behavior
fn validate_correctness() {
    let test_cases = load_test_cases();
    
    for test_case in test_cases {
        let old_result = run_old_borrow_checker(&test_case);
        let new_result = run_new_borrow_checker(&test_case);
        
        // Results should be equivalent
        assert_eq!(old_result.is_ok(), new_result.is_ok());
    }
}
```

## Troubleshooting

### Common Migration Issues

**Issue 1: Missing Events**
```rust
// Problem: Events not generated for all statements
// Solution: Ensure all statement types generate appropriate events

fn generate_statement_events(statement: &Statement) -> Events {
    let mut events = Events::default();
    
    match statement {
        Statement::Assign { place, rvalue } => {
            events.reassigns.push(place.clone());
            generate_rvalue_events(rvalue, &mut events);
        }
        Statement::Call { args, destination, .. } => {
            for arg in args {
                generate_operand_events(arg, &mut events);
            }
            if let Some(dest) = destination {
                events.reassigns.push(dest.clone());
            }
        }
        // Ensure ALL statement types are handled
        _ => {} // Add missing cases here
    }
    
    events
}
```

**Issue 2: Incorrect Aliasing Analysis**
```rust
// Problem: may_alias function not handling all cases
// Solution: Ensure comprehensive aliasing rules

pub fn may_alias(a: &Place, b: &Place) -> bool {
    match (a, b) {
        // Add missing aliasing cases
        (Place::Projection { base: b1, elem: e1 }, 
         Place::Projection { base: b2, elem: e2 }) => {
            // Handle all projection combinations
            match (e1, e2) {
                (ProjectionElem::Field { index: i1, .. },
                 ProjectionElem::Field { index: i2, .. }) => {
                    b1 == b2 && i1 == i2
                }
                // Add other projection element combinations
                _ => conservative_alias_analysis(a, b)
            }
        }
        _ => false
    }
}
```

**Issue 3: Performance Regression**
```rust
// Problem: Performance not meeting expectations
// Solution: Profile and optimize bottlenecks

fn profile_borrow_checking() {
    let profiler = DataflowProfiler::new();
    
    let result = profiler.profile(|| {
        run_borrow_checking_pipeline(test_mir)
    });
    
    println!("Performance breakdown:");
    println!("  Event extraction: {}ms", result.event_extraction_time);
    println!("  Dataflow analysis: {}ms", result.dataflow_time);
    println!("  Conflict detection: {}ms", result.conflict_detection_time);
    
    // Identify and optimize bottlenecks
}
```

### Debug Tools

```rust
// 1. Event debugging
fn debug_events(function: &MirFunction) {
    for &pp in function.get_program_points_in_order() {
        if let Some(events) = function.get_events(&pp) {
            println!("PP{}: {:?}", pp.id(), events);
        }
    }
}

// 2. Dataflow debugging  
fn debug_dataflow(dataflow: &LoanLivenessDataflow, function: &MirFunction) {
    for &pp in function.get_program_points_in_order() {
        let live_in = dataflow.get_live_in_loans(&pp);
        let live_out = dataflow.get_live_out_loans(&pp);
        println!("PP{}: LiveIn={:?}, LiveOut={:?}", pp.id(), live_in, live_out);
    }
}

// 3. Performance debugging
fn debug_performance() {
    let stats = DataflowStatistics::collect();
    println!("Analysis statistics: {:?}", stats);
}
```

## Rollback Strategy

### Rollback Plan

If issues arise during migration, a rollback strategy is available:

**Phase 1: Feature Flag Rollback**
```rust
// Add feature flag for old system
#[cfg(feature = "old_borrow_checker")]
pub fn run_borrow_checker_old(mir: &MirFunction) -> Result<(), Vec<BorrowError>> {
    // Keep old Polonius-based implementation
}

#[cfg(not(feature = "old_borrow_checker"))]
pub fn run_borrow_checker(mir: &MirFunction) -> Result<(), Vec<BorrowError>> {
    // New dataflow-based implementation
}
```

**Phase 2: Gradual Rollback**
```rust
// Rollback specific components while keeping others
pub fn hybrid_borrow_checker(mir: &MirFunction) -> Result<(), Vec<BorrowError>> {
    // Use new event extraction but old constraint solving
    let events = extract_events(mir)?; // New
    let facts = convert_events_to_facts(events)?; // Compatibility layer
    let output = solve_constraints_old(facts)?; // Old
    check_violations(output) // Old
}
```

**Phase 3: Complete Rollback**
```bash
# Revert to previous commit
git revert <migration_commit_hash>

# Or restore from backup
git checkout backup_branch
git merge --strategy=ours main
```

### Rollback Triggers

Rollback should be considered if:

1. **Correctness Issues**: Incorrect borrow checking results
2. **Stability Issues**: Crashes or infinite loops
3. **Test Failures**: >5% of existing tests fail
4. **Compilation Regression**: Significantly slower compilation
5. **Memory Issues**: Excessive memory usage

### Rollback Testing

```rust
// Validate rollback functionality
#[test]
fn test_rollback_compatibility() {
    let test_cases = load_comprehensive_test_suite();
    
    for test_case in test_cases {
        let old_result = run_borrow_checker_old(&test_case.mir);
        let new_result = run_borrow_checker_new(&test_case.mir);
        
        // Results should be equivalent
        assert_eq!(old_result.is_ok(), new_result.is_ok());
        
        if let (Err(old_errors), Err(new_errors)) = (old_result, new_result) {
            assert_equivalent_errors(&old_errors, &new_errors);
        }
    }
}
```

The migration to the simplified MIR borrow checking system provides significant performance benefits while maintaining correctness and minimizing disruption to existing code.