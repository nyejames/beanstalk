# Beanstalk Phase 5 Implementation Plan

## Title

**Phase 5: Test pruning, coverage consolidation, and final quality audit over Phases 0–3**

## Purpose

Phase 5 turns the previous refactors into a maintainable long-term state.

The goal is not to delete tests for the sake of reducing numbers. The goal is to make the test suite clearer, stronger, less duplicated, and better aligned with the compiler’s real regression risks after:

- Phase 0 guardrails and baseline checks
- Phase 1 no-behavior structural split
- Phase 2 declaration pipeline cleanup
- Phase 3 type/access separation

Phase 5 should produce:

1. A cleaner and more intentional test suite.
2. No stale tests encoding pre-refactor internals.
3. Strong integration coverage for user-visible language behavior.
4. Focused unit tests for internal invariants that integration tests cannot observe directly.
5. A final audit proving the code touched in Phases 0–3 did not drift from the docs or style guide.

---

## Grounding in current repo standards

The codebase style guide says:

- Integration tests are the main regression check.
- Prefer real Beanstalk snippets over narrow isolated tests.
- Unit tests should live in module test directories.
- Once a subsystem is stable, prune outdated unit tests to avoid long-term test bloat.
- Rewriting tests is preferable to carrying obsolete ones forward.
- Failure cases should assert `ErrorType` and useful message fragments.
- Success cases should use strong output assertions where possible.
- Avoid preserving old APIs through wrappers or compatibility shims in pre-alpha.

Relevant current files:

| File / directory | Phase 5 relevance |
|---|---|
| `tests/cases/manifest.toml` | Authoritative list of integration cases and tags |
| `tests/cases/*/expect.toml` | Backend matrix expectations and behavior assertions |
| `src/compiler_tests/integration_test_runner/` | Test harness, fixture loading, assertion logic |
| `src/compiler_tests/integration_test_runner/tests.rs` | Harness contract tests |
| `src/compiler_tests/frontend_pipeline_tests.rs` | Multi-stage frontend pipeline tests |
| `src/compiler_frontend/*/tests/` | Module-specific unit tests |
| `src/compiler_frontend/type_coercion/tests/compatibility_tests.rs` | Type compatibility invariants |
| `src/compiler_frontend/declaration_syntax/tests/type_syntax_tests.rs` | Type syntax parsing invariants |
| `src/compiler_frontend/headers/tests/parse_file_headers_tests.rs` | Header parsing contract |
| `src/compiler_frontend/module_dependencies.rs` + tests | Dependency sorting contract |
| `src/compiler_frontend/hir/tests/` | HIR lowering/validation invariants |
| `src/compiler_frontend/analysis/borrow_checker/tests/` | Borrow checker facts and diagnostics |
| `docs/codebase-style-guide.md` | Style and testing standard |
| `docs/compiler-design-overview.md` | Pipeline/stage ownership contract |
| `docs/memory-management-design.md` | Type/access/ownership contract |
| `docs/roadmap/roadmap.md` | Known gaps and non-alpha deferred work |

---

## Phase 5 scope

Phase 5 has four workstreams:

1. **Inventory and classify tests**
   - Identify duplicate, stale, overlapping, fragile, or under-asserted tests.

2. **Consolidate coverage**
   - Prefer integration cases for user-visible behavior.
   - Keep unit tests only for internal contracts that integration tests cannot check cleanly.

3. **Strengthen missing assertions**
   - Add or improve assertions for declaration pipeline, type/access separation, import visibility, diagnostics, HIR lowering, and borrow facts.

4. **Final refactor quality audit**
   - Review all code touched by Phases 0–3 for drift, style guide compliance, comments, naming, module boundaries, dead code, and no hidden compatibility shims.

---

## Non-goals

Phase 5 must not:

- Introduce new language features.
- Change compiler semantics.
- Rework the test harness unless the audit finds a concrete harness bug or unavoidable assertion gap.
- Delete tests only because they are old.
- Replace integration tests with unit tests.
- Preserve stale pre-refactor tests by weakening assertions.
- Hide failing tests behind ignore flags.
- Add snapshot/golden tests where a behavior assertion would be clearer.
- Reintroduce compatibility wrappers from previous APIs.

---

# Required preconditions

Before starting Phase 5, verify Phases 0–3 are complete.

## Phase 1 checks

Run:

```bash
rg "parse_headers_in_file|HeaderParseContext|HeaderBuildContext" src/compiler_frontend/headers
rg "pub\\(crate\\) mod .*;" src/compiler_frontend/headers/mod.rs
rg "HirNodeId|HirValueId|HirExpressionKind|HirStatementKind|HirTerminator" src/compiler_frontend/hir
```

Expected:

- header responsibilities are split or clearly staged according to the Phase 1 plan
- `headers/mod.rs` acts as the structural map
- HIR node concepts are split or intentionally documented if not split
- no new broad “everything file” has replaced the old one

## Phase 2 checks

Run:

```bash
rg "declaration_stubs_by_path"
rg "DeclarationStub"
rg "DeclarationStubKind"
rg "seed_declaration_stubs"
rg "declaration_stub_from_header"
```

Expected: no matches.

Run:

```bash
rg "build_sorted_declarations" src/compiler_frontend
rg "ModuleSymbols" src/compiler_frontend/headers src/compiler_frontend/module_dependencies.rs src/compiler_frontend/ast
```

Expected:

- dependency sorting is the single producer of sorted declaration placeholders
- AST does not backfill declarations from headers or declaration-stub maps
- constant deferral uses header-derived constant path metadata or equivalent direct metadata, not declaration stubs

## Phase 3 checks

Run:

```bash
rg "Ownership" src
rg "DataType::Collection\\([^\\n]*,"
rg "Collection\\(Box<DataType>,"
rg "struct_ownership"
rg "runtime_struct\\([^\\n]*(Ownership|ValueMode)"
```

Expected:

- no `Ownership` code symbol remains
- `DataType::Collection` no longer carries value/access state
- `DataType::Struct` no longer carries value/access state
- `ValueMode` or equivalent frontend value/access classification exists outside `DataType`

Do not proceed to test pruning if any of these fail.

---

# Workstream 1 — Inventory and classify tests

## Step 1.1 — Generate a test inventory

Create a temporary audit file, not intended to be committed unless useful:

```bash
mkdir -p target/test-audit

find src -path "*tests*" -type f | sort > target/test-audit/unit_test_files.txt
find tests/cases -maxdepth 2 -name expect.toml | sort > target/test-audit/integration_case_files.txt
```

Generate a manifest summary:

```bash
python3 - <<'PY'
from pathlib import Path
import re
manifest = Path("tests/cases/manifest.toml").read_text()
ids = re.findall(r'^id\s*=\s*"([^"]+)"', manifest, flags=re.M)
tags = re.findall(r'^tags\s*=\s*\[([^\]]*)\]', manifest, flags=re.M)
print(f"manifest cases: {len(ids)}")
print("duplicate ids:")
seen = set()
for id in ids:
    if id in seen:
        print(f"  {id}")
    seen.add(id)
print(f"tag entries: {len(tags)}")
PY
```

Required outcomes:

- no duplicate manifest IDs
- every `tests/cases/<case>` directory appears in `manifest.toml`
- every manifest `path` exists
- every fixture has an `expect.toml`
- tags exist and are useful

The integration runner already enforces some of this, but Phase 5 should still inspect the suite intentionally because the goal is maintainability, not only pass/fail.

---

## Step 1.2 — Classify every test category

Use this classification table.

| Category | Keep as integration? | Keep as unit? | Notes |
|---|---:|---:|---|
| User-visible syntax behavior | yes | rarely | prefer `tests/cases` |
| User-visible diagnostics | yes | sometimes | unit only for parser edge diagnostics that are painful end-to-end |
| Import/dependency behavior | yes | yes | integration for behavior, unit for graph ordering/cycle invariants |
| Header parsing contract | sometimes | yes | unit useful for classification and source spans |
| AST pass sequencing | sometimes | yes | unit useful if it checks internal stage contract |
| Type compatibility | sometimes | yes | unit essential for pure type predicate |
| Type/access separation | yes | yes | both needed: unit for identity, integration for behavior |
| HIR lowering shape | sometimes | yes | unit essential for IR shape |
| Borrow checker facts | sometimes | yes | unit essential for facts/snapshots; integration for user behavior |
| JS backend emitted shape | yes | rarely | integration backend-contract cases |
| HTML builder output | yes | rarely | integration artifact/render assertions |
| Harness contract | no | yes | keep in `integration_test_runner/tests.rs` |
| Legacy behavior removed from language | yes, as diagnostics | rarely | keep only if user-facing rejection is intentional |

---

## Step 1.3 — Mark tests by purpose

For every unit test file touched by Phases 0–3, add or verify a short module/test-file comment explaining what contract the file protects.

Good examples:

```rust
//! Tests for pure frontend type compatibility.
//!
//! These stay as unit tests because they validate the central compatibility
//! predicate directly; integration fixtures cover user-facing consequences.
```

```rust
//! Tests for dependency sorting invariants.
//!
//! These stay as unit tests because graph order and cycle handling are easier
//! to diagnose here than through full Beanstalk fixtures.
```

Do not add noisy comments to every individual test. Add one clear file-level comment and rename individual tests where needed.

---

# Workstream 2 — Prune stale and duplicate tests

## Step 2.1 — Identify stale tests from Phases 1–3

Search for old concepts that should not appear in tests anymore.

```bash
rg "declaration_stubs_by_path|DeclarationStub|DeclarationStubKind|seed_declaration_stubs" src tests
rg "Ownership" src tests
rg "DataType::Collection\\([^\\n]*," src tests
rg "struct_ownership" src tests
rg "visibility_scope" src tests
```

Required action:

- If any test references old internals, rewrite it to target the new contract.
- If it only tests an implementation detail that no longer exists, delete it.
- If the behavior is still important, move coverage into an integration fixture or a new invariant test using current concepts.

Do not keep tests that teach future readers the old architecture.

---

## Step 2.2 — Find duplicate unit/integration coverage

For each touched subsystem, compare unit tests against integration tests.

### Declaration pipeline

Relevant behavior should be covered by integration cases:

- imported function visible before use
- imported struct visible before use
- imported constant visible before use
- soft constant dependencies resolve
- bare-file import rejected
- non-exported import rejected
- duplicate declaration rejected
- circular dependency rejected

Keep unit tests only for:

- topological ordering
- cycle detection path
- ambiguous/missing path resolution if not easy to assert through fixtures
- start-function excluded from graph
- deterministic ordering

Remove or rewrite unit tests that duplicate full integration behavior without adding internal diagnostic value.

### Type/access separation

Relevant behavior should be covered by integration cases:

- mutable and immutable bindings of same semantic type are accepted in shared type positions
- mutation still requires explicit mutable access
- mutable call args still require `~`
- non-place `~` still rejected
- immutable place `~` still rejected
- borrow conflicts still rejected

Keep unit tests for:

- `DataType` equality
- `is_type_compatible`
- `is_declaration_compatible`
- type syntax parsing
- HIR type lowering

Delete unit tests that only duplicate “this Beanstalk program compiles” if integration coverage already exists.

### Header split

Keep unit tests for:

- header classification
- import parsing normalization
- top-level const/runtime fragment counting
- non-entry top-level executable code rejection
- explicit start exclusion behavior

Delete or rewrite tests that depend on old function/file locations rather than behavior.

---

## Step 2.3 — Strengthen weak integration assertions instead of adding duplicates

For every integration case created during Phases 0–3:

1. Open `expect.toml`.
2. Verify success cases assert meaningful behavior.
3. Verify failure cases assert `error_type` and `message_contains`.
4. Verify warnings are intentionally `forbid`, `allow`, or `ignore`.

Preferred success assertions:

```toml
[backends.html]
mode = "success"
warnings = "forbid"
rendered_output_contains = ["expected text"]
```

Use golden files only when exact emitted output is contractual.

Preferred failure assertions:

```toml
[backends.html]
mode = "failure"
warnings = "forbid"
error_type = "rule"
message_contains = ["Cannot import", "not exported"]
```

Required action:

- If a case only checks “success” without output/artifact assertions, add a stronger assertion or document why compilation success alone is the contract.
- If a failure case has vague message fragments, tighten them.
- If warnings are ignored without reason, change to `forbid` or add a comment in the plan/audit notes explaining why warning behavior is intentionally unstable.

---

## Step 2.4 — Prune broad smoke tests only when covered by stronger cases

Candidates for review in `tests/cases/manifest.toml` include broad cases such as:

- `declarations_only`
- `declaration_smoke`
- `functions`
- `function_calls`
- `control_flow`
- `constants`
- other older smoke fixtures

Do **not** delete these blindly.

For each broad smoke test:

1. Identify exactly what behavior it uniquely protects.
2. Check whether newer focused fixtures protect the same behavior with stronger assertions.
3. If no unique behavior remains, delete the fixture and manifest entry.
4. If it protects a real broad “all together” scenario, keep it and rename or retag if needed.

A smoke test is worth keeping when it checks realistic interaction between features. It is not worth keeping when it is only an old shallow compilation test.

---

# Workstream 3 — Fill mandatory coverage gaps

This workstream adds tests only where previous phases introduced new contracts that must stay locked.

## Step 3.1 — Declaration pipeline coverage

Required integration coverage after Phase 2:

| Contract | Required coverage |
|---|---|
| Sorted declarations include constants | constant references resolve without AST backfill |
| Sorted declarations include structs | struct references resolve before field resolution |
| Sorted declarations include choices | choice declarations/imports still work |
| Sorted declarations include start | entry start still emits after declarations |
| Start excluded from import graph | bare-file/start import rejected |
| Builtins still available | Error/builtin reserved tests still pass |
| Soft constant dependencies | deferred constant resolution still works |
| Cross-file visibility | imported constants/types respect exports |

If any are missing, add integration cases under `tests/cases`.

Recommended case IDs if not already present:

```text
constant_soft_dependency_resolves_after_placeholder
constant_cross_file_soft_dependency
struct_placeholder_visible_before_resolution
dependency_start_excluded_from_graph
builtin_error_type_still_available_after_dependency_sort
```

Each must include manifest tags. Suggested tags:

```toml
tags = ["integration", "constants", "dependency-resolution"]
tags = ["integration", "structs", "dependency-resolution"]
tags = ["integration", "imports", "entry-start", "diagnostics"]
```

---

## Step 3.2 — Type/access separation coverage

Required unit coverage:

| Contract | Test location |
|---|---|
| collection equality ignores value/access mode because no mode exists in `DataType` | `type_coercion/tests/compatibility_tests.rs` or `datatypes` tests |
| struct equality is nominal and const-record-sensitive only | compatibility/type tests |
| type syntax collection parse produces pure `DataType::Collection(inner)` | `declaration_syntax/tests/type_syntax_tests.rs` |
| HIR collection lowering maps pure frontend collection to pure HIR collection | `hir/tests/` |
| HIR struct lowering maps nominal frontend struct to `HirTypeKind::Struct` | `hir/tests/` |

Required integration coverage:

| Contract | Fixture behavior |
|---|---|
| immutable collection accepted by shared collection parameter | success |
| mutable collection accepted by shared collection parameter | success |
| mutable collection mutation still requires explicit `~` | failure without `~`, success with `~` |
| immutable struct accepted by shared struct parameter | success |
| mutable struct accepted by shared struct parameter | success |
| immutable field mutation rejected | failure |
| mutable field mutation accepted | success |
| overlapping mutable/shared access still rejected | failure |

If existing cases cover these, record them in the audit notes rather than adding duplicates.

---

## Step 3.3 — Header split coverage

Required coverage after Phase 1:

| Contract | Required test type |
|---|---|
| import normalization still works | integration and/or header unit |
| grouped imports still work | integration |
| non-entry top-level executable code rejected | integration |
| top-level const fragments maintain ordering | integration |
| runtime top-level fragments maintain ordering | integration |
| const/runtime fragment interleaving preserved | integration |
| header classification still distinguishes function/struct/choice/constant/start | unit |
| header parsing still emits useful diagnostics | unit or integration |

Search existing cases:

```bash
rg "const_runtime|runtime_const|interleave|top-level|entry-start|import" tests/cases src/compiler_frontend/headers/tests
```

Add coverage only for missing contracts.

---

## Step 3.4 — Test harness coverage

The integration runner already has strong harness tests for:

- missing message fragments on failure fixtures
- legacy top-level expectation rejection
- panic expectation rejection
- manifest tags
- manifest order
- undeclared fixtures
- backend matrix expansion
- backend filtering
- backend-specific golden directories
- normalized golden mode
- rendered output assertions

Phase 5 should keep these tests. Do not prune harness tests unless they duplicate the exact same harness contract.

Required additional harness checks only if missing:

1. manifest duplicate `id` rejected
2. manifest duplicate `path` rejected or deliberately allowed with documented reason
3. invalid tag shape rejected
4. unknown integration tag filtering behavior is deterministic, if tag filtering exists
5. rendered-output assertion failure produces a clear failure reason

If the runner already covers these elsewhere, do not add duplicates.

---

# Workstream 4 — Normalize tags and fixture naming

## Step 4.1 — Tag consistency pass

Audit `tests/cases/manifest.toml`.

Required tag categories:

| Feature | Tags |
|---|---|
| constants | `constants` |
| declarations/type compatibility | `type-checking` or `compatibility` |
| dependency sorting/imports | `imports`, `dependency-resolution` |
| functions | `functions` |
| structs | `structs` |
| collections | `collections` |
| borrow checker | `borrows` |
| diagnostics | `diagnostics` |
| HTML builder | `html` |
| HTML Wasm | `html-wasm` |
| backend contracts | `backend-contract` |
| JS backend | `js-backend` |
| entry/start behavior | `entry-start` |
| control flow | `control-flow` |
| config | `config` |
| adversarial broad cases | `adversarial`, `bug-hunt` |

Required action:

- Add `dependency-resolution` to Phase 2 declaration pipeline cases.
- Add `compatibility` to Phase 3 type/access cases.
- Add `diagnostics` to intentional failure cases missing it.
- Remove misleading tags.
- Keep tag names lowercase and hyphenated.

---

## Step 4.2 — Fixture naming rules

Use names that describe the behavior, not implementation details.

Good:

```text
constant_cross_file_soft_dependency
struct_placeholder_visible_before_resolution
function_call_tilde_on_immutable_place
collection_mutating_method_requires_explicit_receiver_tilde
```

Bad:

```text
phase2_test_1
new_declaration_pipeline_case
ownership_regression
stub_map_removed
```

Required action:

- Rename fixtures whose names reference old internals or phase numbers.
- Update manifest paths/ids.
- Keep fixture IDs stable once the phase lands.

---

# Workstream 5 — Unit test pruning rules by subsystem

## 5.1 Header parsing tests

Keep unit tests when they verify:

- classification of top-level declarations
- body token capture boundaries
- start-function capture behavior
- import token parsing and relative path normalization
- const/runtime fragment counting
- specific syntax diagnostics with exact source locations

Prune unit tests when they:

- only prove a full Beanstalk program succeeds
- duplicate an integration import/declaration fixture
- assert internal helper names or old file structure
- encode `declaration_stubs_by_path` or old symbol package behavior

Required quality bar:

- test names should state the invariant
- no test should require reading implementation code to understand why it exists

---

## 5.2 Dependency sorting tests

Keep unit tests when they verify:

- topological order
- deterministic order for independent headers
- cycle detection
- missing strict dependency diagnostics
- same-file type hint behavior
- start-function exclusion from graph

Prune unit tests when they:

- duplicate cross-file import success already covered by integration
- only assert broad “sort returns something”
- depend on old start-function path-resolution behavior

Required Phase 2 invariant:

- no unit test should expect start to be importable or graph-resolvable

---

## 5.3 AST tests

Keep unit tests when they verify:

- `ScopeContext` lookup rules
- `TopLevelDeclarationIndex` ambiguity/latest-visible behavior
- receiver method catalog behavior
- expression parser edge cases
- call argument validation internals
- type/access separation invariants that integration cannot isolate

Prune unit tests when they:

- duplicate full program behavior better covered in `tests/cases`
- encode old `Ownership` state inside `DataType`
- assert old AST fallback declaration seeding
- test implementation-specific function names after Phase 1 splits

Required Phase 3 invariant:

- unit tests should refer to `ValueMode` only as expression/binding metadata
- unit tests should not imply mutable/immutable variants are different semantic types

---

## 5.4 HIR tests

Keep unit tests when they verify:

- type lowering
- local mutability lowering
- block/terminator shape
- match lowering
- runtime fragment push lowering
- validation failures
- borrow-analysis inputs

Prune unit tests when they:

- duplicate parser/AST behavior
- assert incidental ID numbers unless ID stability is the contract
- require fragile exact full-module dumps where targeted structural assertions would be clearer

Required Phase 3 invariant:

- HIR type tests must show type identity is pure
- mutability/access checks should inspect locals, call passing mode, or borrow facts, not type payloads

---

## 5.5 Borrow checker tests

Keep unit tests when they verify:

- borrow facts
- statement facts
- value facts
- advisory drop sites
- conflict detection
- loop/branch merge behavior
- function summaries

Prune unit tests when they:

- duplicate integration failure cases without checking facts
- assert broad behavior already tested in `tests/cases/borrow_checker_*`
- depend on old type ownership payloads

Required invariant:

- borrow checker tests should use HIR/local/fact terminology, not frontend `Ownership`.

---

## 5.6 Type coercion tests

Keep unit tests for:

- `is_type_compatible`
- `is_declaration_compatible`
- numeric promotion rules
- `Option<T>` compatibility
- `Result<T, E>` compatibility
- `StringSlice` / `Template` compatibility
- `BuiltinErrorKind` compatibility
- collection element compatibility
- struct nominal identity

These are central policy tests and should not be replaced by integration tests alone.

Required Phase 3 additions:

- collection equality/compatibility proves value mode is not type identity
- struct compatibility no longer needs to ignore ownership because ownership is gone

---

# Workstream 6 — Required coverage report

Create a temporary or committed Markdown report depending on roadmap preference:

```text
docs/roadmap/refactor-audit/phase-5-test-coverage-report.md
```

or:

```text
target/test-audit/phase-5-test-coverage-report.md
```

If this is being added to the roadmap, commit it under `docs/roadmap/`.

Required sections:

```markdown
# Phase 5 Test Coverage Report

## Summary

## Integration coverage map

| Contract | Case(s) | Status |
|---|---|---|

## Unit coverage map

| Contract | Test file(s) | Status |
|---|---|---|

## Pruned tests

| Test / fixture | Reason | Replacement coverage |
|---|---|---|

## Strengthened tests

| Test / fixture | Change |
|---|---|

## Remaining intentional gaps

| Gap | Reason | Roadmap item |
|---|---|---|
```

Do not leave “remaining gaps” vague. Each must be tied to:

- deferred roadmap work
- not-alpha scope
- or a specific future implementation plan

---

# Mandatory validation sequence

Run in this order.

## 1. Static stale-concept searches

```bash
rg "declaration_stubs_by_path|DeclarationStub|DeclarationStubKind|seed_declaration_stubs" src tests docs
rg "Ownership" src tests
rg "DataType::Collection\\([^\\n]*," src tests
rg "struct_ownership" src tests
rg "visibility_scope" src tests
```

Any match must be reviewed.

Allowed matches:

- roadmap notes explicitly discussing removed concepts as historical context
- generated/release docs only if intentionally not updated yet

Disallowed matches:

- active source code
- active tests
- comments that still describe removed implementation structures as current

## 2. Test manifest checks

```bash
cargo test integration_test_runner
```

Then:

```bash
cargo run tests
```

The runner should catch manifest/fixture contract problems.

## 3. Unit tests

```bash
cargo test
```

## 4. Clippy

```bash
cargo clippy
```

No new allows should be added without a WHAT/WHY comment.

## 5. Docs and speed sanity

```bash
cargo run --features "detailed_timers" docs
cargo run --release --features "detailed_timers" speed-test.bst
```

These are required because Phases 0–3 touched compiler structure, frontend typing, declarations, and likely docs/build behavior.

## 6. Full one-command validation

If `just` is available:

```bash
just validate
```

If it is not available, run the equivalent command bundle used by the repo.

## 7. Formatting

Run formatting after all semantic checks pass:

```bash
cargo fmt
```

Then verify:

```bash
cargo fmt --check
```

---

# Acceptance criteria

Phase 5 is complete only when all criteria are met.

## Test suite criteria

- [ ] Every integration fixture directory is declared in `tests/cases/manifest.toml`.
- [ ] Every manifest case has useful tags.
- [ ] Failure fixtures assert `error_type` and meaningful `message_contains`.
- [ ] Success fixtures have strong output/artifact assertions where practical.
- [ ] No integration case name references old internals or phase numbers.
- [ ] Broad smoke tests are either justified or removed.
- [ ] Unit tests each protect a clear internal contract.
- [ ] Unit tests that duplicate integration coverage without internal value are removed or rewritten.
- [ ] Type/access separation has both unit and integration coverage.
- [ ] Declaration pipeline cleanup has integration coverage and targeted dependency-sort unit coverage.
- [ ] Test harness contract tests remain strong.

## Code quality criteria

- [ ] No active source code references removed declaration-stub structures.
- [ ] No active source code uses frontend `Ownership`.
- [ ] `DataType` is pure semantic type identity.
- [ ] `ValueMode` or equivalent value/access state is documented outside `DataType`.
- [ ] Header modules retain clear stage boundaries.
- [ ] AST does not rebuild or backfill top-level declarations.
- [ ] Dependency sorting remains the single producer of sorted declaration placeholders.
- [ ] HIR type identity remains pure.
- [ ] Borrow checker facts remain the owner of borrow/access analysis data.

## Style guide criteria

- [ ] New/refactored files have module-level doc comments.
- [ ] Complex functions/structs have concise WHAT/WHY comments.
- [ ] No user-input panics, `todo!`, or `unimplemented!` exist on active frontend paths.
- [ ] No `.unwrap()` exists on user-input-dependent paths.
- [ ] No broad `#[allow(dead_code)]` was added without justification.
- [ ] Imports are at the top unless one-off inline imports are clearly better.
- [ ] Function names are descriptive and not compatibility shims.
- [ ] `mod.rs` files act as structural maps, not implementation dumps.
- [ ] Files split in Phase 1 remain focused and navigable.

---

# Required final review over Phases 0–3

This final review is mandatory and should happen after test pruning/consolidation, not before. The purpose is to catch drift introduced while cleaning tests and while implementing the earlier refactors.

## Review target

Review every file touched in Phases 0–3 and Phase 5.

Generate the file list with Git:

```bash
git diff --name-only origin/main...HEAD > target/test-audit/touched_files.txt
```

If the branch is stacked or `origin/main` is not the right base, compare against the commit before Phase 0 began.

Group files into:

```text
headers
dependency resolution
AST
type syntax / type coercion
HIR
borrow checker
build system
tests / fixtures
docs / roadmap
```

---

## Review 1 — Architecture drift

Check each touched file against the intended pipeline:

```text
Tokenization
→ Header Parsing
→ Dependency Sorting
→ AST Construction
→ HIR Generation
→ Borrow Checking
→ LIR / Backend Lowering
```

Required checks:

- Header parsing does not resolve bodies or perform AST semantic work.
- Dependency sorting does not parse expressions.
- AST construction does not rediscover top-level declarations already owned by headers/dependency sorting.
- HIR lowering does not receive AST-only residue such as unresolved `NamedType`.
- Borrow checking does not mutate HIR.
- Backend/build-system code does not depend on frontend implementation details unless explicitly part of the contract.

Run:

```bash
rg "parse_.*expression|ExpressionKind|AstNode" src/compiler_frontend/headers src/compiler_frontend/module_dependencies.rs
rg "HeaderKind|parse_headers|resolve_module_dependencies" src/compiler_frontend/hir src/backends
```

Any match must be intentional and documented.

---

## Review 2 — Phase 1 structural quality

Required checks:

- Header parsing files are split by task category.
- No new file became a large mixed-responsibility replacement for old `parse_file_headers.rs`.
- `headers/mod.rs` explains module structure.
- HIR files are split or clearly documented according to the Phase 1 decision.
- `mod.rs` files provide a readable map of module flow.

Run:

```bash
wc -l src/compiler_frontend/headers/*.rs
wc -l src/compiler_frontend/hir/*.rs
```

Files over roughly 2000 lines require review. They are not automatically wrong, but they must still represent one coherent operation.

---

## Review 3 — Phase 2 declaration pipeline integrity

Required checks:

- `ModuleSymbols.declarations` is filled only by dependency sorting.
- `AstBuildState::new` directly consumes `module_symbols.declarations`.
- No declaration-stub fallback exists.
- Constants and structs are included in sorted declaration placeholders.
- Builtin declarations are still appended once.
- Start functions remain excluded from dependency graph traversal and importability.

Run:

```bash
rg "declarations = std::mem::take\\(&mut module_symbols.declarations\\)" src/compiler_frontend/ast
rg "build_sorted_declarations" src/compiler_frontend
rg "StartFunction" src/compiler_frontend/module_dependencies.rs src/compiler_frontend/ast/import_bindings.rs
```

Manually inspect the results.

---

## Review 4 — Phase 3 type/access integrity

Required checks:

- `DataType` has no access/ownership payload.
- `ValueMode` or equivalent is documented as frontend value/access metadata.
- Type compatibility uses only semantic type data.
- Mutability/access rules use `ValueMode`, `CallAccessMode`, `CallPassingMode`, HIR local mutability, or borrow facts.
- HIR type lowering does not discard frontend access state because it should not receive any in `DataType`.

Run:

```bash
rg "ValueMode" src/compiler_frontend
rg "CallAccessMode|CallPassingMode" src/compiler_frontend
rg "HirLocal" src/compiler_frontend/hir
rg "BorrowAnalysis|BorrowCheckReport|advisory_drop_sites" src/compiler_frontend/analysis
```

Check that each concept is in the right layer.

---

## Review 5 — Test quality and no stale internal assumptions

Required checks:

- Tests do not mention removed internals.
- Tests assert behavior or stable invariants, not incidental implementation details.
- Integration cases use real snippets.
- Unit tests are not mini integration tests unless they target a hard-to-reach invariant.
- Backend goldens are used only where exact output is contractual.
- Normalized goldens are used only to avoid irrelevant counter churn, not to hide semantic changes.

Run:

```bash
rg "stub|Ownership|phase[0-9]|old|legacy|compatibility wrapper|shim" src tests docs
```

Review all matches. Some words like `compatibility` are legitimate in type coercion and tests, but “compatibility wrapper/shim” should not appear in active code.

---

## Review 6 — Style guide compliance

For every touched source file, check:

- module-level doc comment exists for new files
- public or crate-visible structs/functions have clear names
- complex functions have WHAT/WHY comments
- no unnecessary wrappers or legacy paths
- no user-input panics
- no unexplained `allow(dead_code)`
- imports are clean
- code is formatted

Run:

```bash
rg "panic!|todo!|unimplemented!|unwrap\\(" src/compiler_frontend src/build_system src/projects src/backends
rg "#\\[allow\\(dead_code\\)\\]" src
```

Every match must be classified:

| Classification | Allowed? | Required action |
|---|---:|---|
| internal invariant panic | yes | comment must make invariant clear |
| user-input-triggerable panic | no | replace with structured error |
| test-only unwrap | yes | acceptable in tests if clear |
| production unwrap on proven invariant | maybe | prefer structured error unless blatantly safe |
| planned dead code | maybe | must have clear comment |
| stale dead code | no | delete |

---

## Review 7 — Comment quality

Required checks:

- Comments explain why, not just what syntax does.
- Comments do not describe old architecture as current.
- Comments distinguish semantic type identity from value/access mode.
- Comments distinguish runtime ownership optimization from frontend value mode.
- Comments in tests explain why unit coverage remains when integration coverage exists.

Run:

```bash
rg "ownership|declaration stub|manifest|fallback|temporary|TODO|planned|future" src docs tests
```

Review all matches for accuracy.

This is especially important because the previous phases intentionally removed transitional structures. Comments tend to become the last place old architecture survives.

---

## Review 8 — Diagnostics quality

For touched frontend paths, verify:

- malformed user input returns `CompilerError` / `CompilerMessages`
- diagnostics include source locations
- failure cases assert message fragments
- new internal compiler errors are only for broken invariants
- no panic is used as user-facing validation

Search:

```bash
rg "return_rule_error|return_syntax_error|return_compiler_error|CompilerError::compiler_error|CompilerError::new_rule_error" src/compiler_frontend
```

Manually inspect any new or modified diagnostic code.

---

## Review 9 — Final validation command set

Run the complete validation sequence after all review fixes:

```bash
cargo fmt --check
cargo clippy
cargo test
cargo run tests
cargo run --features "detailed_timers" docs
cargo run --release --features "detailed_timers" speed-test.bst
```

If `just validate` is available and matches this sequence:

```bash
just validate
```

The phase is not complete until this passes.

---

## Review output

The final review should produce a short written note in the roadmap or PR description:

```markdown
## Phase 5 Final Review

- Test pruning/consolidation completed:
  - removed:
  - rewritten:
  - added:
  - strengthened:

- Previous phase drift review:
  - Phase 1 structure:
  - Phase 2 declaration pipeline:
  - Phase 3 type/access separation:

- Style guide review:
  - comments/docs:
  - module boundaries:
  - diagnostics:
  - panic/unwrap/dead-code checks:

- Validation:
  - cargo fmt --check:
  - cargo clippy:
  - cargo test:
  - cargo run tests:
  - docs build:
  - speed test:
```

Do not merge Phase 5 without this review note. It is the explicit proof that the cleanup improved the codebase rather than only moving code and tests around.
