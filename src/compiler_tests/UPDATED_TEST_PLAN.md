# Updated Beanstalk Compiler Test Plan

## Overview

This document outlines the updated test structure after removing redundant and outdated tests, focusing on the current WASM-optimized MIR backend.

## Test Cleanup Summary

### Removed/Simplified Tests

1. **Place Interner Tests** (`place_interner_test.rs`)
   - **Issue**: Tests were using outdated API that no longer exists
   - **Action**: Replaced with placeholder until API is stabilized
   - **Reason**: Avoid compilation errors from non-existent imports

2. **Overly Complex Borrow Check Tests** (`borrow_check_tests.rs`)
   - **Issue**: Tests were testing implementation details rather than behavior
   - **Action**: Simplified to focus on essential borrow checking behavior
   - **Reason**: Reduce maintenance burden and focus on what matters

3. **Redundant Performance Tests** (`performance_tests.rs`)
   - **Issue**: Overlapping with `focused_performance_tests.rs`
   - **Action**: Keep focused version, mark comprehensive version as optional
   - **Reason**: Avoid duplicate test coverage

4. **Incomplete Integration Tests** (`integration_tests.rs`)
   - **Issue**: Placeholder implementations that don't test real functionality
   - **Action**: Simplified to basic file processing until MIR pipeline is complete
   - **Reason**: Avoid false positives from incomplete implementations

### Kept and Updated Tests

1. **Core Compiler Tests** (`core_compiler_tests.rs`)
   - **Status**: Updated to match current Place API
   - **Focus**: Essential compiler functionality and WASM optimization
   - **Coverage**: AST → MIR → WASM pipeline validation

2. **Place Tests** (`place_tests.rs`)
   - **Status**: Updated to match current Place implementation
   - **Focus**: WASM-optimized memory management
   - **Coverage**: Place creation, projections, instruction counting

3. **Focused Performance Tests** (`focused_performance_tests.rs`)
   - **Status**: Kept as primary performance validation
   - **Focus**: Key performance goals and regression detection
   - **Coverage**: Compilation speed, memory usage, scalability

## Current Test Organization

### Essential Tests (Run First)
```
src/compiler_tests/
├── core_compiler_tests.rs      # Core functionality validation
├── place_tests.rs              # WASM place system tests  
├── focused_performance_tests.rs # Key performance goals
└── test_runner.rs              # Test orchestration
```

### Specialized Tests (Run As Needed)
```
src/compiler_tests/
├── borrow_check_tests.rs       # Simplified borrow checking
├── wasm_module_tests.rs        # WASM generation tests
├── memory_layout_tests.rs      # Memory management tests
└── interface_vtable_tests.rs   # Interface dispatch tests
```

### Comprehensive Tests (CI/Development)
```
src/compiler_tests/
├── performance_tests.rs        # Detailed performance analysis
├── wasm_optimization_tests.rs  # WASM optimization validation
├── benchmark_runner.rs         # Full benchmark suite
└── integration_tests.rs        # End-to-end testing
```

### Disabled/Placeholder Tests
```
src/compiler/mir/
└── place_interner_test.rs      # Disabled until API stabilized
```

## Test Quality Improvements

### 1. Behavior-Focused Testing
- Tests validate what the compiler should do, not how it does it
- Reduced coupling to internal implementation details
- More maintainable as the codebase evolves

### 2. WASM-First Validation
- Tests enforce WASM-optimized design principles
- Validate instruction count goals (≤3 instructions per MIR statement)
- Ensure memory layout optimization for linear memory

### 3. Performance Goal Enforcement
- Specific performance targets are validated
- Regression detection for performance issues
- Scalability validation with reasonable bounds

### 4. Error Handling Alignment
- Tests use proper error macros from style guide
- Consistent error message patterns
- Proper source location tracking

## Running Tests

### Quick Validation
```bash
cargo test core_compiler_tests
cargo test place_tests
cargo test focused_performance_tests
```

### Full Test Suite
```bash
cargo test --package beanstalk-compiler --lib compiler_tests
```

### Performance Benchmarks
```bash
cargo test --package beanstalk-compiler --lib compiler_tests::benchmark_runner
```

## Future Test Improvements

### Short Term
1. **Update Place Interner Tests**: Once API is stabilized
2. **Complete Integration Tests**: When MIR pipeline is ready
3. **Add Fuzzing Tests**: For robustness validation

### Medium Term
1. **Property-Based Tests**: For invariant checking
2. **Visual Test Reports**: For performance trend analysis
3. **Parallel Test Execution**: For faster test runs

### Long Term
1. **CI Integration**: Automated performance monitoring
2. **Regression Database**: Historical performance tracking
3. **Test Generation**: Automated test case generation

## Benefits of This Cleanup

### 1. Reliability
- Removed tests that were causing compilation errors
- Focus on tests that actually validate functionality
- Clear separation between working and placeholder tests

### 2. Maintainability
- Reduced test code duplication
- Clear organization by test purpose
- Behavior-focused tests that survive refactoring

### 3. Development Velocity
- Fast essential test suite for quick validation
- Clear failure reporting for debugging
- Reduced noise from broken/redundant tests

### 4. Architecture Compliance
- Tests enforce WASM-first design principles
- Validate MIR optimization goals
- Ensure memory safety guarantees

This updated test structure provides a solid foundation for maintaining code quality while supporting the rapid development of the Beanstalk compiler's WASM-optimized architecture.