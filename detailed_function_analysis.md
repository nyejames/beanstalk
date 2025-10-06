# Detailed Function-Level Analysis

## Arena Allocation (src/compiler/mir/arena.rs)

**Category**: 🔴 OPTIMIZATION CODE - Remove (arena allocation is optimization-focused, not core to MIR's purposes)

**Functions**:
- `new` - ✅ Actively used
- `with_capacity` - ⚠️ Limited usage
- `alloc` - ✅ Actively used
- `alloc_slice` - ❌ Potentially unused
- `alloc_raw` - ✅ Actively used
- `allocate_chunk` - ⚠️ Limited usage
- `allocated_size` - ❌ Potentially unused
- `chunk_count` - ❌ Potentially unused
- `drop` - ⚠️ Limited usage
- `new` - ✅ Actively used
- `get` - ✅ Actively used
- `get_mut` - ⚠️ Limited usage
- `deref` - ✅ Actively used
- `deref_mut` - ⚠️ Limited usage
- `new` - ✅ Actively used
- `empty` - ✅ Actively used
- `as_slice` - ⚠️ Limited usage
- `as_mut_slice` - ⚠️ Limited usage
- `len` - ✅ Actively used
- `is_empty` - ⚠️ Limited usage
- `deref` - ✅ Actively used
- `deref_mut` - ⚠️ Limited usage
- `align_up` - ⚠️ Limited usage
- `fmt` - ✅ Actively used
- `new<F>` - ❌ Potentially unused
- `get` - ✅ Actively used
- `put` - ❌ Potentially unused
- `size` - ✅ Actively used
- `max_size` - ✅ Actively used
- `clear` - ⚠️ Limited usage
- `reset` - ⚠️ Limited usage

**Structs**:
- `Arena<T>`
- `Chunk`
- `ArenaRef<T>`
- `ArenaSlice<T>`
- `MemoryPool<T>`

---

## Control Flow Graph (src/compiler/mir/cfg.rs)

**Category**: 🟡 OPTIMIZATION CODE - Investigate (CFG may be used for optimization rather than borrow checking)

**Functions**:
- `new` - ✅ Actively used
- `build_from_function` - ❌ Potentially unused
- `build_linear_cfg` - ⚠️ Limited usage
- `is_function_linear` - ❌ Potentially unused
- `get_successors` - ❌ Potentially unused
- `get_predecessors` - ❌ Potentially unused
- `iter_program_points` - ❌ Potentially unused
- `is_linear` - ✅ Actively used

**Structs**:
- `ControlFlowGraph`

---

## Dataflow Analysis (src/compiler/mir/dataflow.rs)

**Category**: 🟡 OPTIMIZATION CODE - Investigate (dataflow analysis is typically optimization-focused)

**Functions**:
- `new` - ✅ Actively used
- `analyze_function` - ⚠️ Limited usage
- `copy_cfg_from_function` - ⚠️ Limited usage
- `copy_gen_kill_sets` - ⚠️ Limited usage
- `run_forward_dataflow` - ⚠️ Limited usage
- `get_live_in_loans` - ❌ Potentially unused
- `get_live_out_loans` - ❌ Potentially unused
- `is_loan_live_at` - ❌ Potentially unused
- `is_loan_live_after` - ❌ Potentially unused
- `for_each_live_loan_at<F>` - ❌ Potentially unused
- `for_each_live_loan_after<F>` - ❌ Potentially unused
- `get_live_loan_indices_at` - ❌ Potentially unused
- `get_live_loan_indices_after` - ❌ Potentially unused
- `get_statistics` - ❌ Potentially unused
- `handle_control_flow_merge` - ❌ Potentially unused
- `handle_control_flow_branch` - ❌ Potentially unused
- `validate_results` - ⚠️ Limited usage
- `run_loan_liveness_dataflow` - ❌ Potentially unused

**Structs**:
- `LoanLivenessDataflow`
- `DataflowStatistics`

---

## Liveness Analysis (src/compiler/mir/liveness.rs)

**Category**: 🟡 OPTIMIZATION CODE - Investigate (liveness analysis is typically optimization-focused)

**Functions**:
- `new` - ✅ Actively used
- `analyze_mir` - ⚠️ Limited usage
- `analyze_function` - ⚠️ Limited usage
- `copy_cfg_from_function` - ⚠️ Limited usage
- `extract_use_def_sets` - ⚠️ Limited usage
- `extract_statement_uses_defs` - ❌ Potentially unused
- `extract_rvalue_uses` - ⚠️ Limited usage
- `extract_operand_uses` - ✅ Actively used
- `extract_terminator_uses` - ❌ Potentially unused
- `run_backward_dataflow` - ⚠️ Limited usage
- `refine_last_uses` - ⚠️ Limited usage
- `refine_statement_operands` - ⚠️ Limited usage
- `refine_rvalue_operands` - ⚠️ Limited usage
- `refine_terminator_operands` - ⚠️ Limited usage
- `refine_operand` - ✅ Actively used
- `get_live_in` - ❌ Potentially unused
- `get_live_out` - ❌ Potentially unused
- `is_live_at` - ❌ Potentially unused
- `is_live_after` - ❌ Potentially unused
- `get_statistics` - ❌ Potentially unused
- `run_liveness_analysis` - ❌ Potentially unused

**Structs**:
- `LivenessAnalysis`
- `LivenessStatistics`

---

## MIR Extraction (src/compiler/mir/extract.rs)

**Category**: 🟡 UNCLEAR PURPOSE - Investigate (need to determine if this is essential or utility)

**Functions**:
- `new` - ✅ Actively used
- `set` - ✅ Actively used
- `get` - ✅ Actively used
- `clear` - ✅ Actively used
- `intersect_with` - ❌ Potentially unused
- `union_with` - ⚠️ Limited usage
- `union_with_bulk` - ⚠️ Limited usage
- `subtract` - ✅ Actively used
- `subtract_bulk` - ⚠️ Limited usage
- `is_empty_fast` - ✅ Actively used
- `count_ones` - ✅ Actively used
- `for_each_set_bit<F>` - ❌ Potentially unused
- `iter_set_bits` - ❌ Potentially unused
- `clear_all` - ✅ Actively used
- `clear_all_fast` - ✅ Actively used
- `copy_from` - ⚠️ Limited usage
- `clone` - ✅ Actively used
- `capacity` - ✅ Actively used
- `new` - ✅ Actively used
- `extract_function` - ⚠️ Limited usage
- `collect_loans_from_events` - ⚠️ Limited usage
- `generate_loans_from_mir` - ⚠️ Limited usage
- `extract_loan_from_statement` - ⚠️ Limited usage
- `extract_loan_from_terminator` - ⚠️ Limited usage
- `build_place_to_loans_index` - ⚠️ Limited usage
- `build_gen_sets` - ⚠️ Limited usage
- `build_kill_sets` - ⚠️ Limited usage
- `get_gen_set` - ❌ Potentially unused
- `get_kill_set` - ❌ Potentially unused
- `get_loans` - ⚠️ Limited usage
- `get_loan_count` - ❌ Potentially unused
- `get_loans_for_place` - ❌ Potentially unused
- `may_alias` - ✅ Actively used
- `extract_gen_kill_sets` - ❌ Potentially unused

**Structs**:
- `BorrowFactExtractor`
- `BitSet`

---

## WASM Generation (src/compiler/codegen/build_wasm.rs)

**WASM-related functions**: 11
**Validation functions**: 11
**Potentially unused functions**: 2

**Unused functions to investigate**:
- `extract_function_context_from_error<'a>`
- `new_wasm_module`

---

## WASM Encoding (src/compiler/codegen/wasm_encoding.rs)

**WASM-related functions**: 14
**Validation functions**: 3
**Potentially unused functions**: 24

**Unused functions to investigate**:
- `get_string_count`
- `map_global`
- `allocate_local`
- `allocate_global`
- `get_all_locals`
- `get_all_globals`
- `get_local_stats`
- `generate_report`
- `validate_size_limits`
- `validate_module`
- `ensure_function_termination`
- `generate_string_constant`
- `compile_mir_function`
- `add_function_export`
- `add_global_export`
- `add_memory_export`
- `get_lifetime_memory_statistics`
- `get_host_function_index`
- `get_function_index`
- `get_total_function_count`
- `get_host_function_count`
- `get_module_stats`
- `register_function`
- `get_all_functions`

---

