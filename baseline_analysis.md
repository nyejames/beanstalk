# Backend Compilation Fixes - Baseline Analysis

**Date:** 2024-11-15
**Task:** Establish baseline and analyze errors

## Summary Statistics

- **Total Compilation Errors:** 71
- **Total Warnings:** 95
- **Total Line Count (WIR + Codegen):** 12,189 lines

### Line Count Breakdown
```
WIR Module (src/compiler/wir/):
  - wir.rs: 99 lines
  - expressions.rs: 490 lines
  - wir_nodes.rs: 1,291 lines
  - place.rs: 840 lines
  - mod.rs: 154 lines
  - statements.rs: 627 lines
  - utilities.rs: 9 lines
  - build_wir.rs: 744 lines
  - templates.rs: 333 lines
  - context.rs: 621 lines
  Subtotal: 5,208 lines

Codegen Module (src/compiler/codegen/):
  - wasm_encoding.rs: 6,684 lines
  - wat_to_wasm.rs: 35 lines
  - mod.rs: 90 lines
  - build_wasm.rs: 172 lines
  Subtotal: 6,981 lines

TOTAL: 12,189 lines
```

## Error Categorization

### Category 1: String Interning / StringId Type Mismatches (28 errors)
**Description:** Code is passing `StringId` (interned string reference) where `&str` (actual string) is expected, or vice versa. This is because host functions and some codegen functions were updated to use `StringId` but the call sites weren't updated.

**Affected Files:**
- `src/compiler/codegen/wasm_encoding.rs` (majority of errors)

**Example Errors:**
- Line 2792: `expected &str, found &StringId` in `lower_wasix_print_constant`
- Line 3097: `expected &str, found &StringId` in `add_string_slice_constant`
- Line 3110: `no method named len found for reference &StringId`
- Line 4293: `expected (String, String), found (StringId, StringId)`
- Line 4298: Missing `string_table` parameter in `params_to_signature()`
- Line 4313, 4317: `expected String, found StringId` in HashMap insert
- Line 5730: `expected &str, found &StringId` in `has_function`
- Line 5747: Trait bound issue with `Borrow<StringId>`
- Line 5783: `expected StringId, found &str` in comparison
- Line 5869: `expected &str, found &StringId` in `add_string_slice_constant`

**Root Cause:** Host function definitions were updated to use `StringId` for efficiency, but the codegen that calls these functions still expects/provides `&str`.

### Category 2: Missing Fields in Struct Initialization (5 errors)
**Description:** The `Project` struct was updated to include a `warnings` field, but several initialization sites don't include it.

**Affected Files:**
- `src/build_system/embedded_project.rs` (line 171)
- `src/build_system/html_project.rs` (line 112)
- `src/build_system/jit.rs` (line 47)
- `src/build_system/native_project.rs` (line 47)

**Example Error:**
```
error[E0063]: missing field `warnings` in initializer of `build::Project`
```

**Root Cause:** Project struct was extended with warnings field but initialization sites weren't updated.

### Category 3: Missing Method / Removed API (4 errors)
**Description:** Code is calling `CompileError::new_config_error()` which no longer exists. Should use `return_config_error!` macro instead.

**Affected Files:**
- `src/build_system/embedded_project.rs` (line 203)
- `src/build_system/html_project.rs` (lines 125, 139)

**Example Error:**
```
error[E0599]: no function or associated item named `new_config_error` found for struct `compiler_errors::CompileError`
```

**Root Cause:** Error system was unified to use macros, old methods were removed.

### Category 4: Missing Function Parameters (6 errors)
**Description:** Functions were updated to require additional parameters (typically `&mut StringTable`) but call sites weren't updated.

**Affected Files:**
- `src/build_system/embedded_project.rs` (line 141)
- `src/build_system/jit.rs` (line 40)
- `src/build_system/native_project.rs` (line 40)
- `src/build_system/repl.rs` (lines 102, 112)

**Example Errors:**
- `compile_modules` needs `&mut StringTable` parameter
- `Template::new` needs `&mut StringTable` parameter
- `template.fold` needs `&StringTable` parameter

**Root Cause:** String interning system requires StringTable to be passed through call chains.

### Category 5: Wrong Field Names / Field Access (3 errors)
**Description:** Code is accessing fields that don't exist on the error struct (e.g., `.message` instead of `.msg`, `.primary_location` instead of `.location`).

**Affected Files:**
- `src/compiler/wir/build_wir.rs` (lines 664, 667, 668)

**Example Errors:**
```
error[E0609]: no field `message` on type `&compiler_errors::CompileError`
error[E0609]: no field `primary_location` on type `&compiler_errors::CompileError`
```

**Root Cause:** Error struct fields were renamed during unification.

### Category 6: TextLocation vs ErrorLocation Type Mismatches (3 errors)
**Description:** Code is using `TextLocation` where `ErrorLocation` is expected, or vice versa.

**Affected Files:**
- `src/build_system/native_project.rs` (line 68)
- `src/compiler/wir/statements.rs` (line 111)

**Example Error:**
```
error[E0308]: mismatched types - expected `ErrorLocation`, found `TextLocation`
```

**Root Cause:** Error system now uses ErrorLocation (owned data) instead of TextLocation (interned strings).

### Category 7: Type Mismatches in HashMap Operations (4 errors)
**Description:** HashMap operations have type mismatches, particularly with metadata insertion where `&'static str` is expected but `&mut str` is provided.

**Affected Files:**
- `src/compiler/wir/context.rs` (line 580)
- `src/compiler/wir/statements.rs` (lines 183, 184, 185)

**Example Errors:**
- Line 580: `expected String, found StringId` in HashMap insert
- Lines 183-185: Metadata HashMap expects `&'static str` but gets `&mut str` from `Box::leak`

**Root Cause:** Inconsistent use of string types and lifetime issues with metadata.

### Category 8: Path Type Mismatches (2 errors)
**Description:** Code is passing `&Path` or `PathBuf` where `&InternedPath` or `InternedPath` is expected.

**Affected Files:**
- `src/build_system/repl.rs` (lines 92, 95)

**Example Error:**
```
error[E0308]: expected `&InternedPath`, found `&Path`
```

**Root Cause:** Path handling was updated to use interned paths.

### Category 9: Trait Bound Issues (2 errors)
**Description:** HashMap lookup fails because `String` doesn't implement `Borrow<StringId>`.

**Affected Files:**
- `src/compiler/codegen/wasm_encoding.rs` (lines 5747, 5819)

**Example Error:**
```
error[E0277]: the trait bound `std::string::String: std::borrow::Borrow<StringId>` is not satisfied
```

**Root Cause:** HashMap keys are `String` but lookups are attempted with `StringId`.

### Category 10: Unused Imports (45 warnings)
**Description:** Many imports are no longer used after refactoring.

**Affected Files:** Multiple files across the codebase

**Examples:**
- `StringTable` imported but not used
- `CompileError` imported but not used
- Various other unused imports

### Category 11: Unused Variables (15 warnings)
**Description:** Variables declared but never used.

**Examples:**
- `config_source_code`, `config_path` in `src/build.rs`
- `module`, `wasm_bytes` in `src/runtime/jit.rs`
- Various timing variables prefixed with `time`

### Category 12: Unnecessary Mutable Variables (30 warnings)
**Description:** Variables declared as `mut` but never mutated.

**Examples:**
- `external_exports` in `src/compiler/parsers/ast.rs`
- `context` in various parser files
- `map` in error macro expansions

## Priority Order for Fixes

1. **High Priority - Blocking Compilation:**
   - Category 1: String Interning / StringId mismatches (28 errors)
   - Category 2: Missing struct fields (5 errors)
   - Category 3: Missing methods (4 errors)
   - Category 4: Missing function parameters (6 errors)
   - Category 5: Wrong field names (3 errors)
   - Category 6: TextLocation vs ErrorLocation (3 errors)
   - Category 7: HashMap type mismatches (4 errors)
   - Category 8: Path type mismatches (2 errors)
   - Category 9: Trait bound issues (2 errors)

2. **Medium Priority - Code Quality:**
   - Category 10: Unused imports (45 warnings)
   - Category 11: Unused variables (15 warnings)
   - Category 12: Unnecessary mut (30 warnings)

## Next Steps

Based on this analysis, the implementation plan should proceed as follows:

1. **Task 2:** Fix string interning and ErrorLocation errors (Categories 1, 6)
2. **Task 3:** Fix missing field and method errors (Categories 2, 3, 4, 5)
3. **Task 4:** Fix type mismatch errors (Categories 7, 8, 9)
4. **Task 5:** Remove unused imports (Category 10)
5. **Task 6:** Remove unused variables and unnecessary mut (Categories 11, 12)

## Baseline Metrics

- **Starting Error Count:** 71 errors
- **Starting Warning Count:** 95 warnings
- **Starting Line Count:** 12,189 lines
- **Target Line Count Reduction:** At least 10% (target: ~10,970 lines or less)
