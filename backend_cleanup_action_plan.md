# Integration Completions (Implement Missing Pieces)

### 1. Evaluate `src/compiler/mir/counter.rs`
**Current State**: Use counting utility exists but not integrated.

**Investigation needed**:
```rust
// Check if these functions are needed:
// - count_node_uses()
// - count_block_uses() 
// - count_expression_uses()
```

**Action**: Either integrate into MIR construction for optimization hints or remove entirely.

### 2. Complete `src/compiler/mir/extract.rs` Integration
**Current State**: Sophisticated borrow fact extraction exists but not fully connected.

**Missing integrations**:
- Connect `BorrowFactExtractor` to `unified_borrow_checker.rs`
- Ensure `extract_function()` is called during borrow checking
- Complete the loan generation pipeline

**Files to update**:
- `src/compiler/mir/unified_borrow_checker.rs`
- `src/compiler/mir/mir.rs` (borrow_check_pipeline)

### 3. Evaluate `src/compiler/codegen/wat_to_wasm.rs`
**Current State**: WAT text format conversion utility.

**Investigation needed**:
- Determine if WAT conversion is used for debugging
- Check if it's needed for any development tools
- If not used, remove entirely

## Simplification Actions (Remove Over-Engineering)

### 1. Simplify WASM Validation in `build_wasm.rs`
**Current Issues**: Over-engineered validation context tracking.

**Simplifications**:
- Remove `WasmValidationContext` if not actively used
- Simplify error mapping from WASM back to MIR
- Keep only essential validation for correctness

### 2. Clean Up `wasm_encoding.rs` Unused Functions
**Remove these unused functions**:
```rust
// String management
- get_string_count()
- get_allocation_stats()
- calculate_deduplication_savings()

// Local mapping  
- map_global()
- allocate_local()
- allocate_global()
- get_all_locals()
- get_all_globals()

// Statistics
- get_local_stats()
- generate_report()
- validate_size_limits()
- get_module_stats()

// Unused exports
- get_host_function_index()
- get_function_index()
- get_total_function_count()
- register_function()
- get_all_functions()
```

### 3. Simplify Place Management in `place.rs`
**Current Issues**: Complex memory layout and heap allocation tracking.

**Simplifications**:
- Remove unused heap allocation tracking
- Simplify memory layout to essential operations only
- Keep only what's needed for borrow checking and WASM lowering

## Implementation Order

### Week 1: Remove Optimization Modules
1. **Day 1-2**: Remove `arena.rs` and update all references
2. **Day 3**: Remove `cfg.rs` and update borrow checker
3. **Day 4**: Remove `dataflow.rs` and simplify borrow analysis  
4. **Day 5**: Remove `liveness.rs` and clean up MIR construction

### Week 2: Complete Integrations
1. **Day 1-2**: Evaluate and integrate or remove `counter.rs`
2. **Day 3-4**: Complete `extract.rs` integration with borrow checker
3. **Day 5**: Evaluate and handle `wat_to_wasm.rs`

### Week 3: Simplify Over-Engineering
1. **Day 1-2**: Simplify WASM validation in `build_wasm.rs`
2. **Day 3-4**: Clean up unused functions in `wasm_encoding.rs`
3. **Day 5**: Simplify place management in `place.rs`

## Testing Strategy

### After Each Removal
```bash
# Compile to check for broken references
cargo check

# Run essential tests
cargo test --lib

# Run specific test cases
cargo run -- build tests/cases/basic_features.bst
```

### Validation Criteria
- All existing test cases still compile and pass
- No functional regressions in working language features
- 80%+ reduction in compiler warnings achieved
- Compilation speed maintained or improved

## Risk Mitigation

### Backup Strategy
```bash
# Create backup branch before starting
git checkout -b backup-before-cleanup
git checkout main
```

### Incremental Approach
- Remove one module at a time
- Test after each removal
- Commit working state before next removal

### Rollback Plan
If any removal breaks functionality:
1. Identify the specific broken functionality
2. Determine if it's actually needed for core purposes
3. Either restore minimal needed parts or find alternative implementation

## Success Metrics

### Quantitative Goals
- **Compiler warnings**: Reduce from 157 to <30 (80%+ reduction)
- **Module count**: Reduce MIR modules from 13 to 9 (31% reduction)
- **Function count**: Remove ~100 unused functions across modules
- **Compilation time**: Maintain or improve current speeds

### Qualitative Goals
- **Clearer architecture**: MIR focused only on WASM lowering and borrow checking
- **Easier maintenance**: Reduced cognitive load for developers
- **Better alignment**: Code matches stated design philosophy
- **Improved onboarding**: Less complex, unused code to understand

## Post-Cleanup Actions

### Documentation Updates
1. Update architecture documentation to reflect simplified MIR
2. Document which features are implemented vs. planned
3. Update contributor guidelines with new module structure

### Future Prevention
1. Add linting rules to prevent optimization code in MIR
2. Establish clear guidelines for MIR vs. external optimization
3. Regular code usage analysis to prevent accumulation of dead code

## Conclusion

This action plan provides a systematic approach to cleaning up the backend while preserving all functional capabilities. The phased approach minimizes risk while achieving the goal of dramatically reducing compiler warnings and improving code clarity.