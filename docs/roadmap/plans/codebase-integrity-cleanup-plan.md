# Beanstalk Codebase Integrity Cleanup Plan

## Purpose

Harden a small set of verified compiler correctness, diagnostic, filesystem identity and output-safety issues found during a broad style-guide scan.

This is not a general style sweep. It must not introduce speculative performance work, split stable files without a concrete owner boundary, replace justified internal invariants with noisy error plumbing or overlap the active Template IR migration.

The target result is a compiler that:

- never drops, aliases or silently rewrites filesystem path components
- preserves generic-inference conflicts instead of treating them as missing evidence
- reports the right expected delimiter at declaration-initializer EOF
- keeps release builds protected by the same file-preparation ordering invariants as debug builds
- treats output cleanup metadata, watched-file timestamps and tracked-asset traversal conservatively
- exposes only CLI flags that have real behaviour
- removes the remaining small confirmed style and diagnostic drifts

## Current state

ACTIVE_PLAN: `docs/roadmap/plans/codebase-integrity-cleanup-plan.md`
STATUS: active
CURRENT_SLICE: Phase 5, remove dead CLI flag surface
LAST_ACCEPTED_COMMIT: `a5651e93f`
WORKTREE: `main` at `/Users/aneirinjames/projects/beanstalk/beanstalk`, Phase 4C accepted and awaiting its checkpoint commit
REQUIRED_RELOADS: startup files, this plan and current source/diff
RELEVANT_CONTEXT_NOW:
- docs: CLI help truthfulness and pre-release removal policy
- code: CLI argument parsing, flag representation and direct tests for ignored accepted spellings
ACCEPTANCE_CRITERIA:
- parsed flags with no runtime behavior are removed from parser, enum and help text
- removed spellings are rejected as invalid CLI input
- no compatibility aliases, ignored variants or dead forwarding paths remain
- affected direct CLI tests and help-output assertions cover the current surface
VALIDATION_STATE:
- Phase 1 focused path, Stage 0, source-package, diagnostic-scope and HTML-route tests: passed
- Phase 1 `just validate`: passed, including cross-target Clippy, 3,349 unit tests, 1,758 integration tests, docs and 28 benchmark cases
- Phase 1 separate `just bench-check`: passed, 28/28 cases
- Phase 2A focused generic inference, diagnostic-label and rendering tests: passed
- Phase 2A `just validate`: passed, including cross-target Clippy, 3,350 unit tests, 1,762 integration tests, docs and 28 benchmark cases
- Phase 2B focused scanner and delimiter tests: passed, including both mixed nesting orders
- Phase 2B `just validate`: passed, including cross-target Clippy, 3,360 unit tests, 1,762 integration tests, docs and 28 benchmark cases
- Phase 2C focused constant, header and module dependency tests: passed
- Phase 2C `just validate`: passed, including cross-target Clippy, 3,362 unit tests, 1,762 integration tests, docs and 28 benchmark cases
- Phase 2D focused import-environment and diagnostic-model tests: passed
- Phase 2D `just validate`: passed, including cross-target Clippy, 3,366 unit tests, 1,762 integration tests, docs and 28 benchmark cases
- Phase 3A focused orchestration invariant tests: passed, 19 tests
- Phase 3A `just validate`: passed, including cross-target Clippy, 3,372 unit tests, 1,762 integration tests, docs and 28 benchmark cases
- Phase 3B focused config tests: passed, 68 tests
- Phase 3B `just validate`: passed, including cross-target Clippy, 3,374 unit tests, 1,762 integration tests, docs and 28 benchmark cases
- Phase 3 separate `just bench-check`: passed, 28/28 cases
- Phase 4A focused cleanup tests: passed, 23 tests
- Phase 4A `just validate`: passed, including cross-target Clippy, 3,380 unit tests, 1,762 integration tests, docs and 28 benchmark cases
- Phase 4B focused watch tests: passed, 13 tests
- Phase 4B `just validate`: passed, including cross-target Clippy, 3,384 unit tests, 1,762 integration tests, docs and 28 benchmark cases
- Phase 4C focused tracked-asset tests: passed, 13 tests
- Phase 4C `just validate`: passed, including cross-target Clippy, 3,388 unit tests, 1,762 integration tests, docs and 28 benchmark cases
DOCS_IMPACT: no support-status change expected. This plan and roadmap are user-authorized management records.
BLOCKERS_OR_OPEN_DECISIONS: none. The user's explicit sequence overrides the old post-TIR sequencing note while the TIR exclusion remains binding.
DELEGATION_DECISION: ollama implementation worker - Phase 5 is a bounded dead CLI surface removal
NEXT_WORKER_ORDER: ollama, codex-cli, parent-direct
STOP_REASON: none
NEXT_RESUME_ACTION: commit accepted Phase 4C, then delegate Phase 5 dead flag removal

The audit anchor was `a688cc3be9f2eda49586d298a0fff7f3b4ffcf84`. Every named file must be refreshed against current `main`. Keep a finding only when the same failure mode still exists.

## TIR exclusion

The active TIR plan owns all remaining template representation, parser construction state, control-flow body identity, folding, formatter and handoff cleanup.

This plan must not edit:

- `src/compiler_frontend/ast/templates/**`
- durable `Template` fields or TIR references
- template fold or classification paths
- template control-flow preparation
- TIR tests or fixture ownership
- const/runtime fragment-source representation or fragment ordering checks
- `$md` parser or renderer performance
- template file decomposition

The report's template file-size findings, TIR test conversion, template-head delimiter refactors, template const-eval cloning suggestions and fragment-source `debug_assert!` findings are discarded here. Reassess them only through the active TIR plan or its explicit post-TIR handoff.

## Binding decisions

### Filesystem paths are either exact or rejected

Beanstalk source identity is UTF-8 because paths enter the string table and language import namespace. A filesystem path that cannot be represented exactly must be rejected at the owning boundary.

Do not use:

- `to_string_lossy`
- replacement markers
- empty-string fallbacks
- skipped path components
- generic fallback names such as `main`
- `continue` for an invalid path component
- canonicalization fallback to the uncanonicalized path

Lossy conversion can collapse distinct filesystem names into one compiler identity. Silent skipping can change path shape.

### Keep diagnostic ownership explicit

Use `CompilerDiagnostic` for invalid authored source, config and compile-time path input.

Use `CompilerError` for:

- unreadable or unrepresentable filesystem inputs
- broken compiler maps or ordering invariants
- backend, output and dev-server infrastructure failures

Do not turn a compiler invariant into a user-facing source diagnostic.

### No speculative performance work

Do not retain an optimisation from the scan unless a benchmark or profiler proves material cost. This excludes temporary vector removal, extra allocation avoidance, eager preallocation, JS block clone removal, import candidate scan tuning, parallel asset reads and provider-free single-entry scheduling.

### Remove dead CLI surface

The project is pre-release. A parsed flag with no behaviour is a bug, not compatibility surface. Remove it rather than preserving an ignored spelling.

## Phase 1: Make filesystem identity total

Status: Complete. Filesystem identities now use one fallible exact conversion contract. Stage 0,
source-package preflight and single-file HTML routes reject unrepresentable names without lossy
fallbacks. The obsolete infallible constructor is removed. Focused tests, `just validate` and the
28-case benchmark guard passed.

### Goal

Create one exact conversion contract for filesystem paths and remove every verified silent path fallback in the audited scope.

### Slice 1A: Add a fallible path conversion owner

Replace `InternedPath::from_path_buf` with a fallible filesystem conversion API.

Recommended shape:

```rust
pub(crate) struct NonUtf8PathComponent {
    pub(crate) path: PathBuf,
}

impl InternedPath {
    pub(crate) fn try_from_filesystem_path(
        path: &Path,
        string_table: &mut StringTable,
    ) -> Result<Self, NonUtf8PathComponent>;
}
```

The exact local error shape may differ, but it must:

- retain the original `PathBuf`
- fail on the first non-UTF-8 component
- never mutate the resulting path shape
- remain independent of `CompilerDiagnostic` and `CompilerError`
- let the owning stage map the failure to its correct error channel

Remove the infallible `from_path_buf` API after all call sites migrate. Compiler-owned logical paths should use string/component constructors rather than bypassing the filesystem check.

### Slice 1B: Harden Stage 0 discovery

Update the filesystem-origin path call sites in:

- `src/build_system/create_project_modules/source_tree_index.rs`
- `src/build_system/create_project_modules/collision_detection.rs`
- `src/build_system/create_project_modules/source_package_discovery.rs`
- `src/build_system/create_project_modules/compilation.rs`
- relevant project-structure diagnostic helpers

Required changes:

- reject non-UTF-8 module root names, source filenames, folder names, extensions and source-package prefixes
- stop skipping invalid folder names during collision and package-prefix checks
- stop converting invalid extensions to `""`
- ensure root-file recognition and sibling collision detection see the same exact names
- preserve the offending filesystem path in the error

Use a file/infrastructure error for names discovered directly from the filesystem. Use a typed config diagnostic only when the invalid value came from an authored config key before filesystem discovery.

### Slice 1C: Make source-package preflight strict

Change `prepare_source_package_roots` to return a result and make canonicalization mandatory.

Required behaviour:

1. canonicalize each registered filesystem root
2. return a clear file error when canonicalization fails
3. run `discover_hash_root_file` only on the canonical root
4. canonicalize the discovered root file
5. construct `PreparedSourcePackageRoots` only from successful exact identities

Delete the `unwrap_or_else(|_| path.clone())` fallback. Discovery must never proceed against a path whose canonicalization failed.

Keep missing root, multiple roots and unreadable root as separate existing outcomes.

### Slice 1D: Harden single-file HTML route identity

Update:

- `src/projects/html_project/output_plan.rs`
- `src/projects/html_project/document_shell.rs`
- any direct single-file builder entry that derives a logical route or title

Required behaviour:

- an empty or non-UTF-8 source stem is an explicit error
- do not collapse it to `main`
- strip the cosmetic `#` prefix only after exact UTF-8 conversion
- route-title fallback can assume validated route components or return an explicit internal error if that contract is broken

Directory routes remain directory-based and must not start depending on cosmetic hash-root filenames.

### Tests

Add Unix-only invalid-byte tests with `std::os::unix::ffi::OsStringExt` for:

- `InternedPath` conversion
- source tree module-root discovery
- sibling `.bst` file/folder collision discovery
- project-local source-package prefixes
- single-file extension handling
- single-file HTML route derivation

Add platform-independent tests for:

- canonical source-package root success
- canonicalization failure
- missing, multiple and unreadable public-surface roots
- empty single-file stem rejection where constructible

### Exit criteria

- no filesystem-origin path component is silently dropped
- no invalid extension becomes an empty extension
- no source-package root continues after canonicalization failure
- no HTML route falls back to `main`
- focused path, Stage 0 and HTML route tests pass

## Phase 2: Preserve frontend semantic and diagnostic facts

Status: Complete. Generic nominal inference preserves repeated binding conflicts and both evidence
locations. Declaration-initializer EOF diagnostics track the actual open-construct stack. Constant
classification and canonical source/header positions share one total map. Prelude-injected symbols
no longer manufacture authored source locations, while explicit imports retain theirs. Focused
coverage and `just validate` passed after each slice.

### Slice 2A: Propagate generic nominal binding conflicts

File:

- `src/compiler_frontend/ast/expressions/generic_nominal_inference.rs`

The current nominal constructor path ignores the result of `collect_type_parameter_bindings_typeid`. This erases both structural mismatches and repeated-parameter conflicts.

Refactor nominal inference to use `try_collect_type_parameter_bindings_typeid`, matching the generic function inference owner.

Required behaviour:

- a repeated parameter inferred as different concrete `TypeId`s produces a typed generic application diagnostic
- structural non-matches remain distinct from binding conflicts
- constructor argument evidence uses the argument location
- expected-context evidence uses the constructor or receiving-boundary location
- the first evidence location is retained for a secondary diagnostic label
- matching repeated evidence remains valid
- incomplete evidence still uses the existing cannot-infer path

Reuse the existing `BindingConflict` facts and generic-function conflict diagnostic shape where their semantics match. Do not render type names early.

The two field-pair collection helpers may be consolidated only as part of making result propagation and evidence locations explicit.

Tests:

- two constructor fields bind one parameter to different types
- expected type conflicts with a constructor argument
- choice payload fields conflict
- repeated matching bindings succeed
- a structural mismatch does not get mislabeled as a repeated-binding conflict

### Slice 2B: Report the correct initializer delimiter at EOF

File:

- `src/compiler_frontend/utilities/token_scan.rs`

Replace the fixed `unwrap_or("]")` fallback with an explicit open-construct model.

Required mapping:

- open template expects `]`
- open parenthesis expects `)`
- open collection/map expects `}`
- open `catch:` block expects `;`
- open value-producing `if` block expects `;`

If no construct is open while the scanner believes it is nested, report an internal scanner invariant rather than naming a fabricated delimiter.

Tests:

- EOF in a value-producing `if`
- EOF in a `catch`
- EOF in nested parentheses
- EOF in a collection/map
- EOF in a template

### Slice 2C: Remove the constant-header index fallback

File:

- `src/compiler_frontend/headers/constant_dependencies.rs`

Replace `constant_header_indices.get(&path).copied().unwrap_or(0)`.

Preferred implementation:

- store the canonical source file and header index in one position record built in the same pass as the constant path set
- require the position to exist once a reference has classified as `SourceConstant`
- return or record a precise internal compiler error when that map invariant is broken

Do not convert missing compiler-owned index metadata into a user-facing missing declaration diagnostic.

Tests:

- same-file backward constant reference
- same-file forward constant reference
- cross-file constant reference
- self reference
- unit-level corrupted-map invariant where practical

### Slice 2D: Stop manufacturing a prelude source location

Files:

- `src/compiler_frontend/ast/module_ast/scope_context/lookup.rs`
- `src/compiler_frontend/ast/statements/body_symbol.rs`
- duplicate-declaration diagnostic payload, remap and renderer owners as required

`lookup_visible_external_function_location` must return the actual optional authored import location. Prelude-injected symbols have no previous authored source location.

Required behaviour:

- explicit imports retain a secondary label at the import site
- prelude symbols omit the secondary label
- no `SourceLocation::default()` enters a user-facing label
- the new declaration remains the primary location
- payload remapping and diagnostic model tests cover both forms

Prefer making the existing duplicate-declaration payload's previous location optional over adding a parallel diagnostic kind.

### Exit criteria

- generic nominal conflicts are not silently downgraded
- EOF diagnostics name the real open construct
- dependency ordering cannot silently substitute header zero
- no user diagnostic contains a fabricated empty previous location

## Phase 3: Harden Stage 0 and config invariants

Status: Complete. File-preparation chunks receive release-safe validation before string-table
merging, with malformed ranges, records and coverage returning an infrastructure error. Config
resolver construction uses the canonical config parent, while one authored interned scope owns
tokenization, duplicate classification and validation without diagnostic-time recanonicalization.
Imported support files remain non-entry. Focused tests, `just validate` and the separate 28-case
benchmark guard passed.

### Slice 3A: Validate file-preparation chunk order in release builds

File:

- `src/build_system/create_project_modules/frontend_orchestration.rs`

Convert only these ordering assumptions to release-safe validation:

- each chunk starts at the next expected file index
- each chunk range is non-overlapping and ordered
- each prepared record matches its expected file index
- the final covered index matches the module input length

Return `CompilerError` through the existing `CompilerMessages` boundary when the scheduler payload is malformed.

Do not change:

- const fragment source counts
- runtime fragment source counts
- fragment offset semantics
- template ordering representation

Those remain TIR-owned until its active plan closes.

Tests should construct malformed chunk payloads for:

- gap
- overlap
- wrong internal file index
- missing tail
- valid serial, per-file and chunked order

### Slice 3B: Use one canonical config identity

File:

- `src/build_system/project_config/parsing.rs`

The config path is already canonicalized successfully. Use that result as the only filesystem identity.

Required changes:

- derive the resolver directory from `canonical_config.parent()`
- treat a missing canonical parent as an internal/file-path error
- remove the second canonicalization and fallback to the authored path
- retain the authored spelling only for source locations
- store or pass the exact authored `InternedPath` used during tokenization
- classify authored duplicate diagnostics by interned scope identity rather than recanonicalizing paths during diagnostic handling
- remove `paths_match` once it has no callers

Do not alter the current imported support-file entry semantics. Imported config support files must remain non-entry files.

### Exit criteria

- release builds reject malformed preparation ordering
- no config diagnostic classification performs opportunistic filesystem canonicalization
- config resolver roots always come from the canonical config path
- no fragment/TIR owner is changed

## Phase 4: Make output and watch policies conservative

Status: Complete. V2 manifests require exact normalized managed-extension set equality and preserve
stale files on mismatch. Dev-watch fingerprints propagate modified-time failures with path context
and preserve error kinds. Relative tracked assets reject route traversal above the output root with
an authored compile-time path diagnostic while exact-root traversal remains valid. Focused tests and
`just validate` passed after each slice.

### Slice 4A: Verify manifest extension ownership

File:

- `src/build_system/output_cleanup.rs`

`read_v2_build_manifest` currently validates managed-extension syntax but discards the parsed set.

Required behaviour:

- retain the parsed normalized extension set
- compare it exactly with `active_policy.managed_extensions`
- accept equivalent sets regardless of order or input dot/case normalisation
- enter limited safe mode when the sets differ
- preserve stale files in limited safe mode
- add a distinct `ManifestLimitedSafeModeReason::ManagedExtensionsMismatch`
- include both sets in the warning description where it remains concise

Do not infer that a subset or superset is safe. Exact equality is the conservative v2 contract.

Tests:

- same set in a different order
- same set with case/dot variation
- missing extension
- extra extension
- builder mismatch
- malformed metadata
- no stale deletion after extension mismatch

### Slice 4B: Stop hiding watch timestamp failures

File:

- `src/projects/dev_server/watch.rs`

Replace `metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH)` in exact and directory fingerprint collection.

Preferred behaviour:

- propagate the `modified()` error through the existing `io::Result`
- add path context at the dev-server boundary
- do not use an epoch sentinel that can make same-length edits appear unchanged

If a platform requires operation without modification timestamps, use an explicit fingerprint variant and content hashing. Do not silently treat an unavailable timestamp as a valid epoch.

Tests:

- exact-file fingerprint
- recursive directory fingerprint
- extracted timestamp failure helper
- same-length edit detection on supported platforms

The dev-server output-directory `canonicalize().unwrap_or(resolved)` path is not part of this slice. The output directory may not exist yet, so retaining the resolved path there is intentional.

### Slice 4C: Reject tracked-asset traversal underflow

File:

- `src/projects/html_project/tracked_assets.rs`

When resolving `CompileTimePathBase::RelativeToFile`, track the minimum allowed output-root-relative depth.

Required behaviour:

- `..` may pop a real route component
- `..` that would cross above the output root is rejected
- extra parent segments are never discarded
- the diagnostic points to `RenderedPathUsage::render_location`
- final path validation remains as defense in depth

Use a typed invalid compile-time path diagnostic because the bad traversal originates in authored source.

Tests:

- ordinary relative asset
- nested route asset
- traversal exactly back to output root
- one-segment underflow
- repeated underflow

### Exit criteria

- stale cleanup runs only when builder and extension ownership both match
- watch fingerprints never turn metadata errors into valid timestamps
- tracked asset paths cannot cross or silently clamp to the output root

## Phase 5: Remove dead CLI flag surface

Files:

- `src/projects/cli.rs`
- `src/compiler_frontend/mod.rs`
- CLI tests and help snapshots

Verified drift:

- `--show-warnings` is accepted by build/dev/new parsing but maps to no flag
- `--hide-warnings` maps to `Flag::DisableWarnings` with no production consumer
- `--hide-timers` maps to `Flag::DisableTimers` with no production consumer
- `new html` accepts unrelated build flags and ignores them
- timer output is already owned by `BST_TIMERS`

Required changes:

- remove `--show-warnings`
- remove `--hide-warnings`
- remove `--hide-timers`
- remove `Flag::DisableWarnings` and `Flag::DisableTimers`
- stop `new html` accepting `--release` or `--html-wasm`
- handle version output directly instead of carrying `Flag::Version` into compiler-facing flags
- keep only flags with real build behaviour
- make command validation and flag production one parse path so accepted flags cannot diverge from executed flags
- update help text and unknown-flag messages from the same command-owned definitions where practical

Warnings remain shown by default after successful builds. Timer control remains environment/feature-owned.

Tests:

- every advertised flag changes command behaviour
- every removed flag is rejected
- each command rejects irrelevant flags
- `--version`, build flags, dev options, check `--terse` and new `--force` remain valid
- help text matches the accepted surface

## Phase 6: Finish small verified cleanup

### Slice 6A: Scaffold portability

Files:

- `src/projects/html_project/new_html_project/scaffold.rs`
- `src/projects/html_project/new_html_project/target.rs`

Changes:

- treat trimmed `/dev` and `/dev/` lines as the same existing `.gitignore` rule
- do not use substring matching
- expand only bare `~`, `~/...` and Windows `~\...`
- do not interpret `~other` as the current user's home
- on Windows, resolve home from `USERPROFILE`, then `HOMEDRIVE` plus `HOMEPATH` when `HOME` is absent
- isolate environment lookup behind a small testable helper

Tests cover `.gitignore` whitespace, trailing slash, near matches, Unix tilde forms, Windows separator forms and missing home variables.

### Slice 6B: Exact small prose and internal error fixes

Files:

- `src/backends/wasm/runtime/strings.rs`
- `src/backends/wasm/hir_to_lir/terminator.rs`

Changes:

- replace the em dash in the runtime string module documentation with normal punctuation
- name `HirTerminator::Match` in the unsupported Wasm lowering error

Do not broaden this into a documentation sweep or Wasm refactor.

## Validation

Each slice must run:

```bash
cargo fmt --check
```

Run focused tests for every edited owner, then:

```bash
just validate
```

Also run:

```bash
just bench-check
```

after Phase 1 or Phase 3 because they touch Stage 0 traversal, preparation or path-resolution owners. Performance is a regression guard here, not justification for unrelated optimisation.

Additional checks:

- run Unix invalid-byte tests on Unix CI
- keep platform-independent Windows home-expansion tests free of process-global environment races
- confirm all new diagnostics retain concrete `SourceLocation`
- confirm no production `unwrap_or_default`, lossy conversion or `continue` remains at the audited filesystem identity boundaries
- grep removed CLI flags and verify no help/docs references remain
- inspect the final diff for accidental edits under `src/compiler_frontend/ast/templates/**`

## Completion criteria

This plan is complete when:

- all retained findings are fixed and covered by focused tests
- all path identities are exact or rejected
- generic nominal conflict evidence is preserved
- release file-preparation order is validated
- output cleanup, watch polling and tracked assets fail conservatively
- CLI help, parser and behaviour agree
- the small scaffold and Wasm drifts are closed
- `just validate` passes
- `just bench-check` passes for the relevant phases
- no TIR-owned code or fragment representation changed

## Audit ledger

| Batch | Decision | Final disposition |
|---|---|---|
| 1 | Discard | Unsafe string-table code is documented around stable table-owned storage. External package constructors already separate trusted and fallible paths. Registry and token-stream panics are precise internal invariants. |
| 2 | Discard | HTML allocation claims are unmeasured. Canvas TODOs are backlog. Wasm plan `expect` calls are internal invariants. Template file decomposition belongs to active TIR work. |
| 3 | Partial | Retain the fabricated duplicate location, Wasm prose dash and Match error wording. Discard path-format micro-optimisations. Exclude the TIR test conversion. |
| 4 | Partial | Retain silent `InternedPath` component dropping. Discard clean-file observations and the receiver-method `first()` readability preference. |
| 5 | Partial | Retain non-UTF-8 collision skipping. Const-eval pops are guarded invariants. External JS filenames already include stable hash disambiguation. |
| 6 | Partial | Retain watch timestamp fallback, manifest extension drift and CLI no-op flags. Output-directory canonicalization fallback is intentional before directory creation. Discard trivial allocation and explicit REPL placeholder claims. |
| 7 | Retain | Missing constant index must not map to header zero. |
| 8 | Discard | Worst-case HTML escape preallocation would over-reserve common strings and has no measurement. |
| 9 | Discard | The proposed helper extraction is subjective and does not fix a demonstrated defect. |
| 10 | Discard | HIR dead-code allowances have explicit current justification. |
| 11 | Discard | Capacity-budget mutation consumes the root budget as designed. Broad emitter splitting is speculative. |
| 12 | Discard | Parse-context duplication is small, local and not a correctness problem. |
| 13 | Discard | Boxing keeps the local borrow-check boundary small and follows the style guide's large-error guidance. |
| 14 | Discard | The current `IntDiv` lowering already checks ABI equality. |
| 15 | Discard | `u32 as i32` preserves the Wasm i32 bit pattern. No truncation occurs. |
| 16 | Retain | `.gitignore` `/dev/` detection and cross-platform tilde expansion are real portability drift. |
| 17 | Retain | Generic nominal inference discards binding-conflict results. |
| 18 | Discard | Loading the store immediately after allocation is a precise internal invariant. |
| 19 | Exclude | Template head parser structure belongs to active TIR closure. |
| 20 | Exclude | Template const-eval allocation and lookup work belongs to active TIR closure or measured post-TIR performance work. |
| 21 | Discard | Repeated import candidate scans are real but unmeasured and not roadmap-worthy cleanup. |
| 22 | Retain | The fixed `]` fallback can misreport EOF inside a value-producing block. |
| 23 | Discard | JS function scans and block clones are unmeasured. The clone also avoids mutable-borrow conflicts during emission. |
| 24 | Discard | Returning the first source-package root error is a valid fail-fast policy, not style-guide drift. |
| 25 | Retain | Invalid or empty single-file stems must not collapse to `main`. |
| 26 | Discard | A canonical regular entry file has a usable parent. The fallback is not a user-reachable defect. |
| 27 | Retain | Invalid extension bytes currently collapse to an empty extension. |
| 28 | Partial | Retain release-safe chunk and file ordering validation. |
| 29 | Exclude | Fragment-source counters and ordering remain TIR-owned. |
| 30 | Retain | Config identity comparison should not silently treat canonicalization failure as inequality. |
| 31 | Retain | Use the already canonical config path rather than a second parent/canonicalization fallback path. |
| 32 | Discard | Imported config support files deliberately receive a nonmatching entry sentinel so they remain non-entry files. |
| 33 | Discard | Rayon work cannot promise immediate cancellation while preserving deterministic diagnostics. |
| 34 | Discard | Deterministic result ordering is intentional. Any redundant sort removal requires measurement and a separate performance change. |
| 35 | Retain | Source-package canonicalization failure currently falls back to the original path. |
| 36 | Retain | Hash-root discovery must not run on that fallback path. |
| 37 | Retain | Non-UTF-8 source-package prefixes are silently skipped. |
| 38 | Discard | One `PathBuf` membership allocation per directory check is unmeasured micro-optimisation. |
| 39 | Retain | Source-tree name checks silently omit non-UTF-8 entries. |
| 40 | Discard | The multi-entry provider-free threshold is an explicit design decision that avoids single-module fork/merge overhead. |
| 41 | Discard | Worker-result sorting is deterministic policy. Indexed-write replacement is unmeasured. |
| 42 | Discard | An omitted check path intentionally means the current directory. |
| 43 | Discard | Current collision ownership already retains the source entry. Artifact-class provenance is speculative. |
| 44 | Discard | Directory home selection is explicit. The fallback applies to non-directory builds and is not an unstable multi-page policy. |
| 45 | Retain | Extra `..` segments are silently discarded during tracked-asset route derivation. |
| 46 | Discard | Parallel tracked-asset reads need evidence from media-heavy builds. |
| 47 | Fold into Phase 1 | Deterministic title fallback follows from strict validated route components. |
| 48 | Discard | Apostrophes do not need escaping in HTML text nodes. Attribute escaping is separately double-quote safe. |
| 49 | Discard | `str::lines()` already normalises the trailing terminator, so the helper does not duplicate every existing newline. |
