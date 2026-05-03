# Parallel Tokenization/Header Parsing and String Table Plan

## Purpose

This plan implements the parallel frontend preparation model that should follow the major header/dependency/AST and AST refactor work.

The goal is to tokenize and header-parse each source file independently, using per-file `StringTable`s that already contain compiler-owned symbols. The per-file outputs are then merged and remapped into the module/global compilation string table before dependency sorting and AST construction.

This supports the new stage contract:

Header parsing prepares top-level declaration shells, imports, visibility data, and dependency edges.
Dependency sorting produces top-level headers in the order AST needs.
AST consumes sorted top-level shells. It does not discover, bind, or sort them.

Parallelizing this stage is useful because tokenization and header parsing are the last major per-file frontend steps before dependency sorting introduces module-wide ordering. The work is cheap but frequent, and it currently forces shared mutable access to the global `StringTable`.

## Current problem

Tokenizer and header parsing currently take `&mut StringTable` directly. This serializes work across files and spreads string interning into several stages.

Current mutable string-table uses include:

- source identifiers, string literals, raw string literals, template body text, style directives, and path components during tokenization
- synthetic compiler names such as the implicit `start`
- builtin/prelude names such as `Error`, `ErrorKind`, and other compiler-owned symbols
- filesystem/project-derived paths converted into `InternedPath`
- normalized import/path forms created during header/import preparation
- diagnostic or file-level source paths created from `PathBuf`s

Some of these are valid interning sites. Others are churn caused by late string/path construction. This plan separates those cases.

## Target design

Each source file is processed independently:

source file
  -> preseeded local StringTable
  -> tokenize
  -> parse headers
  -> FileFrontendPrepareOutput

Then the module combines the per-file outputs:

```text
Vec<FileFrontendPrepareOutput>
  -> merge local string tables into module/global StringTable
  -> remap all StringIds in tokens, headers, imports, paths, diagnostics, and shell data
  -> aggregate module symbols / visibility / dependency edges
  -> dependency sort
  -> AST
```

No global `Mutex<StringTable>` should be used around the hot path. That would serialize the lexer/header parser and add lock contention.

## Non-goals

- Do not change language semantics.
- Do not change the AST/header/dependency ownership contract.
- Do not use a locked global string table for tokenization/header parsing.
- Do not move expression folding or executable-body type checking into headers.
- Do not parallelize dependency sorting in this plan.
- Do not redesign `DataType` / `TypeEnvironment` here.
- Do not introduce compatibility wrappers or parallel old/new APIs.

## Phase 1 — Audit string interning ownership

### Context

Before adding Rayon, identify which string interning sites are legitimate and which are churn. The first goal is to make string table mutation visible and intentional.

### Implementation steps

1. Audit every `StringTable::intern`, `get_or_intern`, `InternedPath::from_path_buf`, `InternedPath::from_single_str`, `InternedPath::push_str`, and `InternedPath::join_str` call in frontend stages.
2. Classify each call as one of:
   - source text interning
   - compiler-owned fixed symbol
   - project/filesystem-derived path
   - normalized/generated path
   - diagnostic-only path
   - avoidable re-interning/churn
3. Split APIs that only resolve/render strings from APIs that intern new strings.
4. Convert functions from `&mut StringTable` to `&StringTable` where they only read.
5. Add concise comments for mutation points that must remain.
6. Update relevant file docs in tokenizer/header/path modules.
7. Run `just bench`, update the benchmark log if this plan gets its own section, then run `just validate`.

### Audit checklist

- Is every mutable string-table use justified?
- Did read-only functions stop taking `&mut StringTable`?
- Are synthetic/compiler-owned names clearly separated from source text?
- Did the change reduce accidental string-table churn?

## Phase 2 — Pre-intern compiler-owned symbols

### Context

Each per-file table should start with the same compiler-owned symbol universe so common fixed IDs are stable and not repeatedly discovered during parsing.

### Implementation steps

1. Add a `CompilerSymbolSet` or equivalent owned by the frontend setup layer.
2. Pre-intern fixed compiler symbols before per-file processing, including at least:
   - implicit start function name
   - builtin error symbols
   - reserved builtin type names
   - core/prelude names that are compiler-known
   - common path components only if they are truly compiler-owned
3. Add a way to create a local `StringTable` preseeded from the compiler symbol set.
4. Replace late calls such as `join_str(IMPLICIT_START_FUNC_NAME, string_table)` with pre-interned IDs where practical.
5. Keep project/file-derived paths out of the compiler symbol set unless they are fixed language/compiler names.
6. Add tests proving preseeded local tables produce stable IDs for compiler-owned symbols.
7. Run `just bench`, update benchmark notes, then run `just validate`.

### Audit checklist

- Are compiler-owned symbols interned once per local table setup?
- Are source/project-derived names still local to the file/project?
- Did pre-interning simplify synthetic path creation?
- Are builtins still registered once in the correct stage?

## Phase 3 — Add complete StringId remapping support

### Context

Per-file string tables only work if every `StringId` inside per-file outputs can be remapped into the merged module/global table.

### Implementation steps

1. Add or formalize a local trait:

   ```rust
   trait RemapStringIds {
       fn remap_string_ids(&mut self, remap: &StringIdRemap);
   }
   ```

2. Implement remapping for all token/header structures that can carry `StringId`s:
   - `Token`
   - `TokenKind`
   - `FileTokens`
   - `SourceLocation`
   - `InternedPath`
   - `Header`
   - `HeaderKind`
   - `FileImport`
   - `FileReExport`
   - `TopLevelConstFragment`
   - declaration shell types under `declaration_syntax`
   - initializer reference metadata
   - `FunctionSignature`
   - `DataType`
   - generic parameter metadata
   - struct/choice shell payloads
   - any diagnostics/warnings produced during per-file preparation

3. Add tests that create two local tables with overlapping and distinct strings, merge them, remap outputs, and verify all paths/tokens resolve correctly.
4. Ensure remapping does not allocate unnecessary strings beyond the merge itself.
5. Run `just bench`, update benchmark notes, then run `just validate`.

### Audit checklist

- Can every per-file output be safely remapped?
- Are SourceLocations and diagnostics still valid after remapping?
- Are tests covering nested shell data, not only flat tokens?
- Is remapping explicit rather than hidden in ad-hoc conversion code?

## Phase 4 — Introduce per-file frontend preparation output

### Context

Tokenization and header parsing should be grouped as one per-file preparation unit. Module-wide symbol aggregation and dependency sorting remain after recombination.

### Target shape

```rust
struct FileFrontendPrepareOutput {
    source_file: InternedPath,
    file_id: Option<FileId>,
    local_string_table: StringTable,
    tokens: FileTokens,
    headers: Vec<Header>,
    warnings: Vec<CompilerWarning>,
    errors: Vec<CompilerError>,
    top_level_const_fragments: Vec<TopLevelConstFragment>,
    runtime_fragment_count: usize,
    file_re_exports: Vec<FileReExport>,
}
```

The exact type can differ, but the ownership should be explicit.

### Implementation steps

1. Create a per-file preparation function:

   ```rust
   prepare_file_frontend(...)-> FileFrontendPrepareOutput
   ```

2. Let the function tokenize the source file with a preseeded local table.
3. Let the function parse headers from the local `FileTokens` using the same local table.
4. Keep module-wide aggregation out of this function.
5. Make entry-file-only counters explicit:
   - runtime fragment count
   - const top-level fragment placement metadata
6. Avoid shared mutable output buffers in per-file parsing.
7. Preserve diagnostics and warning collection per file.
8. Keep dependency sorting unchanged in this phase.
9. Run `just bench`, update benchmark notes, then run `just validate`.

### Audit checklist

- Is per-file work self-contained?
- Are shared counters removed from the file parser hot path?
- Are warnings/errors collected per file and merged later?
- Is module-wide symbol construction still centralized after recombination?

## Phase 5 — Merge and remap per-file outputs deterministically

### Context

After per-file preparation, all local string tables must be merged into the module/global table and every output must be remapped before aggregation.

### Implementation steps

1. Add deterministic merge ordering, ideally by stable file identity/source path order.
2. Merge each local table into the module/global table using `StringTable::merge_from`.
3. Apply the returned `StringIdRemap` to the entire `FileFrontendPrepareOutput`.
4. Recombine headers, warnings, errors, const fragments, and re-exports in deterministic order.
5. Preserve source-order semantics for the entry file.
6. Ensure same-file constant source-order data remains correct after recombination.
7. Run existing module symbol aggregation on remapped headers only.
8. Add tests for deterministic remapping and output ordering across multiple files.
9. Run `just bench`, update benchmark notes, then run `just validate`.

### Audit checklist

- Are merged outputs deterministic across runs?
- Are all StringIds global before module symbol aggregation?
- Are entry-file runtime/const fragment ordering rules preserved?
- Are diagnostics still renderable through the final string table?

## Phase 6 — Parallelize per-file tokenization and header parsing with Rayon

### Context

Once per-file preparation and remapping are correct, Rayon can be introduced without locking the global string table.

### Implementation steps

1. Use Rayon to process source files in parallel:

   ```rust
   module_files.par_iter().map(prepare_file_frontend)
   ```

2. Keep each worker-owned input immutable or local.
3. Give each worker its own preseeded `StringTable`.
4. Collect outputs into a Vec and sort/merge deterministically.
5. Do not share mutable warnings/errors/string tables across workers.
6. Add detailed timers/counters:
   - `Frontend/parallel file preparation`
   - `Frontend/tokenize file`
   - `Frontend/parse headers file`
   - `Frontend/string table merge/remap`
7. Compare benchmarks against the previous serial implementation.
8. Run `just bench`, update benchmark notes, then run `just validate`.

### Audit checklist

- Is there no global string-table lock in the hot path?
- Is output deterministic despite parallel execution?
- Are errors/warnings stable enough for tests and diagnostics?
- Does parallelism improve or at least not regress small projects badly?
- Is Rayon usage isolated to the orchestration layer?

## Phase 7 — Remove legacy serial scaffolding and update docs

### Context

After the parallel path is correct, remove old serial/token/header plumbing that would become a duplicate implementation path.

### Implementation steps

1. Delete old serial helper paths that are no longer used.
2. Remove compatibility wrappers unless they are test-only and clearly marked.
3. Update file-level doc comments in:
   - tokenizer entry files
   - header parser entry files
   - frontend orchestration
   - string interning module
4. Update `docs/compiler-design-overview.md` to mention:
   - per-file tokenization/header preparation can run in parallel
   - per-file string tables are merged/remapped before module-wide dependency sorting
   - dependency sorting and AST consume remapped global IDs
5. Update `docs/roadmap/roadmap.md` if this plan is completed or if follow-up work is discovered.
6. Run final `just bench`, update benchmark notes, then run `just validate`.

### Audit checklist

- Is there one current implementation path?
- Are old serial paths removed?
- Are docs and module comments accurate?
- Are benchmark results summarized?
- Are generated benchmark results uncommitted?

## Stop criteria

This plan is complete when:

```text
- Tokenization and header parsing run per file using local preseeded StringTables.
- Per-file outputs are remapped into the module/global StringTable before dependency sorting.
- No global StringTable lock is used in the per-file hot path.
- All StringId-bearing token/header structures have explicit remapping support.
- Module symbol aggregation and dependency sorting consume remapped global IDs.
- Output ordering and diagnostics are deterministic.
- Old duplicate serial scaffolding is removed.
- Compiler docs and file doc comments describe the new flow.
- just bench and just validate pass.
```

## Risks

### Risk: remapping misses a nested StringId

Mitigation: add remap tests for nested headers, function signatures, type annotations, struct fields, choices, imports, SourceLocations, and diagnostics.

### Risk: parallel output becomes nondeterministic

Mitigation: sort per-file outputs by stable file identity before merging and aggregating.

### Risk: pre-interning grows into a global symbol dump

Mitigation: only pre-intern compiler-owned fixed symbols. Do not pre-intern project/source names.

### Risk: local tables duplicate many strings temporarily

Mitigation: accept temporary duplication during parallel preparation, then merge/remap. Measure memory/time before optimizing further.

### Risk: header parsing becomes too semantic

Mitigation: headers may parse imports, paths, declaration shells, type surfaces, and constant reference hints. They must not fold expressions, type-check executable bodies, or lower runtime AST nodes.

## Expected implementation order

```text
1. Audit string interning ownership.
2. Pre-intern compiler-owned symbols.
3. Add complete StringId remapping support.
4. Introduce per-file frontend preparation output.
5. Merge/remap per-file outputs deterministically.
6. Parallelize per-file tokenization and header parsing with Rayon.
7. Remove legacy serial scaffolding and update docs.
```

This work should happen after the major AST refactoring plans, because it depends on the corrected header/dependency/AST contract and benefits from AST no longer rebuilding or re-sorting top-level state.
