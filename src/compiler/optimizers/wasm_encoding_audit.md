# WASM Encoding Audit Results

## Current State Analysis

The `src/compiler/codegen/wasm_encoding.rs` file (5833 lines) already leverages wasm_encoder effectively:

### ✅ Already Using wasm_encoder Correctly
- **Section Builders**: Uses `TypeSection`, `FunctionSection`, `MemorySection`, etc.
- **Instruction Encoding**: Uses `wasm_encoder::Instruction` enum throughout
- **Module Structure**: Uses `wasm_encoder` types for all WASM constructs

### ✅ Custom Code That Should Remain
- **MIR-to-WASM Compatibility Validation**: High-level validation of MIR types against WASM constraints
- **Application Metadata**: Custom byte layouts for Beanstalk-specific metadata (struct layouts, vtables)
- **Memory Management Integration**: Custom heap management that integrates with WASM linear memory

### 🔄 Areas for Simplification (Not wasm_encoder Issues)
- **Complex Memory Layout Management**: ~1500 lines of memory layout code that can be simplified
- **Interface Dispatch System**: Already moved to optimizers (saved ~400 lines)
- **Complex Control Flow Handling**: Over-engineered structured control flow (can be simplified)
- **Optimization-Specific Code**: Premature optimizations that add complexity

## Recommendations

1. **Keep Current wasm_encoder Usage**: The codebase already uses wasm_encoder effectively
2. **Focus on Simplification**: Reduce complexity by removing over-engineered features
3. **Target Basic Functionality**: Focus on simple MIR statement lowering to WASM instructions

## Line Count Reduction Strategy

- Current: 5833 lines
- Target: ~2000 lines  
- Reduction needed: ~3800 lines

**Primary reduction opportunities:**
- Complex memory layout management: ~1500 lines
- Over-engineered control flow: ~800 lines
- Optimization-specific code: ~1000 lines
- Complex validation and metadata: ~500 lines

The issue is not wasm_encoder duplication but rather over-engineering of features that should be simplified or moved to optimizers.