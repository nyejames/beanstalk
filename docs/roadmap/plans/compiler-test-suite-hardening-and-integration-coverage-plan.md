# Beanstalk Compiler Test Suite Hardening and Integration Coverage Plan

## Purpose

Harden Beanstalk's compiler test suite around one clear ownership rule:

- user-visible language and project behaviour is owned primarily by integration cases under `tests/cases/`
- hidden compiler facts, impossible states, transfer rules, and narrow algorithms remain owned by focused unit tests
- backend units protect deliberate lowering, ABI, helper, and artifact contracts rather than substituting for executed language behaviour
- each semantic behaviour has one primary test owner
- weaker, obsolete, or implementation-shaped duplicates are removed only after a stronger owner exists

The work is deliberately ordered as:

```text
refresh current repository state
-> make the integration harness explicit and auditable
-> strengthen diagnostic and runtime assertions
-> add missing primary integration owners
-> prune superseded units
-> consolidate redundant integration fixtures
-> normalize backend test ownership
-> enforce the final suite policy in validation
```

This ordering is mandatory. Do not begin broad pruning while the harness can still accept implicit compile-only cases, unexpected diagnostics, or weak unordered runtime substrings.

## Current state

ACTIVE_PLAN: `docs/roadmap/plans/compiler-test-suite-hardening-and-integration-coverage-plan.md`
STATUS: active
CURRENT_SLICE: Phase 2B11d correction — make HTML-Wasm trait-facade parity explicitly compile-only
LAST_ACCEPTED_COMMIT: `e31100868` — correct the HTML-Wasm parity contract and record the clean stop
WORKTREE: parent `main` has only this final state refresh diff atop `e31100868`; reusable worker branch is clean and synchronized through merge commit `155dc204a`; do not create another worktree
REQUIRED_RELOADS: startup files, this plan and current source/diff
RELEVANT_CONTEXT_NOW:
- docs: backend matrices need the strongest supported backend-local contract, not artificial runtime symmetry; HTML-Wasm compilation parity may be explicit compile-only when its current `index.html` does not expose the HTML marker
- code: the first attempt passed the focused HTML trait cases but the trait-facade HTML-Wasm artifact lacked the required marker, so all edits were reverted cleanly
ACCEPTANCE_CRITERIA:
- add runtime output to trait-parse, sorted-entry and config-package cases; add an HTML `index.html` assertion and HTML-Wasm compile-only intent to the trait-facade case; mark package aliases compile-only
- preserve sources, existing backend outcomes and warning policy; exclude `identifier_naming_warnings_mixed` and `method_receiver_this_success`
- all six backend blocks, focused runner tests, audit, diff checks and full gate pass
VALIDATION_STATE:
- final TIR at `dc81f7e53`: `just validate` passed with 3,433 Rust tests, 1,784 integration executions, docs checking and 28 benchmark sanity cases
- Phase 0A documentation-only gate: `cargo run --quiet -- build docs --release` passed; 72 files built and no generated diff was produced (`bean` was unavailable in `PATH`)
- Phase 0B operational evidence: three Rust and three integration runs passed; median wall times were 1.53s and 7.89s respectively
- Phase 0B/0C documentation-only gate: `cargo run --quiet -- build docs --release` passed; 72 files built and no generated diff was produced
- Phase 1A: focused 48-test integration-runner suite, `cargo fmt`, `git diff --check` and `just validate` passed; the full gate covered cross-target Clippy, 3,433 Rust tests, 1,784 integration executions, docs checking and 28 benchmark sanity cases
- Phase 1B: focused 56-test integration-runner suite, `cargo fmt`, `git diff --check` and `just validate` passed; the full gate covered cross-target Clippy, 3,441 Rust tests, 1,784 integration executions, docs checking and 28 benchmark sanity cases
- Phase 1C: focused 61-test integration-runner and 57-test CLI suites, real list/case/tag commands, `cargo fmt`, `git diff --check` and `just validate` passed; the full gate covered cross-target Clippy, 3,451 Rust tests, 1,784 integration executions, docs checking and 28 benchmark sanity cases
- Phase 1D: focused 64-test integration-runner and 60-test CLI suites, real audit generation, `cargo fmt`, `git diff --check` and `just validate` passed; the full gate covered cross-target Clippy, 3,457 Rust tests, 1,784 integration executions, docs checking and 28 benchmark sanity cases
- Phase 1 docs: `testing.bd` and `CONTRIBUTING.md` now document manifest metadata, composable filters, listing and audit; the release docs rebuilt successfully and the full `just validate` gate passed with 3,457 Rust tests, 1,784 integration executions and 28 benchmark sanity cases
- Phase 2 sequencing exploration: Codex CLI Spark completed read-only at `f253b3733`; strict enforcement before migration would reject 110 current success blocks, split between 53 fallback-backed cases and 57 explicit weak blocks
- Phase 2A: focused 70-test integration-runner suite, real 1,651-case audit, `cargo fmt`, `git diff --check` and `just validate` passed; main independently repeated the 70 focused tests and audit after integration
- Phase 2B1: all 10 exact HTML cases, 70 focused runner tests, audit, formatting, diff checks and `just validate` passed; the full gate covered 3,463 Rust tests and 28 benchmark sanity cases
- Phase 2B2: all eight exact HTML cases, 70 focused runner tests, audit, formatting, diff checks and `just validate` passed; the full gate covered 3,463 Rust tests, 1,784 integration executions, docs checking and 28 benchmark sanity cases
- Phase 2B3: all four exact HTML cases, 70 focused runner tests, audit, formatting, diff checks and `just validate` passed; the full gate covered 3,463 Rust tests, 1,784 integration executions, docs checking and 28 benchmark sanity cases
- Phase 2B4: all five exact HTML cases, 70 focused runner tests, audit, formatting, diff checks and `just validate` passed; the full gate covered 3,463 Rust tests, 1,784 integration executions, docs checking and 28 benchmark sanity cases
- Phase 2B5: all five exact HTML cases, 70 focused runner tests, audit, formatting, diff checks and `just validate` passed; audit reported 12 compile-only, 78 backend-baseline-only and 23 fallback-backed cases
- Phase 2B6: all four exact HTML cases, 70 focused runner tests, audit, formatting, diff checks and `just validate` passed; the full gate covered 3,463 Rust tests, 1,784 integration executions and 28 benchmark sanity cases
- Phase 2B7: all six exact HTML cases, 70 focused runner tests, audit, formatting, diff checks and `just validate` passed; `struct_using_constant` uses the correct static HTML artifact lane and the full gate passed
- Phase 2B8: all seven exact HTML cases, 70 focused runner tests, audit, diff checks and `just validate` passed; audit reported 17 compile-only, 61 backend-baseline-only, six fallback-backed and zero hard violations
- Phase 2B9: all four exact HTML cases, 70 focused runner tests, audit and diff checks passed in worker and parent review; worker `just validate` passed; audit reported 17 compile-only, 57 backend-baseline-only, two fallback-backed and zero hard violations
- Phase 2B10 exploration: Codex CLI read-only exploration confirmed the helper-chain fixture is stale and the loop mutation case exposes an existing `BST-BORROW-0003` analysis gap; no files changed
- Phase 2B10a: exact helper-chain HTML case, 70 runner tests, audit and diff checks passed in worker and parent review; worker `just validate` passed; audit reported 17 compile-only, 56 backend-baseline-only, one fallback-backed and zero hard violations
- Phase 2B10b exploration: Codex CLI traced the defect to linear expiry in `is_local_active_for_alias_conflict`; HIR aliasing is correct and existing CFG future-use facts own the fix; no files changed
- Phase 2B10b: three focused borrow-loop tests, exact loop-conflict case, 70 runner tests, audit, formatting and diff checks passed in worker and parent review; worker `just validate` passed with 3,466 Rust tests and 1,784 integration executions; audit reported 17 compile-only, 55 backend-baseline-only, zero fallback-backed and zero hard violations
- Phase 2C1: missing expectations now fail with a case-owned error and the fallback parser path, constant and stub expectation are deleted; 70 runner tests, audit, formatting and diff checks passed in worker and parent review; worker `just validate` passed with 3,466 Rust tests and 1,784 integration executions; audit still reports 55 baseline-only blocks that must be migrated before final success-contract enforcement
- Phase 2B11 exploration: Codex CLI accounted for all 55 remaining blocks in nine bounded batches; corrected detailed totals are 38 rendered-output, one artifact, two warning, 13 compile-only and one design-conflict investigation; no files changed
- Phase 2B11a: all seven choice payload-match cases, 70 runner tests, audit and diff checks passed in worker and parent review; worker `just validate` passed with 3,466 Rust tests and 1,784 integration executions; audit reported 17 compile-only, 48 baseline-only, zero fallback and zero hard violations
- Phase 2B11b: all seven choice constructor/constant/import cases, 70 runner tests, audit and diff checks passed in worker and parent review; worker `just validate` passed with 3,466 Rust tests and 1,784 integration executions; audit reported 23 compile-only, 41 baseline-only, zero fallback and zero hard violations
- Phase 2B11c first attempt: receiver rendered output passed, five static/folded page assertions failed with empty dynamic output, and the worker reverted all edits; runner tests, audit and full gate were not run because the stop rule fired; worktree remained clean
- Phase 2B11c corrected retry: all six exact HTML cases, 70 runner tests, audit and diff checks passed in worker and parent review; worker `just validate` passed with 3,466 Rust tests and 1,784 integration executions; audit reported 23 compile-only, 35 baseline-only, zero fallback and zero hard violations
- Phase 2B11d first attempt: focused HTML trait checks passed, but the trait-facade HTML-Wasm `index.html` lacked the required marker; the worker reverted all edits and stopped before broad validation; worktree remained clean
- Phase 2B9 launch: both the optional `beanstalk-spark-explorer` and required `beanstalk-plan-worker` Codex CLI profiles selected `gpt-5.6-luna` but returned the account usage-limit error before repository edits; the service reported retry availability at 2026-07-25 11:08 AM
DOCS_IMPACT: Phase 1 workflow docs, generated release output and the deferred code-block highlighting roadmap note are current; fixture outcomes/backend coverage and the progress matrix remain unchanged
BLOCKERS_OR_OPEN_DECISIONS: none for this slice; diagnostics Phase 4.1c remains serialized at `d7fb3654f`; disk permits at most one additional worktree
DELEGATION_DECISION: codex-cli implementation — explicit user-requested provider and the seven selected backend contracts are source-bounded with assertion lanes established by 2B11c
NEXT_WORKER_ORDER: codex-cli only for this run
STOP_REASON: context is too low to safely launch, review, validate and checkpoint another full worker slice
NEXT_RESUME_ACTION: relaunch Phase 2B11d with HTML artifact and HTML-Wasm compile-only trait-facade contracts

## Recommended roadmap placement and activation conditions

Recommended path:

- `docs/roadmap/plans/compiler-test-suite-hardening-and-integration-coverage-plan.md`

Recommended roadmap order:

```text
Final TIR completion
-> mandatory post-TIR roadmap refresh
-> compiler test-suite hardening and integration coverage
-> canonical module compilation and scoped packages
-> remaining queued implementation chain
```

This plan should be completed before the canonical module compilation plan begins. Its purpose is to make major compiler and build-system refactors safer by moving stable semantic contracts out of implementation-shaped units.

Activation prerequisites:

- [x] `docs/roadmap/plans/final-tir-completion-plan.md` completed its final architecture, test-ownership, documentation, validation and recorded-performance phases at the `1298da468` review anchor.
- [x] The mandatory post-TIR roadmap review refreshed all queued plans against the final one-store, exact-`TirView` architecture at `1298da468`.
- [x] `docs/roadmap/plans/compiler-diagnostics-improvement-plan.md` is complete or explicitly parked at a clean accepted commit.
- [x] No concurrent worker is changing `tests/cases/manifest.toml`, the integration expectation schema, diagnostic payload identity, or the integration runner.
- [x] The repository is on a clean branch/worktree and the current head is recorded in the capsule below.
- [x] The current implementation and progress matrix have been re-read; future end-state design must not be mistaken for current support.

The harness-only phases may technically be independent of later compiler architecture, but the fixture and unit-test inventory is moving during TIR and diagnostics work. Starting before those plans reach clean checkpoints would create avoidable churn and merge conflicts.

---

## Active context capsule

Refresh this block after every accepted slice and immediately before any context compaction. Do not continue from a compressed summary alone.

ACTIVE_PLAN:
- `docs/roadmap/plans/compiler-test-suite-hardening-and-integration-coverage-plan.md`

CURRENT_SLICE:
- Phase: 2
- Checklist item: 2B11d — classify trait, ordering, package-alias and config-package contracts
- Goal: replace six backend baselines with the narrow observable, artifact or compile-only contract supported by each backend
- Non-goals: no source edits, warning-only fixtures, `method_receiver_this_success`, backend outcome changes, schema enforcement, assertion-module split, diagnostic multiplicity work, role/contract backfill or later phases

LAST_GOOD_COMMIT:
- `e31100868` — accepted Phase 2B11d correction and clean resume state

CURRENT_WORKTREE_STATE:
- Clean / known changes: parent `main` has only this final state refresh diff atop `e31100868`; the reusable worker branch is clean and synchronized through merge commit `155dc204a`
- Branch: local `main`
- Dedicated worker worktrees: reuse `/Users/aneirinjames/projects/beanstalk/.worktrees/test-hardening-phase2a`, currently clean at merge commit `155dc204a`; do not create another worktree

RELEVANT_DOCS_THIS_SLICE:
- `AGENTS.md`
- `docs/compiler-design-overview.md`
- `docs/build-system-design.md`
- `docs/src/docs/codebase/style-guide/style-guide.bd`
- `docs/src/docs/codebase/style-guide/testing.bd`
- `docs/src/docs/codebase/style-guide/validation.bd`
- `docs/src/docs/codebase/language/overview.bd`
- `docs/language-overview.md`
- `docs/src/docs/progress/#page.bst`
- `docs/roadmap/roadmap.md`

RELEVANT_CODE:
- `tests/cases/trait_incompatibility_parse_success/expect.toml`: HTML runtime output `ok`; preserve Wasm `BST-RULE-0058`
- `tests/cases/trait_relation_facade_private_private_success/expect.toml`: HTML `index.html` contains the fixture marker and `ok`; HTML-Wasm is explicit compile-only parity
- `tests/cases/entry_start_sees_sorted_declarations/expect.toml`: runtime output `Hello, World`
- `tests/cases/two_package_symbols_same_name_aliases/expect.toml`: explicit compile-only alias resolution/calls
- `tests/cases/config_package_folder_missing_default_ignored/expect.toml`: runtime output `ok`

ACCEPTANCE_CRITERIA:
- three HTML runtime outputs, one HTML static artifact contract and two backend-local compile-only intents are explicit
- no input source, manifest metadata, failure backend contract or other fixture changes
- exact backend blocks, 70 runner tests, audit, formatting, diff checks and `just validate` pass; compile-only rises to 25 and baseline-only falls from 35 to 29

DECISIONS_ALREADY_MADE:
- decision: harden the integration harness before pruning
  - reason: the current harness can accept implicit compile-success cases and unexpected extra diagnostics
  - source/user/date: test-suite review and user request, 2026-07-18
- decision: integration cases are the default owner of user-visible behaviour
  - reason: this is the repository testing standard and permits compiler refactors without changing the semantic contract
  - source/user/date: `testing.bd`, current repository policy
- decision: hidden facts remain unit-tested
  - reason: side-table state, malformed HIR, transfer rules, canonical type identity, and backend planning facts cannot always be observed through output
  - source/user/date: compiler architecture and `testing.bd`
- decision: no arbitrary deletion quota or coverage-percentage target
  - reason: density and ownership matter more than raw counts
  - source/user/date: user request and test-suite review, 2026-07-18
- decision: final TIR R5C owns the completed TIR-specific consolidation; this plan owns only later suite-wide ownership review against those accepted semantics
  - reason: TIR APIs and primary test owners are stable at `1298da468`, so later pruning must treat them like any other subsystem rather than continue migration-only ownership
  - source/user/date: final TIR R5C/R6D and repository review, 2026-07-18
- decision: diagnostics schema migration must not race the active diagnostics plan
  - reason: both plans touch codes, reasons, labels, and canonical fixtures
  - source/user/date: current diagnostics plan state, 2026-07-18
- decision: add the compile-only schema before migrating fixtures, then enforce explicitness and delete the fallback atomically
  - reason: immediate enforcement would reject 110 canonical success blocks, so schema-first enables green family migrations without preserving the permissive path after phase acceptance
  - source/user/date: Phase 1 audit and Codex CLI sequencing exploration, 2026-07-19
- decision: reuse one Phase 2 worker worktree and remove accepted worker checkouts when safe
  - reason: disk space is constrained to at most one additional worktree; serialized runner/fixture slices do not require fresh parallel checkouts
  - source/user/date: explicit user request, 2026-07-19
- decision: migrate the helper-chain fixture without removing current `@./` resolver compatibility
  - reason: accepted import authority selects `@helper/prefix`, while broad resolver migration belongs to the queued canonical-module plan rather than this test-hardening slice
  - source/user/date: Phase 2B10 Codex CLI exploration and current plan bug policy, 2026-07-19
- decision: fix loop-source alias preservation separately from fixture migration
  - reason: stronger coverage exposed a real borrow-analysis defect and the plan forbids combining semantic compiler fixes with unrelated fixture migration
  - source/user/date: Phase 2B10 Codex CLI exploration and current plan bug policy, 2026-07-19
- decision: isolate `choice_import_visibility_non_exported` from expectation migration
  - reason: its current success conflicts with documented private cross-module import rejection and must not be silently encoded
  - source/user/date: Phase 2B11 Codex CLI exploration, 2026-07-19
- decision: keep static/folded page content out of dynamic rendered-output assertions
  - reason: the runner deliberately executes scripts and combines runtime slot/console output only; five static-page assertions returned empty output while the receiver console assertion passed
  - source/user/date: Phase 2B11c Codex CLI blocked attempt and `validate_rendered_output`, 2026-07-19
- decision: classify trait-facade HTML-Wasm parity as compile-only instead of inventing an HTML marker contract
  - reason: focused validation proved the HTML-Wasm `index.html` does not contain the source marker, while the backend still compiles successfully; backend matrices should not force unsupported artifact symmetry
  - source/user/date: Phase 2B11d Codex CLI blocked attempt and testing standards, 2026-07-19

BLOCKERS / RISKS:
- final TIR is accepted and is no longer a blocker; do not reopen it during suite hardening
- compiler diagnostics Phase 4.1c remains incomplete but is parked at clean accepted commit `d7fb3654f` while this plan is active; do not run a concurrent diagnostics worker
- `tests/cases/manifest.toml` is a high-conflict serialized file
- exact diagnostic matching may expose existing cascades and unintended extra diagnostics
- changing the Node runtime harness can alter ordering assumptions if event capture is not designed first
- TIR unit-test paths and counts changed since the original interview anchor and must be re-inventoried from `1298da468`
- a new integration case may reveal a real compiler bug; do not weaken the test to preserve a green gate
- the design documents describe accepted end state while the progress matrix describes current support
- Phase 2B classified all 53 fallback-backed cases; 55 of the original 57 explicit weak blocks remain and must not be blanket-marked compile-only
- the prior Codex CLI capacity blocker cleared for the successful Phase 2B9 retry; continue to use only the explicitly requested Codex CLI provider for this run

VALIDATION_STATE:
- last recorded command: `just validate`, run for accepted Phase 1D in the dedicated worker worktree
- result: passed with cross-target Clippy, 3,457 Rust unit tests, 1,784 integration executions, docs checking, and 28 benchmark sanity cases
- known unrelated failures: none recorded
- Phase 0A `cargo run --quiet -- build docs --release`: passed, 72 files built and no generated diff produced; direct `bean` invocation was unavailable in `PATH`
- Phase 0B/0C `cargo run --quiet -- build docs --release`: passed, 72 files built and no generated diff produced
- Phase 1A `cargo test --quiet integration_test_runner -- --format terse`: passed, 48 tests
- Phase 1A `cargo fmt` and `git diff --check`: passed
- Phase 1B `cargo test --quiet integration_test_runner -- --format terse`: passed, 56 tests
- Phase 1B `cargo fmt` and `git diff --check`: passed
- Phase 1C `cargo test --quiet integration_test_runner -- --format terse`: passed, 61 tests
- Phase 1C `cargo test --quiet cli -- --format terse`: passed, 57 tests
- Phase 1C real `--list`, exact-case/backend and tag/backend commands: passed
- Phase 1C `cargo fmt` and `git diff --check`: passed
- Phase 1D `cargo test --quiet integration_test_runner -- --format terse`: passed, 64 tests
- Phase 1D `cargo test --quiet cli -- --format terse`: passed, 60 tests
- Phase 1D real `bean tests --audit`: passed, 1,651 cases and 1,784 backend executions inventoried with no hard-policy violations and 3,302 missing-classification advisories
- Phase 1D `cargo fmt` and `git diff --check`: passed
- Phase 1 documentation `cargo run --quiet -- build docs --release`: passed, 72 files built
- Phase 1 documentation checkpoint `just validate`: passed with cross-target Clippy, 3,457 Rust tests, 1,784 integration executions, docs checking and 28 benchmark sanity cases
- Phase 2 sequencing exploration: read-only Codex CLI Spark completed; no tests were run and the worktree remained unchanged
- Phase 2A worker `just validate`: passed with the typed success-contract schema; focused runner tests increased to 70 and the audit remained 1,651 cases / 1,784 backend executions with no hard violations
- Phase 2A main integration checks: `cargo test --quiet integration_test_runner -- --format terse` passed 70 tests; `cargo run --quiet -- tests --audit` passed with 1,651 cases and 1,784 executions
- Phase 2B1 selection exploration: read-only Codex CLI Spark selected 10 fallback-backed borrow-validation cases; parent corrected the evidence to six compile-only and four observable cases, including disjoint-field output `30 21`
- Phase 2B1 worker `just validate`: passed with 3,463 Rust tests, 1,784 integration executions, docs checking and 28 benchmark sanity cases; audit reported six compile-only, 100 backend-baseline-only and zero hard violations
- Phase 2B2 selection exploration: read-only Codex CLI Spark selected eight borrow/lifetime cases; parent corrected two cases to existing explicit-weak expectations and corrected `lifetime_inference_integration_basic` from compile-only to observable output
- Phase 2B2 worker `just validate`: passed with 3,463 Rust tests, 1,784 integration executions, docs checking and 28 benchmark sanity cases; audit reported 11 compile-only, 92 backend-baseline-only, 37 fallback-backed and zero hard violations
- Phase 2B3 selection exploration: read-only Codex CLI Spark found 12 observable candidates but could not execute them; parent split the mixed owners, verified the four selected cases compile on current main, and excluded two design-conflict candidates from silent success migration
- Phase 2B3 worker `just validate`: passed with 3,463 Rust tests, 1,784 integration executions, docs checking and 28 benchmark sanity cases; audit reported 11 compile-only, 88 backend-baseline-only, 33 fallback-backed and zero hard violations
- Phase 2B4 worker corrected the nested-catch fallback marker from the explorer's `guest-1.5` guess to source-verified `guest-0`; `just validate` passed with 3,463 Rust tests and 1,784 integration executions; audit reported 11 compile-only, 83 backend-baseline-only, 28 fallback-backed and zero hard violations
- Phase 2B5 worker `just validate` passed; audit reported 12 compile-only, 78 backend-baseline-only, 23 fallback-backed and zero hard violations
- Phase 2B6 worker `just validate` passed with 3,463 Rust tests, 1,784 integration executions and 28 benchmark sanity cases; audit reported 13 compile-only, 74 backend-baseline-only, 19 fallback-backed and zero hard violations
- Phase 2B7 worker changed `struct_using_constant` from an inapplicable runtime assertion to an `index.html` artifact assertion; `just validate` passed and audit reported 13 compile-only, 68 backend-baseline-only, 13 fallback-backed and zero hard violations
- Phase 2B8 worker `just validate` passed; parent repeated the 70 focused runner tests and audit, which reported 17 compile-only, 61 backend-baseline-only, six fallback-backed and zero hard violations
- Phase 2B9 initial Codex CLI exploration and implementation attempts hit their usage limit before edits; the later retry completed through the same requested provider
- Phase 2B9 worker `just validate` passed; parent repeated all four exact HTML cases, 70 runner tests, audit and diff checks; audit reported 17 compile-only, 57 backend-baseline-only, two fallback-backed and zero hard violations

DOCS_IMPACT:
- progress matrix needed: review after every phase that adds, removes, or materially strengthens current coverage; update only when the coverage statement changes
- other docs stale: the test plan itself must be refreshed after TIR; `testing.bd`, `validation.bd`, `CONTRIBUTING.md`, and `index.md` may require changes in later phases
- authorized docs updates: this plan explicitly authorizes the documentation changes listed in each phase; do not broaden language or architecture docs without a discovered contradiction or an intentional accepted behaviour change

NEXT_ACTION:
- relaunch the bounded Phase 2B11d Codex CLI migration with corrected HTML/HTML-Wasm trait-facade ownership

---

## Capsule refresh protocol

After every accepted slice:

- [ ] Set `CURRENT_SLICE` to the next exact checklist item.
- [ ] Record the accepted commit in `LAST_GOOD_COMMIT`.
- [ ] Record `git status --short --branch`.
- [ ] Record every dedicated worker worktree and its branch.
- [ ] Narrow `RELEVANT_DOCS_THIS_SLICE` to the next slice.
- [ ] Narrow `RELEVANT_CODE` to exact files and important symbols.
- [ ] Replace acceptance criteria with the next slice's criteria.
- [ ] Append any new decision with reason, source, and date.
- [ ] Record blockers rather than silently changing scope.
- [ ] Record the exact validation commands and results.
- [ ] Record whether the progress matrix or other docs changed.
- [ ] Set `NEXT_ACTION` to one concrete operation.

Before compaction:

- [ ] Re-read `AGENTS.md`.
- [ ] Re-read every document named in `RELEVANT_DOCS_THIS_SLICE`.
- [ ] Re-read this plan's current phase.
- [ ] Re-read the current implementation and diff.
- [ ] Confirm that the capsule describes the worktree rather than an earlier commit.

---

## Current repository state at the end of the interview

### Repository anchor

At the end of this planning interview:

- repository: `nyejames/beanstalk`
- default branch: `main`
- head: `14456858799607aba43b88d681625e9957ee7dff`
- head commit: `TIR: complete root expression overlays`
- original test-suite review anchor: `3b17bad3fa15607e1909c7f45adff4e2a82df0c2`
- commits since the original review: six
- material changes since the original review: TIR ownership/view/finalization work, TIR-heavy unit-test changes, and documentation/roadmap changes
- integration runner and canonical manifest changes since the original review: none found in the six-commit comparison

The non-TIR findings from the original review therefore remain current at this anchor. TIR-specific unit-test findings must be refreshed because the TIR test suite has been substantially rewritten and pruned during the active TIR plan.

### Active roadmap work at the anchor

- Final TIR completion is active.
- The TIR plan records R2B accepted and R2C transition centralization as the next slice.
- Compiler diagnostics improvements are active.
- The diagnostics plan records Phase 4.1c as the next remaining slice in its current section.
- The current roadmap places canonical module compilation first in the queued implementation chain after the mandatory post-TIR refresh.
- This test plan should be inserted before canonical module implementation.

### Current integration harness shape

The current harness has these properties:

- `ManifestCaseToml` requires non-empty tags.
- `ManifestCaseSpec` retains only `id` and `path`; tags are discarded before execution.
- `TestRunnerOptions` contains only `show_warnings` and an optional backend filter.
- `bean tests` accepts only `--backend <html|html_wasm>`.
- canonical fixtures are loaded in manifest order and expanded into backend executions
- a missing `expect.toml` falls back to `tests/fixtures/stubs/expect.toml`
- both HTML backends have implicit baseline success contracts
- a success fixture can therefore pass with no explicit semantic assertion
- failure fixtures require `diagnostic_codes`, but the validator only checks that every expected code appears somewhere
- unexpected extra diagnostics do not currently fail a fixture
- rendered message fragments are optional
- warning contracts are ignore, forbid, or exact count; warning identities are not asserted
- rendered runtime output supports only unordered contains and not-contains fragments
- artifact assertions support ordered fragments, exact-once fragments, normalized fragments, Wasm validity, exports, and imports
- strict and normalized text goldens are supported
- the Node harness captures console lines and slot output, then combines them rather than retaining one ordered event stream
- failure triage is written under `target/test-reports/`

### Current validation gate

For every code-bearing slice, `just validate` is the required final gate. It currently runs:

```text
cross-target Clippy
Rust unit tests
compiler integration tests
documentation check
benchmark sanity check
```

`cargo fmt` is required separately when Rust changes.

### Current ownership problems to preserve in the inventory

Known likely migration candidates, subject to re-verification at activation:

- `borrow_checker_map_tests.rs` contains source-level language scenarios that should be primarily integration-owned.
- the first branch/match/loop scope tests in `borrow_checker_scope_tests.rs` overlap integration behaviour; the synthetic malformed/dead-local test should remain.
- borrow pipeline tests should remain minimal stage-boundary sentinels.
- `function_call_tests.rs` correctly owns parser shape and source-location facts, but several full-program success/failure tests overlap canonical integration cases.
- `type_resolution_tests.rs` correctly owns canonical `TypeId` and fixed-capacity identity, but some source-diagnostic tests overlap integration cases.
- `generics_tests.rs` correctly owns identity, binding, substitution, and unification, but rendered user-diagnostic tests should move to integration or structured payload assertions.
- JavaScript backend tests correctly own helper/ABI/emission contracts, but some emitted-source checks act as semantic substitutes.
- build-system and command units should remain where they protect output writing, graph policy, custom-builder sequencing, or no-artifact command behaviour.
- TIR test ownership must be taken from the final accepted TIR plan, not from the original review.

These are candidates, not deletion instructions. Every deletion requires replacement evidence.

---

## Goals

- Make every canonical integration case explicit about what it proves.
- Make compile-only acceptance an intentional contract rather than an implicit fallback.
- Make failure fixtures reject unexpected diagnostics by default.
- Assert structured diagnostic reasons and source locations where broad codes are insufficient.
- Assert warning identities instead of count alone where warnings are expected.
- Preserve runtime event order for order-sensitive semantic cases.
- Make case, tag, contract, and backend filtering available from `bean tests`.
- Generate a machine-readable suite inventory and policy report.
- Add missing integration coverage for high-risk language and project invariants.
- Move source-visible behaviour out of source-shaped unit tests after integration ownership exists.
- Retain focused unit tests for hidden compiler facts.
- Consolidate positive micro-fixtures where one readable scenario provides denser coverage.
- Reduce strict goldens that pin incidental generated text.
- Normalize backend units around deliberate lowering contracts.
- Put permanent suite-policy checks into the normal validation gate.
- Record every removed test and its replacement owner.

## Non-goals

- Do not change accepted language semantics merely to make tests easier.
- Do not implement deferred language or backend features.
- Do not turn the suite into a line-coverage or percentage-coverage project.
- Do not delete tests to reduce validation time without ownership evidence.
- Do not treat benchmarks as correctness coverage.
- Do not replace typed assertions with full rendered snapshots.
- Do not make incidental HIR indexes, local IDs, block order, generated names, or temporary lowering order contractual.
- Do not add a second integration runner or a second fixture schema.
- Do not add a broad shared testing utility module for one or two callers.
- Do not add a permanent mutation-testing dependency without a separate justification.
- Do not reopen final TIR architecture.
- Do not broaden Wasm support as part of this plan.
- Do not run fixture-schema migrations concurrently with the diagnostics plan.

---

## Required authority documents

Read before every phase:

- `AGENTS.md`
- `docs/src/docs/codebase/style-guide/style-guide.bd`
- `docs/src/docs/codebase/style-guide/testing.bd`
- `docs/src/docs/codebase/style-guide/validation.bd`
- this plan
- the current diff

Read when the phase touches compiler semantics, diagnostics, HIR, or backend handoff:

- `docs/compiler-design-overview.md`

Read when the phase touches project discovery, command behaviour, entry activation, builders, target planning, outputs, or the test CLI:

- `docs/build-system-design.md`

Read when the phase adds or moves language-behaviour coverage:

- `docs/src/docs/codebase/language/overview.bd`
- the canonical language leaves selected by that overview
- `docs/language-overview.md` only for unmigrated concepts

Read when the phase touches borrow, map aliasing, moves, ownership, GC, drops, or reactive liveness:

- `docs/src/docs/codebase/memory-management/overview.bd`
- `docs/src/docs/codebase/memory-management/access-and-aliasing/overview.bd`
- `docs/src/docs/codebase/memory-management/borrow-validation/overview.bd`
- additional selected memory leaves as directed by the overview

Always review current implementation status:

- `docs/src/docs/progress/#page.bst`

---

## Confirmed plan decisions

### Harness before pruning

No broad unit or integration deletion occurs before Phases 1–5 establish:

- retained manifest metadata
- focused selection commands
- explicit success contracts
- exact diagnostic multiplicity
- structured reason/location assertions
- ordered runtime assertions
- an auditable report

### One primary owner

A behaviour may have secondary coverage only for a distinct:

- compiler-stage boundary
- backend contract
- diagnostic lane or source-location contract
- malformed internal state
- hidden side-table fact
- adversarial composition

Cosmetic variants are not a reason for duplication.

### No arbitrary test-count target

The final suite may still be large. Success is measured by:

- stronger semantic assertions
- clear ownership
- removal of implementation accidents
- fewer redundant compiler invocations
- preserved hidden-invariant coverage
- better local filtering
- improved failure localization

### Exact diagnostics are the default end state

At final acceptance:

- expected diagnostic codes are compared as an exact multiset by default
- duplicates are represented explicitly
- unexpected diagnostics fail
- a weaker contains mode is permitted only with an authored reason
- message fragments are used only when wording or rendered labels are contractual

### Compile-only is explicit

A success backend with no explicit runtime, artifact, golden, or absence assertion must declare an explicit compile-only contract.

Implicit backend shell checks do not count as an authored semantic assertion.

### Reason identity remains compiler-owned

The integration runner must not duplicate diagnostic payload matching.

A stable reason key, when added, is produced by the compiler diagnostic owner from the typed payload and nested reason enum. The runner consumes it.

### Runtime order is captured once

Do not add a second Node harness. Extend the existing harness to retain one ordered event stream and derive exact, ordered, contains, and not-contains assertions from that result.

### Accepted TIR ownership handoff

Final TIR R5C completed the TIR-specific test consolidation and recorded the primary owners. Include
TIR in the later suite-wide audit like any other subsystem, preserve hidden exact-view/preparation/
handoff invariants where integration output cannot expose them, and do not reopen the accepted
architecture merely to reduce test count.

---

## Slice execution rules

### Agent-sized slice rule

A normal accepted slice should have one primary owner and one coherent result.

Prefer one of:

- one harness data-model/parser change
- one CLI/filter/reporting change
- one expectation-policy migration
- one semantic coverage family
- one unit-test ownership family
- one backend operation family
- one documentation and closure checkpoint

Guidance:

- avoid changing more than one major Rust subsystem in a slice
- avoid migrating more than one semantic fixture family in a slice
- keep fixture batches reviewable; split large mechanical migrations by semantic tag or contract, not arbitrary file count alone
- do not combine harness-schema work with semantic compiler fixes
- do not combine adding primary integration coverage and deleting unrelated units
- do not leave two permanent parsing or assertion paths after a phase acceptance
- run the full required validation gate for every accepted code-bearing slice

### Worktree and manifest ownership

- Use one dedicated worktree per implementation worker.
- Record each worktree in the capsule.
- Only one active worker may edit `tests/cases/manifest.toml`.
- Only one active worker may edit expectation schema or runner core files.
- Parallel fixture workers may edit disjoint case folders only.
- A parent/integrator owns manifest updates and final validation.
- Do not copy documentation or implementation references from another worktree unless explicitly authorized.
- Before integrating a worker slice, compare it with current `main` and refresh stale assumptions.

### Bug discovery policy

When stronger coverage exposes a real compiler defect:

- [ ] Stop the deletion or consolidation that revealed it.
- [ ] Determine the accepted semantic owner from current design and progress docs.
- [ ] Record the bug in `BLOCKERS / RISKS`.
- [ ] Add or retain the failing primary integration test.
- [ ] Implement the root-cause fix in a separate coherent slice.
- [ ] Update the progress matrix when current support or rejection changes.
- [ ] Update language, memory, compiler, or build-system design only when accepted behaviour itself changes or existing docs are demonstrably wrong.
- [ ] Never weaken the expected test merely to restore green validation.

When a design document describes end state that is not implemented:

- assert the current supported behaviour or current structured rejection from the progress matrix
- do not implement the deferred feature under this test plan
- record the future gap against its owning roadmap plan

### Deletion evidence

Every removed test must be entered in the pruning ledger with:

- original test path and test name or case ID
- semantic contract it previously attempted to protect
- stronger replacement owner
- retained secondary owner, if any
- reason the removed test was redundant or implementation-shaped
- accepted commit

### Standard accepted-slice gate

For every code-bearing slice:

- [ ] Re-read the required documents.
- [ ] Inspect the current owner and overlapping tests before editing.
- [ ] Decide whether the slice extends, consolidates, replaces, or removes an existing path.
- [ ] Keep one current implementation path.
- [ ] Run `cargo fmt`.
- [ ] Run focused unit and/or integration iteration commands.
- [ ] Run `git diff --check`.
- [ ] Review changed tests against `testing.bd`.
- [ ] Review implementation style against `style-guide.bd`.
- [ ] Review stage ownership and diagnostics where applicable.
- [ ] Review the progress matrix and update it only when coverage/support wording materially changes.
- [ ] Rebuild documentation when source docs changed; never hand-edit `docs/release/**`.
- [ ] Run `just validate`.
- [ ] Perform the `AGENTS.md` final audit.
- [ ] Update the active context capsule.
- [ ] Commit the accepted slice before starting the next slice.
- [ ] State exactly what was and was not validated.

For a strictly documentation-only slice:

- [ ] Confirm every changed file is documentation.
- [ ] Run `bean build docs --release` or the equivalent Cargo command.
- [ ] Inspect changed routes, links, tables, examples, and generated diff.
- [ ] Do not additionally run the code-bearing gate unless the slice contains non-documentation changes.

---

## Test ownership model

| Behaviour | Primary owner | Allowed secondary owner |
|---|---|---|
| Pure data invariant | focused unit near owner | none unless another boundary consumes it differently |
| Parser token/shape algorithm | narrow parser unit | integration case for user-visible acceptance/rejection |
| Compiler-stage invariant | stage-local unit | minimal pipeline sentinel |
| HIR relationship | HIR unit asserting semantic relationships | integration for observable behaviour |
| Malformed HIR/impossible state | HIR/analysis unit | none |
| Side-table borrow/reactivity/drop fact | analysis unit | integration proving the fact is consumed correctly |
| User-visible language behaviour | `tests/cases/` integration | distinct diagnostic/backend/boundary owner |
| Project/module behaviour | `tests/cases/` integration | build-system unit for graph/output policy |
| Command-only no-artifact/UI policy | command/subsystem Rust test | integration only when source behaviour is also involved |
| Backend ABI/helper/emission contract | backend unit or artifact assertion | executed integration for semantic behaviour |
| Cross-backend parity | one integration input with backend blocks | backend units for target-specific representation |
| Exact generated text contract | strict golden or exact artifact assertion | none |
| Adversarial composition | adversarial integration case | focused units for hidden root causes |

---

## Persistent ledgers

Keep these tables current as the plan proceeds.

### Pruning ledger

| Removed test/case | Previous intended contract | Replacement primary owner | Retained secondary owner | Commit |
|---|---|---|---|---|
| _none yet_ |  |  |  |  |

### Coverage gap ledger

| Gap | Accepted current behaviour | Planned owner | Phase | Status |
|---|---|---|---|---|
| Hashmap `get` alias blocks mutation while live | shared map value access prevents overlapping mutation | integration | 6 | queued |
| Cross-module return-alias/effect summaries | public summaries govern caller borrow transfer | integration plus focused summary units | 9 | queued |
| Reactive update after subscription | subscription is not an active borrow; mutation invalidates mounted output | integration plus invalidation-fact units | 11 | queued |
| `check`/`build` frontend diagnostic parity | same frontend contract, no backend/output work for `check` | command tests | 10 | queued |
| Ordered root/fragment execution | active root exactly once; imported root suppressed; fragment order stable | integration | 10 | queued |
| Structured diagnostic source remapping | codes/reasons/locations survive imports and generated boundaries | integration | 4 and 9 | queued |

### Documentation impact ledger

| Phase | Document | Required change | Status |
|---|---|---|---|
| 0 | this plan and `docs/roadmap/roadmap.md` | activate and position plan | queued |
| 1 | `testing.bd`, `CONTRIBUTING.md` | test filters, manifest metadata, audit/list commands | queued |
| 2 | `testing.bd` | explicit `expect.toml` and compile-only policy | queued |
| 3 | `testing.bd` | exact diagnostics and warning identities | queued |
| 4 | `testing.bd`; possibly compiler diagnostics section | structured reason/location contract | queued |
| 5 | `testing.bd` | ordered/exact runtime assertion policy | queued |
| 6–13 | progress matrix | update materially changed coverage summaries | queued |
| 14 | `testing.bd`, `validation.bd`, `CONTRIBUTING.md`, roadmap, progress matrix | final durable policy and closure | queued |

---

## Proposed durable harness contracts

Exact Rust type names may be adjusted to fit the current module structure. The semantic shape is fixed unless a phase audit identifies a clear conflict.

### Manifest metadata

Target manifest shape:

```toml
[[case]]
id = "hashmap_get_alias_blocks_set"
path = "hashmap_get_alias_blocks_set"
tags = ["integration", "maps", "borrows", "diagnostics"]
contract = "language.maps.get_alias_exclusivity"
role = "primary"
```

Initial role vocabulary:

```text
primary
boundary
backend
adversarial
smoke
```

Rules:

- `contract` is a stable dotted semantic identifier, not a test path.
- one contract has at most one `primary` case
- `boundary` proves a separate compiler/build/command handoff
- `backend` proves a target representation or artifact contract
- `adversarial` composes several behaviours and does not replace primary owners
- `smoke` is compile-only or orchestration-only and must be explicit
- contract and role migration may be incremental, but final acceptance requires all non-harness canonical cases to be classified or explicitly exempted as adversarial
- tags remain free-form categories for filtering; they do not replace exact contract ownership

### Success expectation

Behaviour or artifact case:

```toml
[backends.html]
mode = "success"
warnings = "forbid"
rendered_output_exact = "..."
```

Intentional acceptance-only case:

```toml
[backends.html]
mode = "success"
warnings = "forbid"
success_contract = "compile_only"
```

Rules:

- canonical cases always contain `expect.toml`
- missing expectations are errors
- implicit HTML/HTML-Wasm baseline checks do not make a case explicit
- `success_contract = "compile_only"` is rejected when semantic/artifact assertions are also present unless the parser has a clear reason to allow both; prefer one declared intent
- compile-only cases use `role = "smoke"` unless a narrow accepted syntax contract genuinely has no observable output

### Diagnostic expectation

Target shape:

```toml
[backends.html]
mode = "failure"
warnings = "forbid"
diagnostic_match = "exact"
diagnostic_codes = ["BST-RULE-0044"]

[[backends.html.diagnostic_assertions]]
code = "BST-RULE-0044"
occurrence = 1
reason = "invalid_assignment_target.immutable_binding"
path = "input/#page.bst"
line = 7
```

Intentional cascading case:

```toml
diagnostic_match = "contains"
diagnostic_contains_reason = "multi-file recovery intentionally preserves independent provider diagnostics"
diagnostic_codes = ["BST-RULE-0044"]
```

Rules:

- exact multiset is the default end state
- duplicate expected codes represent expected multiplicity
- matching is order-independent unless an explicit order assertion is added later
- unexpected codes fail exact mode
- contains mode requires an authored reason
- structured assertions may add reason, path, line, column, count, and secondary-label checks
- columns are asserted only when token-span precision is the contract
- `message_contains` remains for contractual wording or label rendering, not semantic identity
- reason keys come from compiler-owned typed diagnostic identity, never runner-side text parsing

### Warning expectation

Target shape:

```toml
warnings = "exact"
warning_codes = ["BST-WARN-...."]
```

Rules:

- `forbid` remains the normal default for success and failure cases that expect no warnings
- expected warnings assert exact code multiplicity
- count-only warning contracts are migrated or explicitly justified
- warning message fragments are used only where wording is contractual

### Ordered runtime result

Target expectation fields:

```toml
rendered_output_exact = "..."
rendered_output_contains = ["..."]
rendered_output_not_contains = ["..."]
rendered_output_contains_in_order = ["first", "second", "third"]
rendered_output_contains_exactly_once = ["start-marker"]
```

Runtime result model:

```text
RenderedEvent::Console
RenderedEvent::FragmentInsert
```

Rules:

- the existing Node harness records one chronological event list
- exact output is line-ending normalized but not broadly whitespace-normalized by default
- order-sensitive tests use the ordered event-derived representation
- contains remains valid for cases where unrelated boilerplate is intentionally ignored
- exact-once protects active-root/start/mount contracts
- no Node invocation occurs unless a runtime assertion requires it

### Audit report

`bean tests --audit` writes:

- `target/test-reports/integration_suite_inventory.json`

Minimum report contents:

- repository commit when available
- manifest case count
- expanded backend execution count
- per-case ID, path, tags, contract, role, and backend blocks
- assertion strength per backend
- compile-only status
- diagnostic match mode and structured assertion presence
- warning assertion mode
- golden mode and artifact assertion presence
- missing or duplicate ownership metadata
- duplicate primary contracts
- strict-golden candidates
- weak assertion findings
- normalized expectation-fingerprint groups as review candidates
- hard policy violations and advisory findings

The report identifies candidates. It never automatically deletes or merges tests.

---

# Phase 0 — Re-anchor, inventory, and activate

## Context and reasoning

The original review is still valid for the integration harness, but TIR and diagnostics tests are moving. This phase turns the plan into an accurate reloadable artifact before implementation begins.

This phase is documentation and analysis only. Temporary inventory scripts belong in `/tmp`.

## Preconditions

- [x] Final TIR completion is accepted at the `1298da468` review anchor.
- [x] The mandatory post-TIR plan refresh is complete.
- [x] Diagnostics work is complete or parked cleanly.
- [x] No fixture/harness worker is active.
- [x] Local worktree state is known.

## Recommended slices

### 0A — Refresh repository anchors

- [x] Read all startup and authority documents.
- [x] Record branch, head, status, and worktrees.
- [x] Compare current head with `14456858799607aba43b88d681625e9957ee7dff`.
- [x] Review every changed test and harness path since the interview anchor.
- [x] Refresh the current-state section and capsule.
- [x] Remove stale TIR paths and assumptions.
- [x] Record final TIR architecture and accepted test-ownership handoff.
- [x] Record final diagnostics plan state and any changed codes/reasons.

Accepted Phase 0A record at `691f3338d`:

- `main` is clean, matches `origin/main` and is the only worktree.
- `tests/cases/manifest.toml`, `src/compiler_tests/integration_test_runner/**` and
  `src/projects/cli.rs` have no changes from `144568587` or `1298da468`.
- Every changed test path from `144568587` is under the accepted TIR/frontend ownership review:
  template parser/folding/TIR tests, module-finalization tests, focused const/expression/field/type
  tests, instrumentation tests and shared HIR fixture support. R5C removed implementation-shaped
  duplicates while retaining exact-view, preparation, runtime-handoff and reactive-metadata
  boundary owners. The final audit moved reactive metadata tests out of production source and made
  no integration-harness or canonical-fixture change.
- Final TIR was validated at `dc81f7e53`, closed at `691f3338d` and its historical plan was removed.
  Current code owns one module store, exact `TirView` transitions, one preparation owner and folded
  strings or neutral owned runtime handoff only. Later suite review must not reopen that architecture.
- The diagnostics plan and diagnostic/harness paths have no post-`144568587` changes. Its current
  state remains Phase 4.1b+d complete, Phase 4.1c pending and last accepted at `d7fb3654f`.
  Diagnostics work is serialized at that clean checkpoint while this plan owns the active worktree.
- The requested Codex CLI simple-exploration attempt changed no files and was unavailable because
  the configured Spark profile reached its usage limit. Implementation slices remain assigned to
  the Codex CLI implementation profile as requested.

### 0B — Produce the baseline inventory

Use temporary scripts or shell commands only.

- [x] Record `cargo test --quiet -- --list` output in `/tmp`.
- [x] Record total Rust test count and counts by major test directory.
- [x] Record full `bean tests` execution count by backend.
- [x] Record median unit and integration wall time from three equivalent runs on the same machine; mark as operational evidence, not correctness.
- [x] Count manifest case folders and backend expansions.
- [x] List canonical cases missing `expect.toml`.
- [x] List success blocks with no explicit runtime/artifact/golden/absence assertion.
- [x] List failure blocks using only code-presence assertions.
- [x] List uses of `message_contains`.
- [x] List warning-count expectations.
- [x] List strict and normalized goldens by backend.
- [x] List cases with artifact assertions.
- [x] List current tags and tag frequencies.
- [x] List duplicate or near-duplicate expectation fingerprints.
- [x] Search Rust tests for full-source helpers such as `parse_single_file_ast`, full frontend construction, `build_project`, and direct backend execution.
- [x] Classify each source-shaped unit as hidden invariant, stage boundary, or migration candidate.
- [x] Refresh the known candidate map.
- [x] Record likely high-conflict manifest regions.

Accepted Phase 0B inventory at `b4503794e`:

- `cargo test --quiet -- --list > /tmp/beanstalk-rust-tests-b4503794e.txt` listed
  3,433 tests. Major owners are compiler frontend 2,519, projects 388, backends 227,
  build system 211, compiler integration-runner units 52, builder surface 33 and benchmarking 3.
  Within the frontend, AST owns 1,340, HIR 217, headers 185, paths 118, datatypes 97,
  compiler messages 87, tokenizer 86, declaration syntax 73 and analysis 47.
- Three `/usr/bin/time -p cargo test --quiet -- --format terse` runs passed with real wall
  times 1.53s, 1.43s and 1.71s. The median is 1.53s.
- Three `/usr/bin/time -p cargo run --quiet -- tests` runs passed with real wall times
  8.66s, 7.89s and 7.75s. The median is 7.89s. Each run executed 1,784 contracts:
  1,642 HTML and 142 HTML-Wasm, comprising 805 successful compilations and 979 expected failures.
  These timings are same-machine operational evidence, not correctness or performance proof.
- The manifest declares 1,651 cases and exactly 1,651 canonical input folders. Explicit
  expectations contribute 1,589 HTML plus 142 HTML-Wasm blocks. The fallback stub adds 53 HTML
  blocks, yielding the runner's authoritative 1,784 executions.
- The 53 missing `expect.toml` cases are: `adversarial_borrow_after_result_handler`,
  `adversarial_borrow_nested_loop_aliases`, `adversarial_loop_match_result_chain`,
  `adversarial_multi_bind_in_loops`, `adversarial_multi_file_helper_chain`,
  `adversarial_nested_catch_handlers`, `adversarial_struct_collection_result_interop`,
  `borrow_checker_basic_variables`, `borrow_checker_function_calls`, `borrow_checker_string_memory`,
  `branch_reborrow_after_last_use`, `choice_basic_declaration_and_use`,
  `choice_import_visibility_exported`, `commas`, `complex_borrowing_scenarios`,
  `consistent_borrow_outcomes`, `consistent_ownership_outcomes`, `disjoint_field_borrows`,
  `error_field_access_in_handler`, `function_call_arg_type_correct`,
  `function_call_arg_user_error_kind_struct`, `struct_constructor_named_args_all_named`,
  `struct_constructor_named_args_mixed`, `struct_constructor_named_args_default_skip`,
  `immutable_alias_while_borrowed`, `implicit_main_call`, `last_use_precision`,
  `lifetime_inference_control_flow`, `lifetime_inference_drop_insertion`,
  `lifetime_inference_error_precision`, `lifetime_inference_integration_basic`,
  `lifetime_inference_move_refinement`, `loop_borrow_mutation_conflict`, `mixed_numeric_literals`,
  `multi_bind_explicit_types_and_mutability`, `multi_bind_mixed_existing_and_new_targets`,
  `multi_bind_optional_slots`, `multi_bind_plain_multi_return`,
  `multi_file_fixture_only_counts_entry_case`, `nested_borrowing_patterns`,
  `none_declaration_success`, `none_mutation_success`, `none_return_success`,
  `path_dependent_reborrow`, `print_special_chars`, `result_catch_handler_scope_bubbles_error`,
  `struct_field_borrowing`, `struct_using_constant`, `white_space`,
  `int_promotion_to_float_declaration`, `int_promotion_to_float_return`,
  `int_promotion_to_float_via_function_call` and `external_package_import_selects_correct_package`.
- Authored-contract analysis, which deliberately does not count the implicit backend baseline,
  finds 110 weak success blocks: the 53 fallback blocks plus 57 explicit blocks. The explicit set
  clusters around borrow/lifetime success, choice construction and matching, config/module/facade
  smoke, HTML-Wasm parity, const-record access and Markdown/Beandown parity. Phase 2B must classify
  each as observable behaviour, artifact contract or intentional compile-only smoke rather than
  blanket-marking it compile-only.
- All 979 failure blocks assert diagnostic codes. Of those, 739 use code presence alone and 240
  additionally use `message_contains`. Twenty-one backend blocks assert warning counts. Exact
  diagnostic multiplicity, structured reasons and warning identity do not yet exist.
- Golden ownership consists of 38 normalized HTML blocks/files and no strict or HTML-Wasm goldens.
  Artifact ownership consists of 250 backend blocks across 245 cases with 287 assertions. One
  backend block uses `artifacts_must_not_exist`.
- The manifest has 155 unique tags. Highest frequencies are integration 1,629, diagnostics 884,
  language 361, imports 245, templates 203, generics 150, choices 133, functions 128, results 94,
  traits 89, control-flow 83, collections 78, aliases 72, receiver-methods 66, JS-backend 64,
  cast 61, pattern-matching and value-blocks 60 each, external-packages 57, config 56, HTML 52,
  structs 49, constants 46, facades 45, facade 44, fixed-collections 41, borrows 40,
  reactivity 37, namespace-imports 33 and hashmaps 32. Thirty-one tags are singletons. Phase 14
  must normalize visible spelling families including facade/facades, cast/casts,
  optionals/options/optional, receiver-methods/receivers, struct/structs, function/functions,
  import/imports, namespace/namespaces, choice/choices and loop/loops.
- Exact expectation fingerprints form 152 repeated groups containing 1,087 backend blocks. The
  largest groups contain 100, 47, 44, 38, 35, 34, 31, 24, 22 and 22 blocks. Shape-only fingerprints
  form 23 groups containing 1,773 blocks, led by code-only failures, unordered rendered-output
  success and artifact-only success. These are review candidates, not automatic merge evidence.
- Source-shaped helper search found 364 hits. `parse_single_file_ast` appears in 23 frontend test
  files. Full frontend construction appears in the borrow-checker pipeline suite. `build_project`
  is concentrated in build-system policy tests.
- Retain hidden-invariant owners for borrow facts, malformed HIR, parser shape and locations,
  canonical type identity, TIR exact-view/preparation/handoff facts and backend ABI/helper contracts.
  Retain minimal stage-boundary owners for borrow-pipeline propagation and build/check orchestration.
  Migration candidates remain `borrow_checker_map_tests.rs`, ordinary source scenarios in the
  beginning of `borrow_checker_scope_tests.rs`, selected whole-program cases in
  `function_call_tests.rs` and `type_resolution_tests.rs`, rendered-diagnostic cases in
  `generics_tests.rs` and JavaScript emission tests acting as semantic substitutes. The broad
  source-shaped AST statement suites require case-by-case later ownership review, not Phase 0 deletion.
- Serialized manifest conflict regions are adversarial lines 2-37, choice equality 532-567,
  fixed collections 1,632-1,727, hashmaps 2,662-2,727, diagnostics 5,558-5,628,
  value blocks 7,323-7,358 and reactivity 7,763-7,833.

### 0C — Activate in the roadmap

- [x] Add this plan to `docs/roadmap/plans/`.
- [x] Link it from `docs/roadmap/roadmap.md` before canonical module compilation.
- [x] Record whether this plan is queued or active.
- [x] Record that canonical module implementation is blocked until this plan closes.
- [x] Refresh the capsule with the first Phase 1 slice.
- [x] Commit the documentation-only activation checkpoint.

## Phase 0 audit

- [x] Every count names the exact commit and command.
- [x] TIR-specific facts come from the final accepted TIR state.
- [x] Diagnostics facts come from the final/parked diagnostics state.
- [x] No temporary inventory script was committed without a permanent owner.
- [x] No test was changed or deleted.
- [x] No implementation claim is inferred from end-state design alone.

## Style-guide review

- [x] Plan language distinguishes current implementation from accepted end state.
- [x] Paths name current owners.
- [x] No obsolete compatibility or migration owner is carried forward.
- [x] The next slice is small and exact.

## Validation

Documentation-only gate:

```bash
bean build docs --release
```

or:

```bash
cargo run --quiet -- build docs --release
```

Inspect the generated documentation diff and confirm only documentation changed.

## Documentation impact

Required:

- this plan
- `docs/roadmap/roadmap.md`

No progress-matrix change is required for inventory alone.

## Phase 0 acceptance

- [x] The plan is current, active/queued correctly, and points at the exact repository head.
- [x] The baseline inventory is recorded.
- [x] All active-plan conflicts are resolved.
- [x] Phase 1 can begin without relying on the original review's stale TIR paths.

---

# Phase 1 — Retain manifest metadata and add focused test selection

## Context and reasoning

Tags are currently mandatory but discarded. The runner can filter only by backend. Large-suite pruning and semantic ownership need stable IDs, tags, contracts, roles, listing, and focused execution.

This phase changes runner metadata and developer workflow only. It does not change fixture outcomes.

## Primary owners

- `src/compiler_tests/integration_test_runner/types.rs`
- `src/compiler_tests/integration_test_runner/manifest.rs`
- `src/compiler_tests/integration_test_runner/fixture.rs`
- `src/compiler_tests/integration_test_runner/runner.rs`
- `src/compiler_tests/integration_test_runner/reporting.rs`
- `src/compiler_tests/integration_test_runner/tests/`
- `src/projects/cli.rs`
- `tests/cases/manifest.toml`

## Non-goals

- no expectation-schema changes
- no diagnostic matching changes
- no semantic fixture deletion
- no bulk contract backfill yet

## Recommended slices

### 1A — Split harness self-tests before expansion

- [x] Convert the monolithic harness test file into a focused test module directory.
- [x] Use files such as manifest, expectations, fixture, assertions, execution, and reporting tests.
- [x] Keep test helpers local to the smallest owner.
- [x] Delete the old monolithic path after wiring the new module.
- [x] Do not change behaviour in this slice.

Accepted at `58dd13f98`: 41 tests and five local helpers moved into manifest, fixture,
expectation, assertion and execution owners; no reporting self-tests existed to move. The original
and split function-name inventories were identical, the focused runner still executed 48 tests,
and `just validate` passed. `index.md` already points at the runner directory, so the path move did
not require a locator update.

### 1B — Retain case identity and metadata

- [x] Add a distinct canonical case ID field to `TestCaseSpec`; do not parse IDs back out of display names.
- [x] Retain tags in `ManifestCaseSpec` and `TestCaseSpec`.
- [x] Add optional `contract`.
- [x] Add optional typed `role`.
- [x] Validate role spelling.
- [x] Validate that a primary role has a contract.
- [x] Validate duplicate primary contracts among currently classified cases.
- [x] Preserve manifest order.
- [x] Preserve one input with backend-specific expansions.

Accepted at `d4daec916`: manifest metadata is retained through every backend-expanded case using one
typed `CaseRole`; existing unclassified cases remain valid. Focused tests cover all five role
spellings, primary-contract requirements, duplicate-primary rejection, allowed shared non-primary
contracts, metadata retention, ordering and backend expansion. `just validate` passed with 3,441
Rust tests and the unchanged 1,784 integration executions.

### 1C — Add filter and list options

- [x] Extend `TestRunnerOptions` with exact case, repeated tag, contract, backend, and list mode.
- [x] Define repeated `--tag` semantics as logical AND.
- [x] Make `--case` exact, not fuzzy.
- [x] Make filters compose deterministically.
- [x] Add `bean tests --case <id>`.
- [x] Add repeatable `bean tests --tag <tag>`.
- [x] Add `bean tests --contract <contract>`.
- [x] Retain `bean tests --backend <backend>`.
- [x] Add `bean tests --list`.
- [x] Reject incompatible or duplicated CLI arguments with clear tooling errors.
- [x] Add CLI parser tests in the existing external test-file style; do not place a large inline test module in production code.
- [x] List case ID, selected backend blocks, tags, contract, and role.

Accepted at `a56133b0d`: one `TestRunnerOptions` path now owns exact ID, logical-AND tags,
contract, backend and list selection; the backend-only entry point was removed. Listing groups selected
backend blocks with canonical metadata without entering execution. Focused runner/CLI tests and real
commands passed, followed by `just validate` with 3,451 Rust tests and 1,784 integration executions.

### 1D — Add the audit-report skeleton

- [x] Add `bean tests --audit`.
- [x] Make audit load and validate the entire suite without compiling cases.
- [x] Reject combining `--audit` with filters.
- [x] Write the initial machine-readable inventory report.
- [x] Include hard schema violations and advisory missing-classification findings.
- [x] Keep audit report construction in reporting/test infrastructure, not compiler semantics.
- [x] Reuse existing `serde_json`.
- [x] Do not add a new crate unless the current dependencies cannot express the report.

Accepted at `120c4baea`: audit branches after one complete suite load and before selection/execution,
writes the versioned reporting-owned inventory, and records 1,651 cases plus 1,784 backend blocks.
The current manifest has no hard-policy violations and intentionally reports 1,651 missing-contract
plus 1,651 missing-role advisories for later classification. Focused tests and `just validate`
passed with 3,457 Rust tests and unchanged integration outcomes.

## Phase 1 audit

- [x] Tags reach the runner and report.
- [x] Case IDs are first-class.
- [x] No code reconstructs metadata from display text.
- [x] One manifest parser remains.
- [x] Filtered execution preserves canonical order.
- [x] Backend expansion remains one expectation matrix per input.
- [x] Audit does not compile or mutate fixtures.
- [x] Existing unclassified cases continue to run, but duplicate classified primary contracts fail.

## Style-guide review

- [x] New option state uses named structs/enums rather than boolean-heavy APIs.
- [x] CLI parsing remains explicit and readable.
- [x] Test modules have clear owners.
- [x] No broad utility module was created.
- [x] New files have concise WHAT/WHY documentation.

## Validation

Iteration:

```bash
cargo test --quiet integration_test_runner -- --format terse
cargo test --quiet cli -- --format terse
cargo run --quiet -- tests --list
cargo run --quiet -- tests --case arithmetic_operator_precedence --backend html
cargo run --quiet -- tests --tag borrows --backend html
cargo run --quiet -- tests --audit
```

Final:

```bash
cargo fmt
git diff --check
just validate
```

## Documentation impact

- [x] Update `testing.bd` with manifest metadata, roles, contracts, filters, list, and audit.
- [x] Update `CONTRIBUTING.md` with focused test commands.
- [x] Update `index.md` if the harness test module path moves. No edit was required because it already names the runner directory.
- [x] Review the progress matrix; no change is expected unless its coverage workflow text is materially affected.

## Phase 1 acceptance

- [x] Case/tag/contract/backend filtering works.
- [x] Listing and audit are deterministic.
- [x] Tags are no longer discarded.
- [x] Existing fixture outcomes are unchanged.
- [x] Full validation passes.
- [x] Capsule points to Phase 2A.

---

# Phase 2 — Require explicit canonical expectations and compile-only intent

## Context and reasoning

Canonical cases can currently omit `expect.toml` and inherit an implicit HTML success stub. Success can also rely only on the backend baseline. That hides incomplete fixtures and makes pruning unsafe.

This phase makes every success case explicit before stronger semantic consolidation begins.

## Primary owners

- `src/compiler_tests/integration_test_runner/fixture.rs`
- `src/compiler_tests/integration_test_runner/expectations.rs`
- `src/compiler_tests/integration_test_runner/types.rs`
- `src/compiler_tests/integration_test_runner/mod.rs`
- `tests/fixtures/stubs/expect.toml`
- canonical `tests/cases/*/expect.toml`

## Recommended slices

### 2A — Introduce explicit success intent

- [x] Add a typed `SuccessContract` or equivalent narrow field.
- [x] Support `success_contract = "compile_only"`.
- [x] Reject unknown values.
- [x] Reject compile-only on failure backends.
- [x] Reject compile-only mixed with artifact, golden, rendered-output or absence assertions.
- [x] Report explicit compile-only intent separately from the temporary implicit backend baseline.
- [x] Add self-tests for explicit compile-only acceptance, invalid combinations and audit classification.
- [x] Keep canonical outcomes unchanged in this schema slice; the implicit baseline and expectation fallback remain only until the 2B migration completes.

Accepted at `603cc00d3`: explicit compile-only intent is typed, validated against failure and mixed
contracts, and reported separately from the temporary backend baseline. Focused coverage increased
to 70 runner tests. The canonical audit remained 1,651 cases and 1,784 backend executions, and the
worker's full `just validate` gate passed without fixture or fallback changes.

### 2B — Migrate existing implicit success cases

The Phase 1 audit found 53 cases using the fallback expectation and 57 explicit success blocks with
no authored semantic assertion. Migrate them under the still-permissive harness so every accepted
batch remains green. Split the work by semantic family and validate every batch.

For each affected success block:

- [ ] Give every fallback-backed case its own `expect.toml`.
- [ ] Determine whether behaviour is externally visible.
- [ ] Add rendered output for behaviour-first cases.
- [ ] Add narrow artifact assertions for artifact contracts.
- [ ] Retain strict goldens only where exact output is contractual.
- [ ] Use `artifacts_must_not_exist` for absence contracts.
- [ ] Mark true acceptance-only cases compile-only and role `smoke`.
- [ ] Do not blanket-mark every weak case compile-only.
- [ ] Update the audit report to classify assertion strength.

Recommended family order:

1. harness, smoke and syntax acceptance
2. borrow, lifetime and access semantics
3. choices, functions and control flow
4. config, imports, modules and package facades
5. HTML, JavaScript, Wasm and artifact contracts
6. constants, records, templates and documentation parity

Accepted batch 2B1 at `713e44f9d`: 10 fallback-backed borrow-validation cases now own
expectations. Six no-output acceptance cases are explicit compile-only smoke cases; four
behavior-visible cases assert their current rendered markers. Audit baseline-only findings fell
from 110 to 100 with six explicit compile-only blocks and no hard violations. Full validation
passed with 3,463 Rust tests, 1,784 integration executions and 28 benchmark sanity cases.

Accepted batch 2B2 at `e9b47908f`: six fallback-backed lifetime/borrow-flow cases now own
expectations and two scoped-alias cases gained rendered-output assertions. Five no-output cases
are explicit compile-only smoke cases. Audit baseline-only findings fell from 100 to 92, fallback
use fell from 43 to 37, and full validation passed with 3,463 Rust tests, 1,784 integration
executions and 28 benchmark sanity cases.

Accepted batch 2B3 at `406763816`: four fallback-backed borrow-flow cases now assert their
observable rendered markers. Audit baseline-only findings fell from 92 to 88, fallback use fell
from 37 to 33, and full validation passed with 3,463 Rust tests, 1,784 integration executions and
28 benchmark sanity cases.

Accepted batch 2B4 at `1f0869712`: five fallback-backed result/catch cases now assert their
observable recovery values. The nested catch case correctly asserts the catch-path result
`guest-0`. Audit baseline-only findings fell from 88 to 83, fallback use fell from 33 to 28, and
full validation passed with 3,463 Rust tests and 1,784 integration executions.

Accepted batch 2B5 at `6193a2e12`: four multi-bind cases now assert observable results and one
no-output optional multi-bind case is an explicit compile-only smoke test. Audit baseline-only
findings fell from 83 to 78, fallback use fell from 28 to 23, and full validation passed.

Accepted batch 2B6 at `195253f61`: three integer-to-float promotion cases now assert their
observable markers and one mixed-literal case is an explicit compile-only smoke test. Audit
baseline-only findings fell from 78 to 74, fallback use fell from 23 to 19, and full validation
passed with 3,463 Rust tests, 1,784 integration executions and 28 benchmark sanity cases.

Accepted batch 2B7 at `315e16c5d`: five function and struct cases now assert their rendered
outputs, while `struct_using_constant` owns a narrow static `index.html` artifact assertion.
Audit baseline-only findings fell from 74 to 68, fallback use fell from 19 to 13, and full
validation passed with 3,463 Rust tests and 1,784 integration executions.

Accepted batch 2B8 at `00435b923`: four no-output choice and syntax cases are explicit
compile-only smoke tests, while three optional-`none` cases assert their observable `ok` marker.
Audit baseline-only findings fell from 68 to 61, fallback use fell from 13 to six, and full
validation passed.

Accepted batch 2B9 at `612439169`: the package-selection case now asserts its package-A runtime
lowering and excludes package B, while three runtime cases assert their authored console values.
Audit baseline-only findings fell from 61 to 57, fallback use fell from six to two, and the worker's
full validation plus the parent's focused cases, 70 runner tests, audit and diff checks passed.

Accepted batch 2B10a at `79d821a2b`: the stale nested helper import now uses the accepted
module-root-relative `@helper/prefix` path and asserts its `A-hello-B` output. Audit baseline-only
findings fell from 57 to 56, fallback use fell from two to one, and the worker's full validation
plus the parent's exact case, 70 runner tests, audit and diff checks passed.

Accepted batch 2B10b at `9f40480a9`: block-aware future-use facts now keep source-semantic
loop-carried aliases active without changing HIR, mutation of the active iterable reports
`BST-BORROW-0003`, and focused tests preserve unrelated and copied roots. Audit baseline-only
findings fell from 56 to 55, fallback use reached zero, and the worker's full validation plus the
parent's focused tests, exact cases, audit, formatting and diff checks passed.

Phase 2B11 exploration accounted for the remaining 55 explicit baseline-only blocks in nine
bounded batches. Its detailed classification, corrected against the full list, is 38 rendered-output
contracts, one artifact contract, two warning contracts, 13 intentional compile-only candidates and
one isolated design-conflict investigation. `choice_import_visibility_non_exported` must be reviewed
against private cross-module import rejection before any expectation migration. No files changed.

Accepted batch 2B11a at `f3e10466a`: seven choice payload-match cases now assert their executed
branch results, including six `bad` payloads and the `forty-two` guard result. Audit baseline-only
findings fell from 55 to 48, and the worker's full validation plus the parent's exact cases, focused
tests, audit, formatting and diff checks passed.

Accepted batch 2B11b at `71ca50815`: six choice constructor, constant and imported-constructor
cases now declare intentional compile-only acceptance, while imported alias payload matching asserts
its executed `captured` value. Audit compile-only findings rose from 17 to 23, baseline-only findings
fell from 48 to 41, and the worker's full validation plus the parent's exact cases, focused tests,
audit, formatting and diff checks passed.

Accepted corrected batch 2B11c at `ba3366218`: five static or folded page fixtures now assert their
`index.html` artifacts, while the receiver-method fixture asserts executed `3, 4` output. The first
attempt cleanly exposed and reverted a dynamic/static assertion-lane mismatch. Audit baseline-only
findings fell from 41 to 35, and the worker's full validation plus the parent's exact cases, focused
tests, audit, formatting and diff checks passed.

### 2C — Enforce canonical expectations and remove the fallback

- [x] Make every manifest-listed case require its own `expect.toml`.
- [x] Return a clear harness error for a missing expectation file.
- [x] Keep direct test helpers explicit by writing their own expectation.
- [ ] Make implicit backend baseline checks insufficient for success-fixture completeness.
- [x] Remove the default-stub read path from canonical fixture loading.
- [x] Remove `DEFAULT_EXPECT_STUB_PATH` if no current owner remains.
- [x] Delete `tests/fixtures/stubs/expect.toml`.
- [x] Delete self-tests that bless missing expectation files.
- [ ] Add self-tests proving implicit baseline-only success fails after the remaining 55 blocks migrate.

Accepted Phase 2C1 at `e4d70489b`: canonical loading now requires a case-owned `expect.toml`,
missing contracts identify their case and paths, and the fallback parser path, constant and stub
expectation are deleted without a compatibility route. The worker's full gate and the parent's 70
focused tests, audit, formatting and diff checks passed. Phase 2 remains active because 55 explicit
success blocks still rely only on the backend baseline and must be classified before that final
implicit contract is rejected.

## Phase 2 audit

- [ ] No canonical case uses an expectation stub.
- [ ] Every manifest case has `expect.toml`.
- [ ] Every success backend has an explicit authored contract.
- [ ] Compile-only cases are intentional and classified.
- [ ] No behaviour-visible case was weakened to compile-only.
- [ ] No legacy fallback constant/path remains.

## Style-guide review

- [ ] Fixture validation has one explicit path.
- [ ] Error messages name the case and missing contract.
- [ ] No compatibility wrapper preserves the default stub.
- [ ] Schema enums are narrow and descriptive.

## Validation

Iteration:

```bash
cargo test --quiet integration_test_runner -- --format terse
cargo run --quiet -- tests --audit
cargo run --quiet -- tests --case <migrated-case> --backend html
```

Final:

```bash
cargo fmt
git diff --check
just validate
```

## Documentation impact

- [ ] Update `testing.bd` with explicit expectation and compile-only rules.
- [ ] Update canonical fixture examples.
- [ ] Update `CONTRIBUTING.md` only if contributor workflow changes.
- [ ] Review progress-matrix coverage wording for cases strengthened from implicit compile-only to semantic assertions.

## Phase 2 acceptance

- [ ] Missing expectations fail.
- [ ] Implicit compile-only success is impossible.
- [ ] Default stub infrastructure is deleted.
- [ ] Audit reports no implicit success cases.
- [ ] Full validation passes.

---

# Phase 3 — Split assertion ownership and enforce exact diagnostic/warning multiplicity

## Context and reasoning

`assertions.rs` already owns several distinct concerns and will grow during hardening. Failure validation currently checks only presence of expected codes. Warning expectations are count-oriented.

This phase first restores narrow module ownership, then makes code multiplicity exact.

## Primary owners

- `src/compiler_tests/integration_test_runner/assertions.rs`
- `src/compiler_tests/integration_test_runner/types.rs`
- `src/compiler_tests/integration_test_runner/expectations.rs`
- harness self-tests
- canonical failure and warning expectations

## Recommended slices

### 3A — Split assertion modules without behaviour change

Target ownership:

```text
assertions/
├── mod.rs
├── diagnostics.rs
├── warnings.rs
├── artifacts.rs
├── goldens.rs
├── rendered_output.rs
└── wasm.rs
```

Adjust exact boundaries to avoid circular ownership.

- [ ] Keep orchestration entries in `assertions/mod.rs`.
- [ ] Move existing code, do not copy it.
- [ ] Delete the old monolithic file.
- [ ] Keep helper visibility narrow.
- [ ] Keep golden normalization with goldens.
- [ ] Keep Node execution with rendered output.
- [ ] Keep Wasm parse/validation with Wasm assertions.

### 3B — Exact diagnostic code multiset

- [ ] Add typed `DiagnosticMatchMode::{Exact, Contains}`.
- [ ] Make exact the final default.
- [ ] Compare code multisets including duplicate counts.
- [ ] Ignore ordering by default.
- [ ] Report missing, unexpected, and count-mismatched codes clearly.
- [ ] Require `diagnostic_contains_reason` for contains mode.
- [ ] Reject a contains reason in exact mode.
- [ ] Preserve `message_contains` as an optional additional rendering contract.
- [ ] Add self-tests for exact success, unexpected extra, duplicate count, and justified contains.

### 3C — Warning identity

- [ ] Add exact warning code assertions.
- [ ] Preserve `warnings = "forbid"` and `warnings = "ignore"`.
- [ ] Replace count-only exact warning mode with code-aware exact mode or require code list plus count consistency.
- [ ] Report missing/unexpected warning codes.
- [ ] Add self-tests for warnings in successful and failed compilation results.

### 3D — Migrate canonical fixtures

- [ ] Inventory actual diagnostic multisets.
- [ ] Migrate one semantic family per slice.
- [ ] Add explicit contains mode only to intentional cascade/recovery cases.
- [ ] Author a specific contains reason for every retained weaker match.
- [ ] Migrate expected-warning fixtures to code identity.
- [ ] Delete obsolete count-only schema fields after the final migration.
- [ ] Update audit findings so exactness is a hard policy.

Recommended family order:

1. borrow and access
2. syntax/tokenization
3. type annotations, collections, maps
4. functions, results/options, calls
5. generics and traits
6. imports, modules, config
7. backend target validation
8. templates/TIR after final TIR closure

## Phase 3 audit

- [ ] Exact mode rejects every unexpected error.
- [ ] Duplicate diagnostics are represented explicitly.
- [ ] Contains mode is rare, justified, and reported.
- [ ] Warning identity replaces count-only assumptions.
- [ ] Assertion modules have one responsibility each.
- [ ] No old assertion path or warning-count compatibility path remains at phase acceptance.

## Style-guide review

- [ ] Errors use descriptive data structures.
- [ ] No dense iterator pipeline hides matching logic.
- [ ] Failure reports are readable.
- [ ] New modules have concise docs.
- [ ] No compiler diagnostic semantics moved into test infrastructure.

## Validation

Iteration:

```bash
cargo test --quiet integration_test_runner::tests -- --format terse
cargo run --quiet -- tests --audit
cargo run --quiet -- tests --tag diagnostics --backend html
```

Final:

```bash
cargo fmt
git diff --check
just validate
```

## Documentation impact

- [ ] Update `testing.bd` with exact multiset and warning-code policy.
- [ ] Update expectation examples.
- [ ] Review progress-matrix structured-diagnostics coverage wording.
- [ ] Update `index.md` for assertion module path changes.

## Phase 3 acceptance

- [ ] Exact diagnostic/warning multiplicity is the normal contract.
- [ ] No unexpected diagnostic can silently pass.
- [ ] Audit reports zero unjustified contains-mode cases.
- [ ] Full validation passes.

---

# Phase 4 — Add compiler-owned diagnostic reason and source-location assertions

## Context and reasoning

Stable codes may intentionally group several structured reasons. The current suite can therefore pass when the right broad code is emitted for the wrong semantic reason. Imported/generated boundaries also need source-remapping coverage.

The runner must consume structured identity from the compiler rather than parsing prose or duplicating payload matches.

## Primary owners

- `src/compiler_frontend/compiler_messages/diagnostic_payload/`
- `src/compiler_frontend/compiler_messages/compiler_diagnostic.rs`
- `src/compiler_tests/integration_test_runner/types.rs`
- `src/compiler_tests/integration_test_runner/expectations.rs`
- `src/compiler_tests/integration_test_runner/assertions/diagnostics.rs`

## Recommended slices

### 4A — Define compiler-owned reason identity

- [ ] Inspect existing diagnostic kind/descriptor ownership.
- [ ] Add one crate-private structured identity path owned by compiler messages.
- [ ] Return code, severity, and optional stable reason key from a diagnostic.
- [ ] Use fully qualified reason keys such as `invalid_collection_type.zero_capacity`.
- [ ] Keep reason keys independent of rendered wording.
- [ ] For payloads without nested reasons, use a stable payload key only where needed.
- [ ] Keep mapping beside typed payload/reason definitions.
- [ ] Do not implement runner-side matches over `DiagnosticPayload`.
- [ ] Add focused compiler-message units for representative keys and uniqueness.
- [ ] Document stability expectations.

### 4B — Add structured expectation tables

- [ ] Add `diagnostic_assertions`.
- [ ] Match by code plus deterministic occurrence.
- [ ] Support optional reason.
- [ ] Support optional normalized source path.
- [ ] Support optional line.
- [ ] Support optional column only for span-contract cases.
- [ ] Support optional expected count.
- [ ] Support narrow secondary-label assertions where declaration/conflict location is part of the contract.
- [ ] Reject assertions whose code is absent from `diagnostic_codes`.
- [ ] Reject ambiguous assertions.
- [ ] Produce actionable mismatch output without rendering full snapshots.

### 4C — Migrate high-risk broad-code clusters

Prioritize:

- [ ] fixed collection zero/negative/invalid capacity reasons
- [ ] invalid call-shape reasons
- [ ] mutable/immutable/temporary access reasons
- [ ] trait-name misuse across type surfaces
- [ ] import visibility/path reasons
- [ ] result/fallible handling reasons
- [ ] backend unsupported-feature reasons
- [ ] diagnostic remapping and secondary labels
- [ ] source locations across imported files

Do not require reason/location on every fixture. Require it where the code alone cannot distinguish the intended contract or where source attribution is important.

## Phase 4 audit

- [ ] Reason identity has one compiler owner.
- [ ] Runner contains no diagnostic payload taxonomy.
- [ ] Reason keys do not depend on `Debug` output or rendered text.
- [ ] Paths are normalized through existing source identity/string-table data.
- [ ] Broad-code clusters have reason assertions.
- [ ] Location assertions avoid brittle columns unless contractual.
- [ ] Message fragments remain rare and purposeful.

## Style-guide review

- [ ] Structured facts remain in diagnostics.
- [ ] Source locations and labels are preserved.
- [ ] No generic map of stringly typed payload fields was added.
- [ ] New identity types are narrow and descriptively named.
- [ ] No public API was broadened unnecessarily.

## Validation

Iteration:

```bash
cargo test --quiet compiler_messages -- --format terse
cargo test --quiet integration_test_runner -- --format terse
cargo run --quiet -- tests --tag diagnostics --backend html
cargo run --quiet -- tests --audit
```

Final:

```bash
cargo fmt
git diff --check
just validate
```

## Documentation impact

- [ ] Update `testing.bd` with structured diagnostic assertions.
- [ ] Update `docs/compiler-design-overview.md` only if reason keys become a durable machine-readable diagnostic identity beyond test infrastructure.
- [ ] Update diagnostic educational docs only when they explicitly describe the changed identity contract.
- [ ] Review progress-matrix diagnostics coverage.

## Phase 4 acceptance

- [ ] Integration cases can distinguish broad-code reasons.
- [ ] Source path/line contracts are assertable.
- [ ] Compiler messages own identity.
- [ ] High-risk clusters are migrated.
- [ ] Full validation passes.

---

# Phase 5 — Preserve ordered runtime events and strengthen artifact assertions

## Context and reasoning

Runtime behaviour currently uses contains/not-contains checks over a combined result. This cannot reliably protect execution order, exact-once activation, repeated updates, or mixed console/fragment chronology.

This phase strengthens the existing harness rather than introducing another executor.

## Primary owners

- `src/compiler_tests/integration_test_runner/assertions/rendered_output.rs`
- `src/compiler_tests/integration_test_runner/types.rs`
- `src/compiler_tests/integration_test_runner/expectations.rs`
- existing Node harness construction/extraction
- order-sensitive integration cases

## Recommended slices

### 5A — Introduce an ordered rendered-event model

- [ ] Define typed rendered events for console and fragment insertion.
- [ ] Modify the existing Node harness to append events when they occur.
- [ ] Serialize one ordered event array.
- [ ] Keep any useful channel-specific views as derived data.
- [ ] Preserve one microtask/event-loop flush policy and document it.
- [ ] Preserve temporary-file cleanup and retry behaviour.
- [ ] Add units for script extraction, event decoding, ordering, and harness errors.
- [ ] Do not invoke Node for cases without runtime assertions.

### 5B — Add stronger runtime expectation fields

- [ ] Add exact output.
- [ ] Add ordered fragments.
- [ ] Add exact-once fragments.
- [ ] Retain contains/not-contains.
- [ ] Validate incompatible/empty combinations.
- [ ] Normalize line endings.
- [ ] Do not silently collapse arbitrary whitespace in exact mode.
- [ ] Classify exact/ordered mismatches distinctly in failure triage.

### 5C — Strengthen existing order-sensitive cases

At minimum:

- [ ] active root executes exactly once
- [ ] imported root runtime marker never executes
- [ ] compile-time and runtime fragments appear in source-defined order
- [ ] loop iteration order is exact
- [ ] output before `break`/`continue` is preserved
- [ ] map insertion/replacement/removal order is exact
- [ ] reactive mount marker appears exactly once
- [ ] runtime helper output is not duplicated

### 5D — Audit goldens

- [ ] Inventory every strict golden.
- [ ] Mark the deliberate exact artifact contract.
- [ ] Convert behaviour-only goldens to runtime assertions.
- [ ] Convert structural contracts to narrow artifact assertions.
- [ ] Keep normalized goldens only where normalized generated structure is genuinely the contract.
- [ ] Delete unused golden files and directories.
- [ ] Record every conversion in the pruning ledger.

## Phase 5 audit

- [ ] One Node harness remains.
- [ ] Runtime chronology is not reconstructed after execution.
- [ ] Exact-once checks protect activation/mount contracts.
- [ ] Exact mode is not over-normalized.
- [ ] Strict goldens are deliberate.
- [ ] Behaviour tests do not pin generated identifier numbering or CSS boilerplate.

## Style-guide review

- [ ] Rendered-output execution, decoding, and matching have separate narrow helpers.
- [ ] Temporary-file failures remain infrastructure failures.
- [ ] Semantic mismatches remain test expectation failures.
- [ ] No broad abstraction hides the event flow.

## Validation

Iteration:

```bash
cargo test --quiet integration_test_runner::tests -- --format terse
cargo run --quiet -- tests --case <order-case> --backend html
cargo run --quiet -- tests --tag reactivity --backend html
cargo run --quiet -- tests --audit
```

Final:

```bash
cargo fmt
git diff --check
just validate
```

## Documentation impact

- [ ] Update `testing.bd` with exact/ordered/exact-once runtime policy.
- [ ] Update canonical expectation examples.
- [ ] Review progress-matrix coverage wording for order-sensitive surfaces.

## Phase 5 acceptance

- [ ] Runtime event order is available.
- [ ] High-risk order cases use strong assertions.
- [ ] Strict goldens have explicit ownership.
- [ ] Full validation passes.

---

# Phase 6 — Add primary integration coverage for hashmap access and ownership semantics

## Context and reasoning

The language and memory contracts state that map `get` returns shared access into the map, mutation is forbidden while that access is live, `remove` returns an owned value, and key/value storage follows move/copy rules. Several direct tests currently live as source-shaped borrow-checker units.

Add the integration owners before deleting any unit.

## Required semantic reading

- map language reference
- access-and-aliasing memory reference
- borrow-validation memory reference
- borrow checker implementation and summaries
- current progress matrix

## Primary owners

- new or strengthened `tests/cases/` map/borrow cases
- `tests/cases/manifest.toml`
- existing focused borrow fact tests remain secondary

## Checklist

### Live lookup alias conflicts

- [ ] Add a primary failure case for `get` result live across `set`.
- [ ] Add a distinct failure case for live `get` across `remove` if the transfer path or diagnostic reason is distinct.
- [ ] Add a distinct failure case for live `get` across `clear` if the transfer path or diagnostic reason is distinct.
- [ ] Assert exact code, reason, primary mutation location, and earlier shared-access label.
- [ ] Use one stable contract family with clear boundary roles where appropriate.

### Alias lifetime ends

- [ ] Add success coverage where the lookup result's final use precedes mutation.
- [ ] Assert runtime output after mutation.
- [ ] Add branch/loop form only when it protects a distinct control-flow join.

### Copy independence

- [ ] Add success coverage where `copy` of the retrieved value remains usable after mutation.
- [ ] Assert the copied value and final map state.
- [ ] Keep copy semantics distinct from alias-last-use semantics.

### Removed value ownership

- [ ] Add success coverage where `remove` returns a value that remains usable after later map mutation or clear.
- [ ] Assert exact runtime result.

### Key and insertion semantics

- [ ] Add or strengthen coverage proving lookup keys are borrowed, not consumed.
- [ ] Add or strengthen coverage proving inserted non-copy keys/values follow consumption rules.
- [ ] Add explicit-copy success coverage where independence is required.
- [ ] Reuse current runtime map case for insertion ordering rather than duplicating it.

### Backend contract

- [ ] Use one input with HTML success and HTML-Wasm structured rejection where map reachability is relevant.
- [ ] Do not require Wasm success.

## Phase 6 audit

- [ ] Every map access/ownership contract has a primary integration owner.
- [ ] Negative cases fail for the intended reason, not merely a broad code.
- [ ] Positive cases observe runtime values.
- [ ] No unit has been deleted yet.
- [ ] No deferred map feature was introduced.

## Style-guide review

- [ ] Cases are readable real programs.
- [ ] Output uses templates unless console behaviour is under test.
- [ ] Each failure case isolates one reason.
- [ ] Cross-backend expectations share one input.

## Validation

Iteration:

```bash
cargo run --quiet -- tests --tag maps --backend html
cargo run --quiet -- tests --tag maps --backend html_wasm
cargo test --quiet borrow_checker_map -- --format terse
cargo run --quiet -- tests --audit
```

Final:

```bash
git diff --check
just validate
```

## Documentation impact

- [ ] Update progress-matrix hash map and borrow coverage summaries if the new integration ownership is materially stronger.
- [ ] Do not edit language or memory semantics unless a real contradiction is found.

## Phase 6 acceptance

- [ ] Map semantic gaps are covered end to end.
- [ ] Exact diagnostics and runtime output pass.
- [ ] Full validation passes.
- [ ] Capsule identifies the exact units eligible for Phase 7.

---

# Phase 7 — Prune and narrow borrow-checker source-shaped units

## Context and reasoning

With Phase 6 integration owners in place, remove source-level units that only re-prove language acceptance/rejection. Retain hidden fact, transfer, summary, malformed-HIR, drop-site, and invalidation tests.

## Primary owners

- `src/compiler_frontend/analysis/borrow_checker/tests/borrow_checker_map_tests.rs`
- `src/compiler_frontend/analysis/borrow_checker/tests/borrow_checker_scope_tests.rs`
- `src/compiler_frontend/analysis/borrow_checker/tests/borrow_checker_pipeline_tests.rs`
- call-summary, fact, drop-site, reactivity, state, and engine tests
- shared test support under `src/compiler_frontend/tests/`

## Checklist

### Map units

For each test in `borrow_checker_map_tests.rs`:

- [ ] Identify whether it inspects a hidden fact.
- [ ] Map source-only acceptance/rejection to a Phase 6 contract.
- [ ] Delete tests fully replaced by integration.
- [ ] Retain one narrow transfer/fact unit only where integration cannot expose the root/access classification.
- [ ] Delete helpers used only by removed tests.
- [ ] Split or delete the file if its remaining responsibility no longer justifies a separate module.

### Scope units

- [ ] Strengthen the existing integration case for branch-local alias expiry with runtime output.
- [ ] Add match/loop integration only if distinct control-flow owners require it.
- [ ] Delete ordinary source-equivalent branch/match/loop scope units once replaced.
- [ ] Retain synthetic dead-local or malformed-HIR tests.
- [ ] Retain precise merge-state units when they inspect hidden snapshots.

### Pipeline sentinels

- [ ] Keep one minimal failure-propagation sentinel.
- [ ] Keep one success/report-storage sentinel only if it protects a distinct orchestration boundary.
- [ ] Delete repeated semantic scenarios from the pipeline level.
- [ ] Name remaining tests after the stage boundary.

### Fact and summary suites

- [ ] Preserve statement/terminator/value facts.
- [ ] Preserve return-alias/call-summary tests.
- [ ] Preserve drop-site and liveness facts.
- [ ] Preserve reactive invalidation facts.
- [ ] Preserve malformed-HIR rejection.
- [ ] Remove only obsolete API-shape assertions revealed by final TIR/diagnostics work.

### Ledger

- [ ] Enter every removed test and replacement contract.
- [ ] Record retained secondary units and their hidden invariant.

## Phase 7 audit

- [ ] No user-visible borrow behaviour is unit-only.
- [ ] Hidden facts remain covered.
- [ ] Pipeline coverage is minimal.
- [ ] Removed helpers and stale comments are gone.
- [ ] No test depends on incidental HIR numbering.
- [ ] The borrow checker still has strong internal coverage.

## Style-guide review

- [ ] Test names state invariants.
- [ ] Fixture support remains local.
- [ ] No production API was preserved for deleted tests.
- [ ] No broad helper hides source shape.

## Validation

Iteration:

```bash
cargo test --quiet borrow_checker -- --format terse
cargo run --quiet -- tests --tag borrows --backend html
cargo run --quiet -- tests --tag maps --backend html
```

Final:

```bash
cargo fmt
git diff --check
just validate
```

## Documentation impact

- [ ] Update progress-matrix borrow/map coverage summaries when ownership has materially moved.
- [ ] Update `index.md` if test modules are removed or renamed.
- [ ] No language/memory design update is expected.

## Phase 7 acceptance

- [ ] Source-shaped borrow duplicates are removed.
- [ ] Hidden borrow invariants remain focused and strong.
- [ ] Pruning ledger is complete for the phase.
- [ ] Full validation passes.

---

# Phase 8 — Reassign function-call, type-resolution, and generic test ownership

## Context and reasoning

These suites contain both valuable internal algorithm tests and full-source user-behaviour duplicates. Preserve parser shape, locations, canonical identity, unification, and substitution. Move or delete whole-program acceptance/rejection once integration ownership is strong.

## Primary owners

- `src/compiler_frontend/ast/expressions/tests/function_call_tests.rs`
- `src/compiler_frontend/ast/tests/type_resolution_tests.rs`
- `src/compiler_frontend/datatypes/tests/generics_tests.rs`
- related syntax/statement tests
- existing and new `tests/cases/` owners

## Recommended slices

### 8A — Function-call ownership

Keep units for:

- [ ] positional/named parsed shape
- [ ] target/value/marker source locations
- [ ] access-mode classification
- [ ] narrow parser error conversion
- [ ] hidden parameter binding/order algorithms

Ensure integration ownership for:

- [ ] positional-after-named
- [ ] duplicate named target
- [ ] unknown named target
- [ ] missing required argument
- [ ] existing mutable place missing `~`
- [ ] `~` on immutable place
- [ ] `~` on non-place
- [ ] fresh template/collection/struct/computed mutable arguments
- [ ] named mutable success

Then:

- [ ] delete whole-source units that only compile or fail
- [ ] retain a narrow structured-reason unit only where it tests the owning validator directly
- [ ] record replacements

### 8B — Type-resolution ownership

Keep units for:

- [ ] canonical `TypeId`
- [ ] fixed versus growable collection identity
- [ ] nested collection/map shapes
- [ ] alias transparency
- [ ] imported canonical type projection
- [ ] field/variant registration
- [ ] invalid internal parsed-type handling

Ensure integration ownership for:

- [ ] zero/negative/floating/non-constant capacities
- [ ] invalid capacity surfaces
- [ ] signature/field/alias/return diagnostics
- [ ] cross-file capacity resolution
- [ ] current backend rejection where relevant

Then:

- [ ] delete source-level diagnostic units replaced by exact integration contracts
- [ ] keep direct resolver units that prove invalid syntax is not erased into another semantic type

### 8C — Generic ownership

Keep units for:

- [ ] canonical generic parameter identity
- [ ] binding consistency/conflict
- [ ] argument order
- [ ] type identity keys
- [ ] substitution/unification
- [ ] generated instance identity
- [ ] impossible/incomplete binding states

Ensure integration ownership for:

- [ ] duplicate parameter
- [ ] invalid naming style
- [ ] visible type collision
- [ ] unused parameter
- [ ] inference ambiguity/conflict
- [ ] cross-file/facade visibility
- [ ] trait-bound evidence and privacy

Then:

- [ ] remove rendered-string diagnostic units
- [ ] assert structured payload directly only for narrow compiler-message or validator algorithms
- [ ] record replacements

## Phase 8 audit

- [ ] Parser units stop at parser facts.
- [ ] Type units stop at semantic identity.
- [ ] Generic units stop at identity/binding/substitution facts.
- [ ] User behaviour is integration-owned.
- [ ] No renderer wording is pinned in datatype units.
- [ ] No test-only production API remains.

## Style-guide review

- [ ] Test helpers remain with their owner.
- [ ] No giant table obscures distinct invariants.
- [ ] Similar logic is consolidated only when ownership is genuinely identical.
- [ ] Test names use source-visible terms for user-facing cases and compiler terms only for internal invariants.

## Validation

Iteration:

```bash
cargo test --quiet function_call_tests -- --format terse
cargo test --quiet type_resolution_tests -- --format terse
cargo test --quiet generics_tests -- --format terse
cargo run --quiet -- tests --tag functions --backend html
cargo run --quiet -- tests --tag generics --backend html
```

Final:

```bash
cargo fmt
git diff --check
just validate
```

## Documentation impact

- [ ] Update progress-matrix coverage summaries for functions, types, generics, and named arguments if materially changed.
- [ ] Update `index.md` for moved/removed modules.
- [ ] No language semantics update is expected.

## Phase 8 acceptance

- [ ] Whole-program duplicates are removed.
- [ ] Internal algorithms remain strongly covered.
- [ ] Integration cases carry exact output/diagnostics.
- [ ] Full validation passes.

---

# Phase 9 — Add cross-module access/effect-summary and diagnostic-remapping integration coverage

## Context and reasoning

The compiler architecture makes parameter access, mutation, possible consumption, return aliasing, and reactive effects part of public semantic interfaces. Unit summary tests alone do not prove that module boundaries, imports, facades, and generated functions consume those summaries correctly.

This phase adds multi-file primary owners while retaining focused summary units.

## Required semantic reading

- compiler public semantic interfaces
- cross-module call targets and borrow validation
- module/facade visibility
- access-and-aliasing memory docs
- current progress matrix

## Checklist

### Exported return aliases

- [ ] Add a provider module with an exported function returning an alias of a parameter.
- [ ] Import and call it from a consumer.
- [ ] Reject caller mutation while the returned alias is live.
- [ ] Assert consumer mutation location and provider/call context where available.
- [ ] Add success after final alias use.

### Fresh returns

- [ ] Add a provider returning a fresh value from a shared parameter.
- [ ] Prove caller mutation is accepted.
- [ ] Assert runtime output.

### Mutable parameters

- [ ] Add cross-module mutable-parameter success using explicit `~`.
- [ ] Add missing-`~` rejection at the consumer call site.
- [ ] Assert reason and source location.

### Facade/re-export behaviour

- [ ] Route the same exported function through a module facade/re-export.
- [ ] Prove access/effect semantics remain identical.
- [ ] Assert public alias spelling does not alter origin semantics.

### Generated generic instances

- [ ] Add a cross-module generic function instance where current support permits.
- [ ] Prove concrete generated function access/alias semantics.
- [ ] Keep unit tests for request/summary identity.

### Diagnostic remapping

- [ ] Add an imported-provider diagnostic path case.
- [ ] Add a consumer call-site diagnostic case.
- [ ] Add a re-export/facade path case.
- [ ] Add a generated generic call-site case.
- [ ] Add a borrow conflict with meaningful primary and secondary locations across files where supported.
- [ ] Assert normalized paths and lines, not full snapshots.

## Discovery policy

When a case fails because current implementation does not yet support the accepted end-state module architecture:

- [ ] Check the progress matrix.
- [ ] Do not implement canonical module architecture under this phase.
- [ ] Assert current structured rejection when that is the current contract.
- [ ] Move future success coverage to the canonical module plan.
- [ ] Record the handoff in this plan and the queued plan.

## Phase 9 audit

- [ ] Public summaries are proven at the boundary.
- [ ] Unit summary facts remain.
- [ ] No provider HIR is copied into consumer tests.
- [ ] Source remapping is structured.
- [ ] Facades do not bypass visibility.
- [ ] Future module features were not pulled forward.

## Style-guide review

- [ ] Multi-file fixtures stay in one case directory.
- [ ] Cases name semantic behaviour rather than implementation types.
- [ ] Source paths are clear.
- [ ] Diagnostics use current lanes and codes.

## Validation

Iteration:

```bash
cargo run --quiet -- tests --tag imports --tag borrows --backend html
cargo run --quiet -- tests --tag generics --tag imports --backend html
cargo test --quiet borrow_checker_call_summary -- --format terse
```

Final:

```bash
git diff --check
just validate
```

## Documentation impact

- [ ] Update progress-matrix borrow/import/generic coverage summaries.
- [ ] Update compiler/build/memory design only for a confirmed contradiction, not merely missing end-state implementation.
- [ ] Record deferred handoffs to canonical module plan.

## Phase 9 acceptance

- [ ] Current cross-module effect behaviour has primary integration owners.
- [ ] Remapped locations are asserted where available.
- [ ] Deferred architecture remains deferred.
- [ ] Full validation passes.

---

# Phase 10 — Strengthen `check`/`build` parity and root/fragment activation coverage

## Context and reasoning

`check` is a frontend/tooling overlay that must preserve frontend diagnostics while avoiding backend lowering and output writing. Active-root behaviour must execute exactly once, imported roots must remain dormant, and page fragments must preserve source order.

These are project/command contracts, not ordinary AST unit contracts.

## Primary owners

- `src/projects/check.rs`
- `src/projects/check/tests/`
- build-system/project command test owners
- root/module integration cases
- integration runner ordered runtime assertions

## Recommended slices

### 10A — `check`/`build` frontend parity

- [ ] Build one reusable test fixture through `check` and ordinary build frontend paths.
- [ ] Compare exact frontend diagnostic code multiset.
- [ ] Compare reason keys.
- [ ] Compare normalized source paths and lines.
- [ ] Prove `check` writes no backend artifacts.
- [ ] Prove warning collection matches frontend build behaviour.
- [ ] Keep command formatting/summary tests separate from semantic parity.
- [ ] Avoid invoking the public CLI subprocess when a direct command owner provides a stable test seam.

Target-validation parity:

- [ ] Inspect current implementation against build-system design.
- [ ] Add parity only for currently implemented target planning.
- [ ] Otherwise record a canonical-module/mixed-backend handoff rather than implementing it here.

### 10B — Active and imported root execution

- [ ] Strengthen active-root exact-once output.
- [ ] Strengthen imported-root suppression.
- [ ] Assert imported public APIs remain usable.
- [ ] Assert API-only roots emit no route/artifact where current support permits.
- [ ] Use exact-once runtime markers.

### 10C — Fragment order

- [ ] Assert compile-time/runtime fragment merge order.
- [ ] Assert runtime fragments preserve source order.
- [ ] Assert output before loop control is preserved.
- [ ] Assert no duplicate hydration/mount.
- [ ] Use ordered events, not generated JS text, as primary semantics.

## Phase 10 audit

- [ ] `check` parity uses shared frontend contracts.
- [ ] `check` does not become a second compiler path.
- [ ] No artifact is written.
- [ ] Root activation is exactly once.
- [ ] Imported roots stay dormant.
- [ ] Fragment order is externally observed.
- [ ] Build-system policy units remain for hidden graph/output state.

## Style-guide review

- [ ] Command tests use narrow helpers.
- [ ] Filesystem fixtures use owned temporary directories.
- [ ] No current-directory mutation leaks across tests.
- [ ] Source and build-system responsibilities remain separate.

## Validation

Iteration:

```bash
cargo test --quiet check_tests -- --format terse
cargo test --quiet build_orchestration -- --format terse
cargo run --quiet -- tests --tag integration --tag imports --backend html
```

Final:

```bash
cargo fmt
git diff --check
just validate
```

## Documentation impact

- [ ] Update progress-matrix active-root, build, and check coverage statements.
- [ ] Update `CONTRIBUTING.md` only if command test workflow changes.
- [ ] Update build-system design only for a confirmed accepted-contract correction.

## Phase 10 acceptance

- [ ] Current `check`/build frontend parity is protected.
- [ ] Root/fragment activation contracts use exact/ordered assertions.
- [ ] Full validation passes.

---

# Phase 11 — Add runtime integration ownership for reactivity after subscription

## Context and reasoning

Hidden reactive invalidation facts are appropriately unit-tested. Integration must prove that those facts produce current HTML-JS behaviour and that a subscription is not an active mutable borrow.

This phase begins only after final TIR completion.

## Required semantic reading

- language reactivity surface
- memory reactive invalidation/liveness
- compiler reactivity boundary
- HTML-JS current progress status
- final neutral owned runtime-template handoff at the AST-to-HIR boundary

## Checklist

### Subscription does not hold an active borrow

- [ ] Mount a template subscribed to a reactive source.
- [ ] Mutate the source after subscription.
- [ ] Prove the mutation is accepted.
- [ ] Prove the mount updates.

### Exact rerender behaviour

- [ ] Assert initial output.
- [ ] Assert update output in order.
- [ ] Assert one mount.
- [ ] Assert expected rerender count.
- [ ] Add repeated mutations only when current runtime semantics define the batching contract.
- [ ] Do not make an incidental microtask count contractual.

### Reactive parameters

- [ ] Add or strengthen rejection proving a reactive parameter does not grant mutation permission.
- [ ] Assert exact reason and location.
- [ ] Keep invalidation-fact units for parameter/source identity.

### Map/place invalidation

- [ ] Add runtime coverage for currently supported map/place mutation invalidation only where current progress says it works.
- [ ] Keep hidden `ReactiveInvalidationKind` units.
- [ ] Do not implement field/path subscriptions or other deferred reactivity.

### Backend matrix

- [ ] HTML-JS success.
- [ ] HTML-Wasm structured rejection where runtime reactivity is reachable.
- [ ] One input, backend-specific blocks.

## Phase 11 audit

- [ ] Runtime semantics and hidden facts each have one owner.
- [ ] Subscription is not modeled as a borrow in tests.
- [ ] No deferred reactive surface was introduced.
- [ ] Ordered assertions do not pin incidental scheduler internals.
- [ ] No TIR identity, view, overlay or preparation state leaks into completed AST/HIR/backend expectations; assert only neutral owned runtime-template semantics at that boundary.

## Style-guide review

- [ ] Integration case is readable.
- [ ] Reactive facts remain in analysis units.
- [ ] Runtime harness changes remain with rendered-output owner.
- [ ] Target rejection is structured.

## Validation

Iteration:

```bash
cargo test --quiet borrow_checker_reactivity -- --format terse
cargo run --quiet -- tests --tag reactivity --backend html
cargo run --quiet -- tests --tag reactivity --backend html_wasm
```

Final:

```bash
git diff --check
just validate
```

## Documentation impact

- [ ] Update progress-matrix reactivity coverage statement.
- [ ] Do not broaden language/memory docs unless current accepted behaviour is corrected.
- [ ] Record deferred HTML-Wasm and fine-grained reactivity work against existing roadmap owners.

## Phase 11 acceptance

- [ ] Post-subscription mutation/update has an integration owner.
- [ ] Reactive parameter permission is covered.
- [ ] Hidden invalidation units remain.
- [ ] Full validation passes.

---

# Phase 12 — Consolidate redundant positive integration scenarios

## Context and reasoning

Many small positive cases can be denser without becoming unreadable. Consolidation reduces repeated full compiler/build/runtime setup while preserving distinct failure cases.

Do not create cross-feature mega-fixtures. Consolidate only cases with one semantic contract.

## Candidate clusters

### Fresh mutable argument forms

Potential combined contract:

- fresh template
- fresh collection
- fresh struct
- fresh computed scalar

Requirements:

- [ ] one readable program
- [ ] distinct output markers
- [ ] exact output
- [ ] keep existing-place negative cases separate

### Choice equality truth table

Potential combined contract:

- same unit
- different unit
- equal payload
- unequal payload
- unit versus payload
- nested equal
- nested unequal
- side effects evaluated once

Requirements:

- [ ] one exact truth-table output
- [ ] preserve backend representation units only where ABI is deliberate
- [ ] keep unsupported-payload diagnostics separate

### Named/default argument success

Potential combined contract:

- positional then named
- all named
- skipped defaults
- named mutable access
- constructor defaults if the source remains coherent

Requirements:

- [ ] one function/constructor scenario
- [ ] exact output
- [ ] keep duplicate/unknown/missing/order failures separate

### Collections/maps runtime operations

- [ ] consolidate only where one scenario already naturally tests ordered operations
- [ ] do not merge borrow failures into runtime success
- [ ] do not hide which operation failed

## Checklist for every cluster

- [ ] Identify the exact shared contract.
- [ ] Choose the strongest existing primary case as the destination.
- [ ] Add missing sub-scenarios and distinct output markers.
- [ ] Use exact/ordered assertions.
- [ ] Run the combined case before deleting anything.
- [ ] Delete superseded folders, expectations, goldens, and manifest entries.
- [ ] Update tags/contracts/roles.
- [ ] Record every deletion in the pruning ledger.
- [ ] Confirm failure localization remains acceptable.
- [ ] Keep negative reason cases focused.

## Phase 12 audit

- [ ] No combined fixture owns unrelated contracts.
- [ ] Positive coverage is denser.
- [ ] Negative diagnostics remain isolated.
- [ ] Manifest order remains deliberate.
- [ ] Deleted directories are fully removed.
- [ ] No stale golden or expectation remains.
- [ ] Suite wall time is recorded but not used as correctness proof.

## Style-guide review

- [ ] Cases are understandable without implementation knowledge.
- [ ] Output markers are semantic.
- [ ] No shared helper file became an unintended case.
- [ ] No broad fixture builder hides important source.

## Validation

Iteration by contract/tag, then:

```bash
cargo run --quiet -- tests --audit
just validate
```

## Documentation impact

- [ ] Update progress-matrix coverage wording only when case consolidation changes the useful description.
- [ ] Update this plan's pruning ledger.
- [ ] No language design change is expected.

## Phase 12 acceptance

- [ ] Selected positive clusters are consolidated.
- [ ] No semantic coverage was lost.
- [ ] Exact output passes.
- [ ] Full validation passes.

---

# Phase 13 — Normalize backend units around lowering and ABI contracts

## Context and reasoning

Backend units are valuable when they pin target-specific contracts. They are brittle when they act as the primary owner of language semantics by searching generated text.

Executed integration must own behaviour. Backend units should own:

- helper selection
- ABI/import/export shape
- operation-to-target mapping
- stable carrier representation
- target planning policy not visible through artifacts
- malformed-HIR/backend invariant handling

## Primary owners

- `src/backends/js/tests/`
- `src/backends/wasm/` tests
- artifact assertions in integration cases
- runtime integration cases

## Recommended slices

### 13A — Inventory and classify

For each backend test:

- [ ] name the intended contract
- [ ] classify as ABI, helper, mapping, planning, malformed HIR, artifact, or semantic substitute
- [ ] locate the integration runtime owner for semantic behaviour
- [ ] identify incidental generated-name/local-index assertions
- [ ] record candidates before deleting

### 13B — JavaScript lowering contracts

- [ ] Rename module/file comments from broad semantic ownership to lowering-contract ownership.
- [ ] Keep stable runtime helper and ABI tests.
- [ ] Keep explicit map/choice/error carrier contracts only when intentionally stable.
- [ ] Table-drive repetitive operator/operation mappings where readability improves.
- [ ] Delete source-text checks fully replaced by executed integration.
- [ ] Replace exact generated local names with semantic fragments or structured helper facts.
- [ ] Keep malformed-HIR failure tests.

### 13C — Wasm structural contracts

- [ ] Keep binary validation, required imports/exports, ABI, and unsupported-feature validation.
- [ ] Keep target-specific LIR/emit invariants.
- [ ] Use integration artifact assertions for final Wasm file structure where appropriate.
- [ ] Do not add runtime parity for unsupported surfaces.
- [ ] Record future executed Wasm parity as mixed-backend plan work.

### 13D — Artifact/golden alignment

- [ ] Move final artifact contracts to integration where they depend on project assembly.
- [ ] Keep backend-local units where no project artifact can expose the invariant.
- [ ] Delete duplicate goldens/text assertions.

## Phase 13 audit

- [ ] Language semantics are integration-owned.
- [ ] Backend representation contracts remain.
- [ ] No backend reparses source or reconstructs semantics.
- [ ] Exact generated names are contractual only when ABI requires them.
- [ ] Malformed-HIR coverage remains.
- [ ] Wasm support was not broadened.

## Style-guide review

- [ ] Backend tests state target-specific invariants.
- [ ] HIR builders are narrow.
- [ ] Table-driven tests remain readable.
- [ ] No broad macro abstraction was introduced.

## Validation

Iteration:

```bash
cargo test --quiet backends::js::tests -- --format terse
cargo test --quiet backends::wasm -- --format terse
cargo run --quiet -- tests --tag js-backend --backend html
cargo run --quiet -- tests --backend html_wasm
```

Final:

```bash
cargo fmt
git diff --check
just validate
```

## Documentation impact

- [ ] Update progress-matrix backend coverage summaries.
- [ ] Update `index.md` for moved backend test modules.
- [ ] Update backend educational docs only if they currently claim semantic ownership that moved.

## Phase 13 acceptance

- [ ] Backend units are intentional lowering/ABI owners.
- [ ] Semantic substitutes are removed.
- [ ] Runtime/artifact integration remains strong.
- [ ] Full validation passes.

---

# Phase 14 — Backfill ownership, enforce audit policy, run mutation probes, and close

## Context and reasoning

The final phase turns the project from a one-time cleanup into a durable test policy. It classifies remaining cases, makes audit failures part of validation, verifies that primary tests detect representative semantic mutations, updates documentation, and closes the roadmap plan.

## Checklist

### Complete contract/role classification

- [ ] Backfill `contract` and `role` for every canonical non-harness case.
- [ ] Classify adversarial cases explicitly.
- [ ] Classify compile-only cases as smoke.
- [ ] Reject duplicate primary contracts.
- [ ] Reject primary role without contract.
- [ ] Report contracts with no primary owner.
- [ ] Review every boundary/backend secondary owner.
- [ ] Remove obsolete free-form tags.
- [ ] Normalize tag spelling and ordering.

### Final integration audit policy

Hard failures:

- [ ] missing `expect.toml`
- [ ] implicit compile-only success
- [ ] duplicate case ID/path
- [ ] empty/unknown tags or roles
- [ ] duplicate primary contract
- [ ] unjustified diagnostic contains mode
- [ ] failure with no diagnostic codes
- [ ] expected warnings without warning identity
- [ ] invalid backend expectation shape
- [ ] undeclared fixture folder
- [ ] stale manifest path

Advisory findings to resolve or explicitly document:

- [ ] strict golden without declared exact artifact rationale
- [ ] message fragment without wording/label rationale
- [ ] normalized expectation fingerprint duplicates
- [ ] compile-only case that appears to have observable output
- [ ] unclassified unit source program
- [ ] contract with excessive secondary owners

### Put audit in the normal gate

- [ ] Add a fast audit command to `just validate` before full integration execution, or make canonical suite loading enforce every hard rule.
- [ ] Preserve `bean tests --audit` for the JSON report.
- [ ] Update `validation.bd` to describe the new gate.
- [ ] Ensure audit never mutates tracked files.
- [ ] Ensure report output stays under `target/`.

### Unit ownership final pass

- [ ] Re-run the Phase 0 source-shaped-unit inventory.
- [ ] Review every remaining full-source unit.
- [ ] Record why each remains.
- [ ] Delete stale test-only helpers and production APIs.
- [ ] Confirm TIR tests match the accepted one-store/exact-view owners recorded at `1298da468` without reopening internal architecture.
- [ ] Confirm build-system units remain policy-focused.
- [ ] Confirm HIR tests use semantic relationships.

### Targeted mutation probes

Use a temporary branch/worktree and do not commit deliberate faults.

Probe at least:

- [ ] mutable call-site access classification
- [ ] map `get` alias creation
- [ ] return-alias summary propagation
- [ ] fixed-capacity semantic identity
- [ ] match exhaustiveness
- [ ] checked numeric failure mode
- [ ] imported-root activation suppression
- [ ] target reachability for unsupported features
- [ ] reactive invalidation

For each probe:

- [ ] make one deliberate semantic defect
- [ ] run the expected primary filtered test
- [ ] confirm it fails for the intended contract
- [ ] revert the defect
- [ ] record the test and result in this plan
- [ ] do not claim general mutation score

A permanent mutation-testing dependency requires separate approval.

### Final measurements

- [ ] Record final unit count.
- [ ] Record final integration case and backend-execution counts.
- [ ] Record exact/contains diagnostic mode counts.
- [ ] Record compile-only count.
- [ ] Record strict/normalized golden counts.
- [ ] Record primary/boundary/backend/adversarial/smoke role counts.
- [ ] Record median unit and integration wall times under the same method as Phase 0.
- [ ] Record added, removed, merged, and strengthened cases.
- [ ] Record unit tests removed and retained by invariant category.
- [ ] Do not present lower counts or faster time as proof of correctness.

### Documentation and roadmap closure

- [ ] Finalize `testing.bd`.
- [ ] Finalize `validation.bd`.
- [ ] Update `CONTRIBUTING.md`.
- [ ] Update `index.md` for moved/removed test modules.
- [ ] Update progress-matrix coverage statements.
- [ ] Update compiler design diagnostic identity wording if Phase 4 made reason keys durable.
- [ ] Rebuild generated documentation.
- [ ] Mark this plan complete.
- [ ] Update `docs/roadmap/roadmap.md`.
- [ ] Refresh the canonical module plan against the final harness/test owners.
- [ ] Set the next active plan and capsule.

## Phase 14 audit

Perform the complete `AGENTS.md` final audit:

- [ ] Relevant style, compiler, build, memory, and language contracts are respected.
- [ ] Stage and subsystem ownership remain clear.
- [ ] No duplicated, legacy, or obsolete test/harness path remains.
- [ ] No unnecessary indirection or broad abstraction remains.
- [ ] Diagnostics use the correct lane and preserve source context.
- [ ] Tests protect behaviour or real hidden invariants.
- [ ] Progress matrix accurately reflects current coverage.
- [ ] Documentation names current owners and commands.
- [ ] Generated documentation came from source.
- [ ] No benchmark history changed accidentally.
- [ ] Final report states exactly what was validated.

## Style-guide review

- [ ] Harness files remain reviewable and below practical size targets.
- [ ] Module entry points are structural maps.
- [ ] Names are descriptive.
- [ ] Control flow is explicit.
- [ ] New comments explain WHAT/WHY.
- [ ] No lint suppression or compatibility shim hides unfinished cleanup.
- [ ] No test-only need preserved a bad production API.

## Validation

Focused audit:

```bash
cargo run --quiet -- tests --audit
```

Full code-bearing gate:

```bash
cargo fmt
git diff --check
just validate
```

Documentation release build, when documentation source changed:

```bash
bean build docs --release
```

Inspect the generated documentation diff.

## Phase 14 acceptance

- [ ] Audit hard findings are zero.
- [ ] Every canonical case has explicit ownership.
- [ ] Every removed test has replacement evidence.
- [ ] Representative mutations are caught by their primary owners.
- [ ] Full validation passes.
- [ ] Documentation is current.
- [ ] Roadmap is updated.
- [ ] Canonical module work can begin against the hardened suite.

---

## Final plan acceptance criteria

The plan is complete only when all of the following are true:

### Harness

- [ ] Tags, contracts, and roles are retained.
- [ ] Case/tag/contract/backend filtering works.
- [ ] List and audit modes work.
- [ ] Canonical expectations are explicit.
- [ ] Compile-only intent is explicit.
- [ ] Diagnostic codes are exact by default.
- [ ] Structured diagnostic reasons and locations are supported.
- [ ] Warning identities are supported.
- [ ] Runtime exact/order/exact-once assertions are supported.
- [ ] Audit policy is part of normal validation.

### Coverage

- [ ] Hashmap access/ownership semantics have primary integration owners.
- [ ] Cross-module access/effect summaries have current integration owners or explicit deferred handoffs.
- [ ] Reactive post-subscription mutation/update has current HTML-JS integration ownership.
- [ ] `check`/build frontend parity is covered.
- [ ] Root activation and fragment ordering are strongly asserted.
- [ ] Diagnostic source remapping is covered.
- [ ] Cross-backend cases use one input with intended success/rejection blocks.

### Pruning

- [ ] Source-shaped borrow/map duplicates are removed.
- [ ] Whole-source call/type/generic duplicates are removed.
- [ ] Backend semantic substitutes are removed.
- [ ] Positive micro-fixtures are consolidated where one contract allows it.
- [ ] Strict goldens are deliberate.
- [ ] Hidden facts, malformed HIR, canonical identity, transfer, and backend ABI tests remain.

### Governance

- [ ] Every canonical case has one role.
- [ ] Every primary case has one contract.
- [ ] No contract has multiple primary owners.
- [ ] Contains-mode diagnostics are justified.
- [ ] Pruning and coverage ledgers are complete.
- [ ] Mutation probes catch representative semantic faults.
- [ ] Documentation and roadmap are current.
- [ ] `just validate` passes.

---

## Final expected repository ownership

```text
tests/cases/
    primary user-visible language and project contracts
    cross-backend matrices
    runtime and artifact outcomes
    focused diagnostic failures

src/compiler_tests/integration_test_runner/
    manifest and expectation schema
    fixture loading
    selection and execution
    semantic/artifact/diagnostic assertion infrastructure
    audit and failure reporting
    harness self-tests

src/compiler_frontend/**/tests/
    parser algorithms
    canonical semantic identities
    HIR relationships and validation
    borrow/analysis side-table facts
    impossible states and transfer rules

src/backends/**/tests/
    target lowering
    stable ABI/helper/import/export contracts
    malformed backend input and planning invariants

src/build_system/tests/
src/projects/**/tests/
    graph, output, command, filesystem, and orchestration policy not expressible as a source artifact

docs/src/docs/codebase/style-guide/testing.bd
    durable test ownership, fixture, assertion, and pruning rules

docs/src/docs/codebase/style-guide/validation.bd
justfile
    executable completion gate
```

The final suite should make compiler internals easier to refactor because semantic acceptance, rejection, output, and project behaviour are protected at the language boundary, while narrow units continue to protect the hidden facts that make those outcomes correct.
