# Beanstalk Compiler Test Suite Hardening and Integration Coverage Plan

## Purpose

Harden Beanstalk's compiler test suite around one ownership rule:

- user-visible language and project behaviour is primarily owned by canonical integration cases under `tests/cases/`
- hidden compiler facts, impossible states, transfer rules, semantic identities, and narrow algorithms remain owned by focused unit tests
- backend tests own deliberate lowering, ABI, helper, target-validation, and malformed-IR contracts rather than substituting for executed language behaviour
- each semantic contract has one primary owner
- weaker or implementation-shaped duplicates are removed only after a stronger owner exists

The required order is:

```text
make the harness explicit and truthful
-> finish explicit success-contract migration
-> make diagnostics and runtime assertions exact
-> add missing primary integration owners
-> prune superseded units and fixtures
-> normalize backend ownership
-> enforce the final policy in validation
```

Do not begin broad pruning while success intent, diagnostic multiplicity, warning identity, or runtime order can still be misreported.

---

## Current state

ACTIVE_PLAN: `docs/roadmap/plans/compiler-test-suite-hardening-and-integration-coverage-plan.md`
STATUS: active
CURRENT_SLICE: Phase 14C5 AST fallible-handling unit ownership review
LAST_ACCEPTED_COMMIT: `4fa61e817` (Phase 14C4 AST declaration unit ownership)
WORKTREE: `main` at `/Users/aneirinjames/projects/beanstalk/beanstalk`; accepted code is committed; concurrent example-name work remains separately committed
REQUIRED_RELOADS: startup files, this plan, and current source/diff
RELEVANT_CONTEXT_NOW:
- docs: unit-test ownership, pruning, compiler-stage boundaries, and final governance rules govern the next slice
- code: AST fallible-handling source-shaped units are the next bounded family; later AST, HIR, build-system, backend, and final TIR owners still require review
ACCEPTANCE_CRITERIA:
- every remaining full-source unit has a distinct hidden invariant, parser fact, stage boundary, or policy owner
- units superseded by a stronger canonical integration primary are deleted with replacement evidence
- stale test-only helpers or production APIs are removed with their final caller
- HIR, build-system, backend, and final TIR units keep only their owning semantic relationships and hidden facts
VALIDATION_STATE:
- `just validate`: passed; cross-target Clippy, 3,511 Rust tests, 1,793 integration executions, docs check, and 28 benchmark cases
- Phase 14B audit: passed; zero hard findings; 66 backend-only and 15 adversarial-only primary-less contract advisories
- Priya expectation alignment: accepted at `6efac7012`; 13 stale Rust and integration expectation files now match renamed inputs
DOCS_IMPACT: testing, validation, and contributor workflow aligned with final suite policy; progress matrix and index unchanged
BLOCKERS_OR_OPEN_DECISIONS: none; 81 contract families without a primary are intentionally 66 backend-only and 15 adversarial-only families, with no ordinary orphan family
DELEGATION_DECISION: Ollama — bounded Phase 14 implementation slices
NEXT_WORKER_ORDER: Ollama only; no provider substitution
STOP_REASON: none
NEXT_RESUME_ACTION: launch the AST fallible-handling unit ownership review through Ollama

---

## Authority and invariants

Read before every code-bearing slice:

- `AGENTS.md`
- `docs/src/docs/codebase/style-guide/style-guide.bd`
- `docs/src/docs/codebase/style-guide/testing.bd`
- `docs/src/docs/codebase/style-guide/validation.bd`
- this plan
- the current implementation and diff
- `docs/src/docs/progress/#page.bst`

Also read:

- `docs/compiler-design-overview.md` for compiler stages, diagnostics, HIR, and backend handoff
- `docs/build-system-design.md` for project discovery, command policy, builders, artifacts, and output ownership
- `docs/src/docs/codebase/language/overview.bd` and selected language leaves for user-visible behaviour
- `docs/src/docs/codebase/memory-management/overview.bd` and selected memory leaves for access, aliasing, borrow validation, moves, drops, GC, and reactive liveness

Durable invariants:

1. There is one canonical integration runner and one expectation schema.
2. Manifest metadata is never reconstructed from display text or paths.
3. A backend baseline is a universal harness invariant, not a case-specific semantic assertion.
4. A successful case declares either an authored case-specific contract or explicit acceptance-only intent.
5. Borrow validation reads validated HIR and writes side tables; it does not rewrite HIR.
6. Test infrastructure consumes compiler-owned diagnostic identity and never creates a second diagnostic taxonomy.
7. Cross-backend matrices use the strongest supported backend-local contract; symmetry is not invented.
8. Tests do not pull deferred language, module, or backend features forward.
9. No broad pruning occurs before Phases 2R–5 close.
10. Benchmarks are operational evidence, not correctness coverage.

---

## Completed foundation

Keep historical detail in Git history. This table is the complete in-plan implementation record for accepted foundation work.

| Milestone | Accepted checkpoint | Durable result |
|---|---:|---|
| Phase 0 inventory | `b4503794e` | Baseline counts, ownership candidates, weak expectation inventory, and timing method recorded. |
| Phase 1 harness metadata and selection | `120c4baea` | Typed IDs/tags/contracts/roles, exact filters, list mode, and audit skeleton are present. |
| Phase 2A success intent | `603cc00d3` | Typed success intent exists and invalid combinations are rejected. |
| Phase 2 fallback removal | `e4d70489b` | Every canonical case requires its own `expect.toml`; fallback parser, constant, and stub are deleted. |
| Loop-carried borrow correction | `9f40480a9` | Active iterable mutation is rejected through CFG future-use facts without HIR mutation. |
| Phase 2 migration through static/runtime batch | `ba3366218` | Zero fallback-backed blocks and 35 backend-baseline-only blocks remain. |
| Durable pause checkpoint | `97d3174fd` | Work was pushed and paused cleanly before the next migration batch. |

No accepted language feature was broadened by this work. Coverage became stronger, and one previously missed invalid iterable-mutation program is now correctly rejected.

---

## Plan maintenance rules

This file is a reloadable execution plan, not a command transcript.

- Keep the **Active checkpoint** under 20 factual lines.
- Record only the latest full gate there.
- Add one row per accepted slice to the checkpoint log below; do not append narrative command histories.
- Replace stale counts and next actions instead of preserving superseded versions.
- Put detailed investigation notes in commits, PRs, or temporary files, not in this plan.
- Keep durable design decisions in the decision log only when they affect later phases.
- Update the pruning and coverage ledgers only when ownership changes.

### Checkpoint log

| Slice | Commit | Result | Authoritative counts after slice |
|---|---:|---|---|
| Foundation through Phase 2B11c | `ba3366218` | Accepted | 3,466 Rust tests; 1,784 integration executions; 23 current `compile_only`; 35 baseline-only; zero fallback |
| Plan pause | `97d3174fd` | Accepted | No code change |
| Phase 2R1 success intent and inventory | `5dc811a7c` | Accepted | 3,475 Rust tests; 1,784 integration executions; 23 acceptance-only; 33 baseline-only; zero hard findings |
| Phase 2R2 golden inventory | `ca2d013ee` | Accepted | 3,480 Rust tests; 1,784 integration executions; 38 file-backed golden blocks; 17 orphaned modes removed; zero hard findings |
| Phase 2R3 suite policy owner | `7f9078329` | Accepted | 3,486 Rust tests; 1,784 integration executions; 33 baseline-only advisories; zero hard findings |
| Phase 2R4 path containment | `5e39b34aa` | Accepted | 3,496 Rust tests; 1,784 integration executions; canonical fixture/input/entry containment enforced; zero hard findings |
| Phase 2R5a choice-construction contracts | `8992704e9` | Accepted | 17 acceptance-only; 526 rendered-output blocks; six choice cases now observe variants/payloads; zero hard findings |
| Phase 2R5b named fixture contracts | `dee1a6176` | Accepted | 17 acceptance-only; 528 rendered-output blocks; struct default, Bool branch, and current-config wording now observed; zero hard findings |
| Phase 2R5c1 named weak runtime markers | `d036eef0d` | Accepted | 528 rendered-output blocks; ten named fixtures now use context-rich markers; zero hard findings |
| Phase 2R5c2 borrow/adversarial markers | `f1ea28e3f` | Accepted | 528 rendered-output blocks; ten borrow/adversarial fixtures now use context-rich markers; zero hard findings |
| Phase 2R5c3 call/result/option markers | `58ce37ee2` | Accepted | 528 rendered-output blocks; ten call/result/option fixtures now observe context-labeled behavior; zero hard findings |
| Phase 2R5c4 receiver/struct markers | `1a3ff086c` | Accepted | 528 rendered-output blocks; six receiver/struct/multi-file fixtures now observe context-labeled values; zero hard findings |
| Phase 2R5c5 marker/role closure | `bd070da33` | Accepted | 17 acceptance-only smoke cases; two HTML-Wasm parity cases are backend-role; 528 rendered-output blocks; zero hard findings |
| Phase 2R6a CFG/projected actors | `a6a1ad0a9` | Accepted | 3,499 Rust tests; CFG-accurate naming; projected user/compiler origin distinction; 1,784 integration executions |
| Phase 2R6b loop-borrow edge matrix | `8dc9f6049` | Accepted | 3,504 Rust tests; 1,652 cases and 1,785 executions; one primary plus one boundary integration owner; zero hard findings |
| Phase 2R7b inferred assignment-move fact | `26a707729` | Accepted | 3,505 Rust tests; source `UNINIT`, target `SLOT`, and empty target aliases protected by one focused snapshot test |
| Phase 2R7c lifetime/final-use ownership | `5fbf183a4` | Accepted | 1,645 cases and 1,778 executions; 12 acceptance-only; 32 baseline-only; 529 rendered-output; zero hard findings |
| Phase 2R8 workflow documentation | `4fa34390c` | Accepted | 1,645 cases and 1,778 executions; 32 baseline-only; codebase-standards and progress routes rebuilt from corrected source |
| Phase 2D1 trait/order/package/config contracts | `9a929d9d5` | Accepted | 13 acceptance-only; 26 baseline-only; 532 rendered-output; 257 artifact; zero hard findings |
| Phase 2D2 same-module choice visibility | `4e0b4ca7b` | Accepted | 13 acceptance-only; 25 baseline-only; 533 rendered-output; 257 artifact; zero hard findings |
| Phase 2D3a1 package/facade/import contracts | `7597f5106` | Accepted | 13 acceptance-only; 19 baseline-only; 538 rendered-output; 258 artifact; zero hard findings |
| Phase 2D3a2 module-boundary contracts | `12c23040c` | Accepted | 13 acceptance-only; 16 baseline-only; 541 rendered-output; 258 artifact; zero hard findings |
| Phase 2D3b canvas package contracts | `747e8436d` | Accepted | 13 acceptance-only; 13 baseline-only; 541 rendered-output; 261 artifact; zero hard findings |
| Phase 2D3c HTML-Wasm content parity contracts | `b537bee80` | Accepted | 13 acceptance-only; 7 baseline-only; 541 rendered-output; 267 artifact; zero hard findings |
| Phase 2D3d const-record contracts | `b7a60487d` | Accepted | 13 acceptance-only; 3 baseline-only; 545 rendered-output; 267 artifact; zero hard findings |
| Phase 2D3e final baseline contracts | `5ae591167` | Accepted | 13 acceptance-only; zero baseline-only; 548 rendered-output; 267 artifact; zero hard findings |
| Phase 2E authored completeness enforcement | `1b97b360b` | Accepted | 3,511 Rust tests; 1,645 cases and 1,778 executions; 13 acceptance-only; zero baseline-only; zero hard findings |
| Phase 3A assertion ownership split | `e86a5d660` | Accepted | Seven assertion-family modules; 27 focused assertion tests; 3,511 Rust tests; 1,778 integration executions |
| Phase 3B exact diagnostic multisets | `69a0f38dc` | Accepted | Exact default with duplicate counts; 19 justified contains backend blocks; 3,523 Rust tests; 1,778 integration executions |
| Phase 3C1 warning code identity | `8fdf00022` | Accepted | Schema 4 exact warning-code multisets; 130 focused runner tests; 3,535 Rust tests; 21 count-only fixtures remain |
| Phase 3D1 pattern/match warning migration | `a01728438` | Accepted | 15 exact-code fixtures; duplicate `BST-RULE-0022` counts preserved; six count-only fixtures remain |
| Phase 3D2 import-alias warning migration | `c566da516` | Accepted | `BST-IMPORT-0003` exact counts preserved across three fixtures; three count-only fixtures remain |
| Phase 3D3 final warning migration | `d1057bc5a` | Accepted | All 21 exact-warning fixtures author stable code multisets; zero canonical `warning_count` occurrences |
| Phase 3C2 warning compatibility deletion | `e3f820ec1` | Accepted | Schema 5 exact-code-only warnings; 129 focused runner tests; 3,534 Rust tests; no active count path |
| Phase 3E contains-mode hard policy | `ec6c828a1` | Accepted | One hard-policy owner; 135 focused runner tests; 19 justified canonical contains blocks; zero hard findings |
| Phase 3F assertion workflow documentation | `9cc05cf61` | Accepted | Schema 5 exact multiset and justified-contains workflow documented; release docs build passed; `index.md` current |
| Phase 4A compiler-owned reason identity | `afd00ca04` | Accepted | 50 reason-bearing payload families; 580 unique qualified keys; 68 focused model tests; 3,544 Rust tests |
| Phase 4B structured diagnostic assertions | `3dab97639` | Accepted | Code-plus-occurrence selectors; compiler-owned reasons; fixture-relative primary/secondary locations; 145 focused runner tests; 3,555 Rust tests |
| Phase 4C1 fixed-collection reasons | `124bee9f6` | Accepted | 22 `BST-SYNTAX-0016` fixtures distinguish nine `invalid_collection_type.*` reasons; 27/27 focused executions; zero hard findings |
| Phase 4C2 call-shape reasons | `a4f20eedc` | Accepted | All 27 `BST-RULE-0054` fixtures distinguish eleven `invalid_call_shape.*` reasons; 1,778/1,778 integration executions; zero hard findings |
| Phase 4B location correction / Phase 4C3 mutable access | `c5af4f2a2` | Accepted | One-based display coordinates and fixture-input-relative scopes; primary borrow fixture asserts reason/path/line; 146 focused runner tests; 3,556 Rust tests |
| Phase 4C4 trait-name misuse locations | `5d4a6c84c` | Accepted | Eight `BST-RULE-0075` fixtures and nine backend blocks assert fixture-relative path/line without inventing a reason taxonomy; 1,778/1,778 executions |
| Phase 4C5 invalid import-clause reasons | `e33d1f6e9` | Accepted | Ten `BST-SYNTAX-0019` fixtures distinguish eight reasons; dedicated per-entry-plus-trailing reason is reachable; dead parser check removed; 3,556 Rust tests |
| Phase 4C6 invalid import-path locations | `b2992ef92` | Accepted | Both `BST-IMPORT-0016` fixtures assert `parent_directory_segment` with root/nested fixture-relative paths and one-based lines; 1,778/1,778 executions |
| Phase 4C7 invalid fallible-handling reasons | `5d72c1b75` | Accepted | All 36 `BST-RULE-0051` fixtures distinguish 17 compiler-owned reasons; 12 redundant rendered wording fragments removed; 1,778/1,778 executions |
| Phase 4C8a backend feature reason identity | `a9661ada0` | Accepted | Nine reachable backend-feature reasons replace interned feature prose; unreachable runtime-fragment rejection identity removed; 3,557 Rust tests and 1,778 integration executions |
| Phase 4C8b backend feature reason assertions | `d71dfa6a5` | Accepted | All 29 `BST-RULE-0064` fixtures distinguish seven exercised reasons; 24 redundant rendered wording fragments removed; 1,778/1,778 executions |
| Phase 4C9 generated generic remapping | `a72876029` | Accepted | Recursive generic instantiation keeps the call site primary while body, declaration, and substitution labels remap to a helper source; rendered label prose removed; 1,778/1,778 executions |
| Phase 4C10 declaration/conflict secondary labels | `83aecdad2` | Accepted | Duplicate declarations and duplicate exclusive accesses assert primary and earlier-site secondary paths/lines through rule and borrow diagnostic lanes; 1,778/1,778 executions |
| Phase 4D structured assertion documentation | `14bd9178b` | Accepted | Testing/contributor workflow and progress coverage now describe compiler-owned reasons plus primary/secondary remapping; release docs rebuilt and owned routes inspected |
| Phase 5A chronological runtime-event model | `786171dd3` | Accepted | One typed event array preserves console/fragment chronology; strict decoding and chronological/channel views have focused coverage; 3,562 Rust tests and 1,778 integration executions |
| Phase 5B stronger runtime assertion schema | `323d54aae` | Accepted | Typed exact/order/exact-once contracts use one Node harness; schema 6 reports all five runtime forms; 3,575 Rust tests and 1,778 integration executions |
| Phase 5C1 root and fragment runtime order | `2871e776d` | Accepted | Active and facade root markers execute once; imported root remains suppressed; runtime fragment exact-once/order complements static slot order; 1,778/1,778 executions |
| Phase 5C2 exact runtime behavior | `c32dea5ef` | Accepted | Loop-control output is ordered and single-use; loop/map/reactive cases assert complete exact output; 1,778/1,778 executions |
| Phase 5D2 runtime golden conversion | `d5256868b` | Accepted | 15 behavior-first whole-page goldens replaced by exact runtime output; 23 golden backend blocks remain; 1,778/1,778 executions |
| Phase 5D3 receiver golden conversion | `914c1c131` | Accepted | Nine receiver whole-page goldens replaced by exact context-rich output; seven scalar markers relabeled; 14 golden backend blocks remain; 1,778/1,778 executions |
| Phase 5D4 template/import golden conversion | `71f75c220` | Accepted | Nine whole-page goldens replaced by runtime and narrow static owners; order-sensitive console markers are exact-once; five golden backend blocks remain; 1,778/1,778 executions |
| Phase 5D5 static golden conversion | `952bd3134` | Accepted | Final five whole-page goldens replaced by narrow static content/order/exact-once and absence contracts; zero golden backend blocks; 1,778/1,778 executions |
| Phase 5 documentation and acceptance closure | `fc3b25eaa` | Accepted | Schema 6 chronology, exact/order/exact-once runtime workflow, line-ending-only normalization, and zero-golden policy documented; release docs build passed |
| Phase 6 live hashmap lookup alias conflict | `3e27e8d95` | Accepted | Fallible-success aliases survive catch joins; final-use semantics are restored; live `get` alias blocks `set` with exact reason and primary/secondary locations; 1,646 cases and 1,780 executions |
| Phase 6 alias final-use and explicit-copy independence | `d002a80cf` | Accepted | Two primary HTML runtime owners prove final-use mutation and non-copy explicit-copy independence with HTML-Wasm structured rejection; 1,648 cases and 1,784 executions |
| Phase 6 removed-value ownership and borrowed lookup keys | `d9e4d7fa3` | Accepted | Two primary HTML runtime owners prove removed non-copy values outlive later mutation/clear and lookup keys remain usable after contains/get/remove; 1,650 cases and 1,788 executions |
| Phase 6 inserted ownership and explicit-copy insertion | `4c323a4f4` | Accepted | Three primary negatives distinguish immutable `set` later-use access from literal key/value ownership transfer; one exact runtime owner proves copied insertion independence; 1,654 cases and 1,796 executions |
| Phase 7 hashmap unit pruning | `f6c514180` | Accepted | Fifteen source-shaped units removed; five hidden get/remove/set/nested-literal state facts retained in the fact owner; 3,566 Rust tests and 1,796 integration executions |
| Phase 8 function-call ownership | `720c23e9a` | Accepted | Eighteen whole-source units removed; eleven parser/access/location facts retained; computed mutable-rvalue behavior gained one exact-output primary integration owner; 3,548 Rust tests and 1,797 integration executions |
| Phase 8 type-resolution ownership | `53039a0e4` | Accepted | Four source-shaped capacity rejection units removed; eighteen hidden bridge/folding/TypeId/registration/map facts retained; struct-field and return-slot zero-capacity diagnostics gained primary integration owners; 3,544 Rust tests and 1,799 integration executions |
| Phase 8 generic ownership | `034f919ed` | Accepted | Three user-behavior scope units and one datatype-renderer wording unit removed; 24 binding/identity/substitution/rollback facts and nine AST remapping facts retained; 11 canonical diagnostics strengthened; 3,540 Rust tests and 1,799 integration executions |
| Phase 9 facade effect summaries | `6d92cbd1e` | Accepted | Three primary facade cases cover return-alias liveness/final use, fresh returns, and mutable-parameter access across re-exports; existing generated-generic remapping owns supported generated labels; 3,540 Rust tests and 1,804 integration executions |
| Phase 10A check/build frontend parity | `6a2c2e6b0` | Accepted | One focused command test compares exact typed warning and error identity across check/build seams, including reason/path/line/multiplicity, and proves check writes no artifacts; 3,541 Rust tests and 1,804 integration executions |
| Phase 10B root/fragment activation | `68fb90fb6` | Accepted | Existing exact/ordered owners cover active/imported/API-only roots, mixed fragment order, loop-control output, and non-duplicated hydration; API-only imported runtime output is now exact-once; 3,541 Rust tests and 1,804 integration executions |
| Phase 11 reactive mutation ownership | `780d0419c` | Accepted | Exact HTML events protect one mount plus one batched rerender, the mount call is exact-once, HTML-Wasm rejects the same source structurally, and reactive-parameter assignment has a primary reason/path/label owner; 3,541 Rust tests and 1,807 integration executions |
| Phase 12A fresh mutable values | `73f13864c` | Accepted | Four positive fresh-rvalue forms consolidated into one exact-output primary owner; every mutable callee mutates its fresh local; explicit-copy and negative diagnostic owners remain separate; 3,541 Rust tests, 1,658 cases, and 1,804 integration executions |
| Phase 12B choice equality truth table | `f601ebe2c` | Accepted | Eight local equality micro-fixtures consolidated into one exact-output primary truth table; single-evaluation, imported/aliased/generic, constructor-routing, and negative owners remain separate; 3,541 Rust tests, 1,651 cases, and 1,797 integration executions |
| Phase 12C named/default arguments | `4b9bd49e7` | Accepted | Three ordinary call-routing successes consolidated into one exact-output primary owner for mixed positional/named, reversed all-named, and skipped-default binding; mutable named access, constructors, and diagnostics remain separate; 3,541 Rust tests, 1,649 cases, and 1,795 integration executions |
| Phase 12D ordered collection runtime | `7c4e33a99` | Accepted | Literal/set/method semantic positives consolidated into one exact ordered push/set/remove/readback primary owner; map ordering, empty/fixed/control-flow, and backend artifact owners remain separate; 3,541 Rust tests, 1,647 cases, and 1,793 integration executions |
| Phase 13A JS value/call mappings | `c728e315f` | Accepted | All 24 choice, expression, map-statement, receiver-call, and fallible-result tests are deliberate operation mapping, ABI, helper, or carrier owners; no semantic substitute was deleted; one stale option-carrier banner corrected; 3,541 Rust tests and 1,793 integration executions |
| Phase 13B1 JS control/value ABI | `7350c0825` | Accepted | All 32 control-flow, binding, and value-use tests are deliberate CFG/jump, place/helper, alias-write, or load/copy/call/return ABI owners; three match-temp assertions no longer pin incidental numeric suffixes; 3,541 Rust tests and 1,793 integration executions |
| Phase 13B2 JS numeric/reactive mappings | `ecbce5fc6` | Accepted | All 37 numeric and reactive tests are deliberate failure-mode mapping, helper selection/contract, runtime ABI, invalidation/scheduler, or malformed-HIR owners; no substitute deleted; stale Phase 7 mounting wording removed; 3,541 Rust tests and 1,793 integration executions |
| Phase 13B3 JS runtime-helper ownership | `b42e9ab6f` | Accepted | All 65 runtime-helper tests classified; one weaker collection-push success-carrier duplicate removed; 64 helper-selection, helper-contract, and ABI owners retained; 3,540 Rust tests and 1,793 integration executions |
| Phase 13B4 JS policy and symbol ownership | `b87477d98` | Accepted | All 41 prelude, host, symbol, inline-expression, and emission-policy tests retained as helper selection/ordering, ABI, mapping, planning, or malformed-input owners; stale legacy wording corrected; 3,540 Rust tests and 1,793 integration executions |
| Phase 13C Wasm backend ownership | `c1fab2a91` | Accepted | All 28 Wasm lowering, binary-emission, and shared feature-validation tests retained as LIR mapping, ABI, planning, malformed-input, artifact, or structured target-rejection owners; incidental LIR block/local ids replaced by semantic relationships; 3,540 Rust tests and 1,793 integration executions |
| Phase 13D HTML project artifact ownership | no code change after `ccc692886` | Accepted | All 24 HTML builder and four HTML-Wasm artifact tests retained as capability, artifact, planning, mapping, config-reason, or ABI owners; single-file and deeper nested-route contracts lack stronger canonical integration owners; latest full gate remains 3,540 Rust tests and 1,793 integration executions |
| Phase 13E HTML assembly-policy ownership | `ca89c014e` | Accepted | All 55 document, JS-path, output-plan, metadata, registry, and tracked-asset tests classified; 14 weaker artifact substitutes removed in favor of nine canonical integration owners; 41 hidden config, escaping, mapping, registry, and warning-policy owners retained; 3,526 Rust tests and 1,793 integration executions |
| Phase 13F external-JS runtime ownership | no code change after `461559f1e` | Accepted | All 23 external-JavaScript runtime-emission and glue tests retained as ABI, helper, mapping, planning, or malformed-input owners; canonical integration owns reachable/unreachable page behavior; latest full gate remains 3,526 Rust tests and 1,793 integration executions |
| Phase 14A classification batch 1 | `96130e3f4` | Accepted | First 100 manifest entries reviewed; 95 contracts and 86 roles added; two boundary-only families given explicit primary owners; zero hard findings; 1,516 missing roles and 1,527 missing contracts remain; 3,526 Rust tests and 1,793 integration executions |
| Phase 14A classification batch 2 | `e403e628f` | Accepted | Entries 101–200 reviewed; 100 contracts and 100 roles added; generic, trait, and choice families consolidated under one primary with deliberate boundaries; zero hard findings; 1,416 missing roles and 1,427 missing contracts remain; 3,526 Rust tests and 1,793 integration executions |
| Phase 14A classification batch 3 | `324103cd3` | Accepted | Entries 201–300 reviewed; 99 incomplete cases classified; generic and collection families reuse existing primaries, while backend-only carrier/lowering contracts remain backend-role owners; zero hard findings; 1,319 missing roles and 1,328 missing contracts remain; 3,526 Rust tests and 1,793 integration executions |
| Phase 14A classification batch 4 | `3987cef7a` | Accepted | Entries 301–500 reviewed; 197 contracts and 192 roles added, one incorrect smoke role repaired, shared fixed-collection/config/pattern families consolidated, and tag spacing normalized; zero hard findings; 1,127 missing roles and 1,131 missing contracts remain; 3,526 Rust tests and 1,793 integration executions |
| Phase 14A classification batch 5 | `76ae0584f` | Accepted | Entries 501–700 reviewed; 185 contracts and roles added, three whole-case acceptance-only fixtures retained as smoke, shared option/result/loop/path families consolidated, and the canonical hashmap runtime/order case confirmed as a language primary; zero hard findings; 942 missing roles and 946 missing contracts remain; 3,526 Rust tests and 1,793 integration executions |
| Phase 14A classification batch 6 | `139a88195` | Accepted | Entries 701–900 reviewed; 200 contracts and roles added, 32 boundary cases consolidated under deliberate normal primaries, runtime cases remained language-primary, and template const-loop variants reused matching loop contracts without creating duplicate primaries; zero hard findings; 742 missing roles and 746 missing contracts remain; 3,526 Rust tests and 1,793 integration executions |
| Phase 14A classification batch 7 | `4908f02e0` | Accepted | Entries 901–1100 reviewed; 199 contracts and roles added while one whole-case acceptance-only fixture remained smoke, cast/char/assert families gained explicit ownership, eight target-lowering/ABI cases remained backend-role, and the shared unhandled-fallible-expression family received context-neutral naming; zero hard findings; 543 missing roles and 547 missing contracts remain; 3,526 Rust tests and 1,793 integration executions |
| Phase 14A classification batch 8 | `925d446e9` | Accepted | Entries 1101–1300 reviewed; 200 contracts and roles added, four JS lowering cases remained backend-role, observable Core package behavior stayed primary, and config/facade policy families were narrowed so distinct rejection reasons do not hide under broad secondary ownership; zero hard findings; 343 missing roles and 347 missing contracts remain; 3,526 Rust tests and 1,793 integration executions |
| Phase 14A classification batch 9 | `901fcbacd` | Accepted | Entries 1301–1500 reviewed; 200 contracts and roles added, eight target/reachability/asset cases remained backend-role, and namespace misuse, value-producing if, match-warning, and facade private-type families were split into distinct ownership contracts; zero hard findings; 143 missing roles and 147 missing contracts remain; 3,526 Rust tests and 1,793 integration executions |
| Phase 14A classification batch 10 | `6d51d6e07` | Accepted | Entries 1501–1647 reviewed; all 143 incomplete cases classified, reactive subscription positions were narrowed into distinct semantic owners, 22 target/helper cases remained backend-role, and zero roles remained missing; 3,526 Rust tests and 1,793 integration executions |
| Phase 14A classification closure | `292b50350` | Accepted | Eight invented contracts were removed from whole-case acceptance-only smoke fixtures; all 1,647 cases now have explicit roles, every non-smoke case has a contract, and the only primary-less contract families are 66 backend-only plus 15 adversarial-only owners; zero hard findings; 3,526 Rust tests and 1,793 integration executions |
| Phase 14B final suite policy | `5614e7d01` | Accepted | Missing roles and non-smoke contracts are hard findings; contractless whole-case smoke remains valid; primary-less families are reported once with backend-only and adversarial-only ownership distinguished; normal execution and audit share one evaluator; zero hard findings; 3,535 Rust tests and 1,793 integration executions |
| Phase 14C1 AST collection units | `eb6c92d9e` | Accepted | Fourteen source-shaped diagnostic units removed in favor of canonical primaries with exact structured reasons, reason-specific rendered assertions, or unique diagnostic families; 50 AST shape, TypeId, operation identity, source-path, and broad-code payload owners retained; 3,521 Rust tests and 1,793 integration executions |
| Phase 14C2 AST branching units | `329cf59f5` | Accepted | Two match-arm diagnostic units removed in favor of canonical cases asserting the same code plus reason-specific rendered text; 38 AST shape, precedence, parser-boundary, exhaustiveness, keyword-boundary, and unmirrored payload owners retained; 3,519 Rust tests and 1,793 integration executions |
| Phase 14C3 AST function-parsing units | `5962d65b3` | Accepted | Four receiver, catch-fallthrough, and multi-return trailing-comma units removed in favor of reason-specific or exact-source canonical owners; 43 signature-shape, return-slot, generic-instance, call-node, fallible-shape, and broad-code payload owners retained; 3,515 Rust tests and 1,793 integration executions |
| Phase 14C4 AST declaration units | `4fa61e817` | Accepted | Four regular-division and fixed-collection diagnostic units removed in favor of exact-source or structured-reason canonical owners; 19 declaration-shape, named-type, reserved-name, multiline-token, and fixed/growable identity owners retained; 3,511 Rust tests and 1,793 integration executions |

---

## Slice execution rules

A normal slice has one primary owner and one coherent result. Prefer one of:

- one harness data-model or parser correction
- one policy/reporting correction
- one semantic compiler fix
- one fixture family migration
- one unit-test ownership family
- one backend contract family
- one documentation and closure checkpoint

Rules:

- do not combine harness-schema work with an unrelated semantic compiler fix
- do not combine adding primary integration coverage with deleting unrelated units
- only one active worker may edit `tests/cases/manifest.toml`
- only one active worker may edit runner core or expectation schema
- use disjoint fixture workers only when manifest integration is serialized
- compare worker changes with current `main` before integration
- do not leave compatibility aliases or parallel parsing/assertion paths after acceptance
- stop when stronger coverage exposes a compiler defect; fix the root cause in a separate slice
- never weaken an expectation merely to restore a green gate

### Standard code-bearing gate

For every accepted code-bearing slice:

```bash
cargo fmt
# focused unit/integration commands for the slice
git diff --check
just validate
```

Also:

- review changed tests against `testing.bd`
- review implementation style against `style-guide.bd`
- review stage ownership and diagnostic lanes where applicable
- review the progress matrix and update it only when current support or coverage wording materially changes
- rebuild generated documentation from source when documentation changes
- perform the final `AGENTS.md` audit
- update the active checkpoint and add one checkpoint-log row
- commit the accepted slice before starting the next slice

For strictly documentation-only changes:

```bash
cargo run --quiet -- build docs --release
```

Inspect the generated documentation diff and confirm that only documentation changed.

---

# Phase 2R — Review corrections before continuing Phase 2

## Goal

Correct the semantic and reporting drift found in the paused implementation before migrating the remaining baseline-only blocks.

Phase 2R is mandatory. Do not resume the former 2B11d batch until every 2R slice is accepted.

## Non-goals

- no exact diagnostic multiset migration yet
- no warning-code schema migration yet
- no ordered runtime-event model yet
- no broad unit pruning
- no canonical module implementation
- no Wasm feature expansion
- no new language semantics

## Primary owners

- `src/compiler_tests/integration_test_runner/types.rs`
- `src/compiler_tests/integration_test_runner/expectations.rs`
- `src/compiler_tests/integration_test_runner/fixture.rs`
- `src/compiler_tests/integration_test_runner/reporting.rs`
- `src/compiler_tests/integration_test_runner/runner.rs`
- a new narrow suite-policy owner if required
- harness self-tests
- selected canonical fixtures named below
- borrow-checker conflict transfer and focused tests
- `docs/src/docs/codebase/style-guide/testing.bd`
- `CONTRIBUTING.md`
- this plan and the progress matrix where wording changed

---

## 2R1 — Make success intent match actual execution

The current `compile_only` name and audit output imply that only compilation is checked, while every successful backend still executes its universal backend baseline. Preserve the baseline, but make the case-specific intent truthful.

### Required implementation

- [x] Rename `SuccessContract::CompileOnly` to `SuccessContract::AcceptanceOnly`.
- [x] Change the schema spelling from `success_contract = "compile_only"` to `success_contract = "acceptance_only"`.
- [x] Migrate every current canonical occurrence atomically.
- [x] Delete the old spelling with no compatibility alias or fallback.
- [x] Define acceptance-only as: **no case-specific semantic, artifact, golden, absence, or expected-warning assertion beyond the always-on backend baseline**.
- [x] Continue applying the HTML and HTML-Wasm baselines to acceptance-only cases.
- [x] Keep acceptance-only mutually exclusive with case-specific assertions.
- [x] Treat a non-default expected-warning contract as authored behaviour rather than acceptance-only.
- [x] Do not require `role = "smoke"` for a mixed-backend case whose other backend owns a stronger boundary/backend contract.
- [x] Require `role = "smoke"` when the whole canonical case is acceptance-only or orchestration-only.

### Inventory fidelity

- [x] Report the universal backend baseline independently from acceptance-only intent.
- [x] An acceptance-only backend must report both `backend_baseline` and `acceptance_only` assertion kinds.
- [x] Add explicit `baseline_applied` or an equivalent unambiguous field.
- [x] Rename report fields and finding codes that still say implicit/compile-only where the meaning changed.
- [x] Add summary counts for acceptance-only, baseline-only, rendered output, artifacts, goldens, absence, and expected-warning contracts.
- [x] Bump the audit schema version.

### Self-tests

- [x] Acceptance-only HTML still fails a broken HTML baseline.
- [x] Acceptance-only HTML-Wasm still fails invalid Wasm or missing required baseline exports.
- [x] Acceptance-only does not require a fixture-specific source marker.
- [x] Inventory records both baseline and acceptance intent.
- [x] The removed `compile_only` spelling is rejected.
- [x] Mixed-backend acceptance-only parity remains valid without forcing the whole case to be smoke.

### Acceptance

- [x] No `compile_only` schema or enum spelling remains.
- [x] Audit output describes what actually runs.
- [x] Focused harness tests and the full gate pass.

---

## 2R2 — Close golden-contract loopholes

Golden presence and golden comparison must use one file inventory. Empty directories must never satisfy fixture completeness.

### Required implementation

- [x] Establish one recursive golden-file discovery owner used by both fixture validation and golden comparison.
- [x] If this requires creating `assertions/goldens.rs` early, move the existing golden logic rather than copying it; Phase 3 will complete the remaining assertion split.
- [x] Count files, not immediate directory entries.
- [x] An empty `golden/<backend>/` directory does not count as a contract.
- [x] An empty nested directory does not count as a contract.
- [x] Nested actual files do count.
- [x] Reject explicit `golden_mode` when the backend has no golden files.
- [x] Report `golden_mode = null` when no golden is present.
- [x] Preserve strict as the default only when golden files exist and no mode is authored.
- [x] Remove empty golden directories discovered during migration.

### Self-tests

- [x] No golden directory.
- [x] Empty backend golden directory.
- [x] Empty nested golden directory.
- [x] Nested golden file.
- [x] Explicit `golden_mode` without files.
- [x] Audit JSON consistency between `golden_present` and `golden_mode`.

### Acceptance

- [x] A directory-only golden cannot bypass final success-contract enforcement.
- [x] Fixture validation and comparison use the same discovered file set.
- [x] Focused harness tests and the full gate pass.

---

## 2R3 — Give suite policy one owner

Manifest parsing, audit reporting, and normal execution currently duplicate or disagree about hard policy.

### Ownership boundary

- **Manifest parser:** TOML shape, required local fields, typed role spelling, duplicate IDs/paths, and local lexical validity.
- **Fixture loader:** contained filesystem paths, required files/directories, expectation parsing, and typed suite construction.
- **Suite policy evaluator:** cross-case ownership and assertion-strength rules.
- **Reporting:** serialization and human-readable presentation only.
- **Runner:** invokes the evaluator and enforces its result.

### Required implementation

- [x] Add one narrow suite-policy evaluator, for example `policy.rs`.
- [x] Move duplicate-primary, primary-without-contract, acceptance-only role, baseline-only, and related cross-case rules into that owner.
- [x] Remove duplicated policy reconstruction from `reporting.rs` and `manifest.rs` where it no longer owns the rule.
- [x] Normal list/execution rejects hard policy findings before selection or compilation.
- [x] Audit writes the JSON report even when hard findings exist, then returns an error/nonzero result.
- [x] Advisory findings remain non-fatal.
- [x] Keep malformed TOML and unsafe filesystem shape as immediate loader errors rather than reportable policy findings.
- [x] Make hard/advisory ordering deterministic.

### Self-tests

- [x] Duplicate primary contract is produced once by the policy owner.
- [x] Primary without contract is produced once by the policy owner.
- [x] Audit serializes hard findings and fails.
- [x] Normal execution fails before compiling when hard findings exist.
- [x] Advisories serialize without failing.
- [x] Reporting contains no independent policy taxonomy.

### Acceptance

- [x] Every policy rule has one owner.
- [x] Audit hard findings are operationally meaningful.
- [x] Focused harness/CLI tests and the full gate pass.

---

## 2R4 — Contain fixture and entry paths

Canonical tests must be self-contained and deterministic.

### Required implementation

- [x] Reject absolute manifest case paths.
- [x] Reject `..`, root, and platform-prefix components in manifest case paths.
- [x] Reject `.` components unless a documented normalized form explicitly permits them.
- [x] Canonicalized fixture roots must remain inside the canonical suite root, including symlink resolution.
- [x] Preserve `entry = "."` as the one explicit directory-entry sentinel.
- [x] Otherwise reject absolute, parent, root, prefix, and current-directory components in `entry`.
- [x] Canonicalized entry paths must remain inside the case `input/` directory.
- [x] Reject leading/trailing whitespace in IDs, tags, contracts, paths, and entries rather than silently changing identity.
- [x] Reject duplicate tags within one case.
- [x] Keep lowercase/tag-family normalization for final Phase 14 unless a current duplicate requires immediate correction.

### Self-tests

- [x] Absolute manifest path.
- [x] Parent-traversing manifest path.
- [x] Symlink escape from suite root where supported by the test platform.
- [x] Absolute entry.
- [x] Parent-traversing entry.
- [x] Symlink escape from `input/` where supported.
- [x] Whitespace-padded metadata.
- [x] Duplicate tags.
- [x] Valid nested contained path.

### Acceptance

- [x] A canonical case cannot load source outside its fixture.
- [x] Metadata identity is canonical and deterministic.
- [x] Focused harness tests and the full gate pass.

---

## 2R5 — Correct recently migrated fixture contracts

These are targeted corrections, not a broad fixture rewrite.

### Choice construction cases

The following cases currently contain runtime output but were classified as acceptance-only:

- `choice_const_payload_success`
- `choice_const_unit_success`
- `choice_imported_payload_constructor_success`
- `choice_payload_constructor_mixed_success`
- `choice_payload_constructor_named_success`
- `choice_payload_constructor_positional_success`

For each:

- [x] Prefer observing the constructed variant and payload through pattern matching and a contract-specific output marker.
- [x] If the case genuinely owns syntax acceptance only, remove unrelated runtime output and mark the whole case `role = "smoke"`.
- [x] Do not keep runtime output while declaring acceptance-only.
- [x] Keep imported-constructor and local-constructor contracts distinct only where visibility/binding is the reason.

### `struct_using_constant`

- [x] Construct the struct.
- [x] Omit the defaulted field so the constant-backed default is exercised.
- [x] Render the resulting default value.
- [x] Assert `Hello World!` through the correct static artifact or runtime lane.
- [x] Remove the unrelated `struct_using_constant` marker contract.

### `html_wasm_bool_conditional`

- [x] Use distinct branch markers such as `bool-branch-yes` and `bool-branch-no`.
- [x] Require the true marker.
- [x] Forbid the false marker.
- [x] Preserve current backend outcomes.

### Ambiguous runtime substrings

- [x] Review all Phase 2 migrated `rendered_output_contains` values that are numeric-only, very short, or repeated generic words.
- [x] At minimum correct:
  - `borrow_checker_alias_not_live_after_scope`
  - `adversarial_nested_catch_handlers`
  - `adversarial_struct_collection_result_interop`
  - `multi_bind_explicit_types_and_mutability`
  - the `choice_payload_match_*` cases that assert only `bad` or an unlabeled scalar
- [x] Replace ambiguous fragments with context-rich markers emitted by the fixture, for example `first=10 second=20`.
- [x] Do not introduce artificial console output into static page contracts.
- [x] Keep Phase 5 exact/ordered runtime work; unique markers are not a substitute for exact events.

### Config wording

- [x] Rename the `config_current_keys_success` marker from `Backward Compatibility Test` to current-behaviour wording such as `Current Config Keys`.
- [x] Update its artifact assertion atomically.
- [x] Do not preserve legacy vocabulary as a contractual artifact.

### Role audit

- [x] Every whole-case acceptance-only fixture is `role = "smoke"`.
- [x] Mixed-backend parity cases use `backend` or `boundary` where that better describes the primary purpose.
- [x] Do not assign a primary contract merely to silence an advisory.

### Acceptance

- [x] No recently migrated behavior-visible case is mislabeled acceptance-only.
- [x] Named contracts are actually exercised.
- [x] Short fragments cannot pass through accidental substring overlap in the reviewed set.
- [x] Focused exact-case runs, audit, and the full gate pass.

---

## 2R6 — Audit the loop-carried borrow fix

The accepted fix uses CFG future-use facts and is in the correct compiler stage, but its naming and edge coverage must match its actual scope.

### Required implementation review

- [x] Rename `has_loop_carried_future_use` to a CFG-accurate name such as `has_cfg_future_use_after_linear_expiry` unless the implementation is narrowed to verified backedges.
- [x] Update adjacent comments and helper names to describe CFG-carried future use rather than loops only.
- [x] Review projected assignment targets that currently provide no actor identity.
- [x] Prefer passing `place_root_local_index(layout, target)` for user-local field/index writes.
- [x] If any projected target must intentionally have no actor identity, encode that distinction explicitly and test it.
- [x] Preserve compiler-temporary linear-expiry behavior.
- [x] Preserve the invariant that borrow validation reads HIR/future-use facts without mutating HIR.

### Focused unit coverage

- [x] Mutating the active iterable fails with `SharedMutableConflict`.
- [x] Mutating the iterable through a mutable helper call inside the loop fails.
- [x] Mutating through an alias of the iterable fails.
- [x] Nested collection loops preserve both active iterable aliases.
- [x] A branch/join case proves the intended CFG behavior after linear expiry.
- [x] A compiler-temporary projected target retains the intended linear-expiry behavior.
- [x] A user-local field/index projected target retains source-semantic conflict behavior.
- [x] Mutation after loop exit succeeds.

### Integration ownership

- [x] Keep `loop_borrow_mutation_conflict` as the primary negative user-visible owner.
- [x] Add or strengthen one positive integration case covering unrelated-root and copied-root independence with labeled observable output.
- [x] Retain focused units only for transfer/origin/CFG facts that integration output cannot expose.

### Documentation/status

- [x] Describe the result as corrected rejection coverage, not a new language feature.
- [x] Update the progress matrix only if its current borrow/loop coverage wording was inaccurate.

### Acceptance

- [x] Helper names match the implemented invariant.
- [x] Source locals and compiler temporaries are distinguished deliberately.
- [x] The edge matrix passes and no unrelated valid program regresses.
- [x] The full gate passes in a separate semantic-fix slice.

---

## 2R7 — Correct misleading lifetime fixture ownership

Beanstalk has no source-level lifetime system. Integration IDs and comments must describe source-visible alias, final-use, access, or ownership-transfer behavior.

### Required review

- [x] Audit every canonical case with the `lifetime_inference_` prefix.
- [x] Do not retain a canonical integration case whose only claimed contract is a hidden drop site, move decision, or diagnostic precision fact.
- [x] Rename retained user-visible cases and paths to source-semantic terminology.
- [x] Update manifest IDs, paths, tags, expectations, and references atomically.

### Accepted audit disposition

| Current case | Disposition and owner |
|---|---|
| `lifetime_inference_conflict_detection` | Rename to `live_shared_alias_blocks_mutable_rebinding`; retain `BST-BORROW-0007` as the unique source-visible owner. |
| `lifetime_inference_control_flow` | Remove after strengthening `branch_reborrow_after_last_use` with labeled branch reborrow output. |
| `lifetime_inference_drop_insertion` | Remove; focused advisory return/break/block-exit drop-site tests own the hidden facts. |
| `lifetime_inference_error_precision` | Remove; Phase 4 owns the missing structured negative path/line assertion. |
| `lifetime_inference_integration_basic` | Remove; `borrow_checker_basic_variables` plus the pipeline report-storage sentinel cover its actual behavior. |
| `lifetime_inference_move_refinement` | Remove only after Phase 2R7b adds the focused inferred-assignment-move state owner. |
| `lifetime_inference_use_after_move` | Remove as a duplicate of the existing case renamed to `mutable_alias_blocks_later_source_access`; retain `BST-BORROW-0003`. |
| `last_use_precision` | Remove after strengthening `borrow_conflict_resolved_by_reordering` with labeled final-use output. |

### Mandatory dispositions

- [x] `lifetime_inference_drop_insertion`: verify the focused drop-site fact owner; remove the canonical case when replacement evidence exists.
- [x] `lifetime_inference_error_precision`: remove the misleading success case; add a real structured location owner in Phase 4 if that gap remains.
- [x] `lifetime_inference_move_refinement`: verify the focused move-decision owner; retain or add an integration case only for an observable final-use transfer consequence, under a source-semantic name.
- [x] Review `lifetime_inference_control_flow` and other remaining prefixed cases for the same naming/ownership drift.
- [x] Record every removal or rename in the pruning ledger.

### Acceptance

- [x] No integration case claims to observe an internal lifetime/drop/move fact that it cannot inspect.
- [x] Hidden facts remain covered by focused units.
- [x] User-visible access behavior remains covered end to end.
- [x] Focused tests, exact cases, audit, and the full gate pass.

---

## 2R8 — Correct workflow documentation and re-anchor

### Documentation corrections

- [x] Update `testing.bd` with acceptance-only semantics, universal backend baselines, truthful audit fields, golden-file rules, contained paths, and hard-policy behavior.
- [x] Replace the nonexistent concrete contract example in `testing.bd` and `CONTRIBUTING.md` with `--contract <contract-id>` unless a real classified contract is added in the same slice.
- [x] Update canonical expectation examples from `compile_only` to `acceptance_only`.
- [x] Correct any claim that fixture outcomes were unchanged: supported feature breadth stayed the same, assertion coverage improved, and one missed invalid loop mutation is now rejected.
- [x] Update this plan's active checkpoint and current counts from a fresh audit.
- [x] Review the progress matrix for strengthened coverage wording only.
- [x] Rebuild documentation from source.

### Phase 2R acceptance

- [x] Success intent is truthful and the old spelling is deleted.
- [x] Golden presence cannot be faked by directories.
- [x] One suite-policy evaluator owns hard/advisory rules.
- [x] Audit writes useful reports and fails on hard findings.
- [x] Fixture and entry paths are contained.
- [x] Recently migrated weak or inaccurate fixtures are corrected.
- [x] Loop-borrow edge coverage is complete for the touched behavior.
- [x] Misleading lifetime fixtures are removed, renamed, or reassigned.
- [x] Workflow documentation is current.
- [x] `cargo fmt`, `git diff --check`, documentation build, and `just validate` pass.
- [x] The checkpoint log records the accepted Phase 2R commits without narrative bloat.

---

# Phase 2 — Finish explicit success contracts

Phase 2 resumes only after Phase 2R closes.

## 2D — Complete remaining baseline-only migrations

Regenerate the audit after 2R. Do not rely on the stale count of 35 or the old preclassification list.

For every remaining baseline-only success backend:

- [x] determine the user-visible, artifact, warning, absence, backend-parity, or acceptance-only contract
- [x] use rendered output for executed behavior
- [x] use artifact assertions for static/folded output or target structure
- [x] use goldens only where exact emitted text is contractual
- [x] use absence assertions for explicit non-emission contracts
- [x] use acceptance-only only when no stronger observable contract exists
- [x] classify whole-case acceptance-only fixtures as smoke
- [x] preserve backend-specific success or structured rejection rather than forcing symmetry
- [x] use context-rich markers until Phase 5 exact output is available
- [x] validate one semantic family per slice

### 2D1 — Trait, ordering, package-alias, and config-package batch

Formerly queued as 2B11d:

- [x] `trait_incompatibility_parse_success`: assert HTML runtime output; preserve the existing HTML-Wasm structured failure.
- [x] `trait_relation_facade_private_private_success`: assert the HTML static artifact; use HTML-Wasm acceptance-only parity because its current artifact does not expose the HTML marker.
- [x] `entry_start_sees_sorted_declarations`: assert labeled runtime output.
- [x] `two_package_symbols_same_name_aliases`: use acceptance-only only after confirming there is no meaningful observable result; classify the whole case as smoke if appropriate.
- [x] `config_package_folder_missing_default_ignored`: assert labeled runtime output.
- [x] Preserve warning policy, source behavior, and backend outcomes.

### 2D2 — Isolated design conflict

- [x] Review `choice_import_visibility_non_exported` against current private cross-module import rules and the progress matrix.
- [x] Confirm the fixture is not cross-module: `#page.bst` and `choices.bst` belong to one module, whose files may import private declarations directly.
- [x] Rename or otherwise correct misleading cross-module terminology while preserving same-module success.
- [x] Add a meaningful observable construction/import contract rather than acceptance-only intent.
- [x] Remove the stale canonical-module handoff after the corrected fixture is accepted.

### 2D3 — Remaining families

Process the fresh audit list in this order:

1. traits and generic-bound acceptance
2. package, facade, import, and config behavior
3. HTML/HTML-Wasm parity and target artifacts
4. constants, records, templates, Beandown, and Markdown parity
5. expected-warning cases
6. remaining intentional smoke cases

After each family:

- [x] run exact selected cases
- [x] run focused harness tests
- [x] run audit and record new counts
- [x] run the full gate

## 2E — Enforce authored success completeness

- [x] Remove the temporary backend-baseline exception from fixture completeness.
- [x] A success backend must have at least one of:
  - acceptance-only intent
  - rendered-output assertion
  - artifact assertion
  - non-empty golden file set
  - artifact-absence assertion
  - non-default expected-warning contract
- [x] The universal backend baseline alone is insufficient.
- [x] Add self-tests for every accepted success-contract form.
- [x] Add a self-test proving a baseline-only success backend is rejected.
- [x] Add a self-test proving default `warnings = "forbid"` does not count as the authored contract.
- [x] Audit must report zero baseline-only backends.
- [x] Remove temporary baseline-only finding terminology that no longer has a valid canonical state.

## Phase 2 acceptance

- [x] Every manifest case has a contained case-owned `expect.toml`.
- [x] No fallback infrastructure or old success spelling remains.
- [x] Every success backend has an authored case-specific contract or acceptance-only intent.
- [x] No behavior-visible case is weakened to acceptance-only.
- [x] Acceptance-only and baseline execution are reported separately.
- [x] Audit hard findings are zero.
- [x] Workflow docs and fixture examples are current.
- [x] Full validation passes.

---

# Phase 3 — Split assertion ownership and enforce exact diagnostic/warning multiplicity

## Goal

Give each assertion family one module and make unexpected diagnostics or warnings fail by default.

## 3A — Complete assertion-module ownership

Target shape, adjusted for any `goldens.rs` work completed in Phase 2R:

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

- [x] Keep orchestration in `mod.rs`.
- [x] Move code; do not copy it.
- [x] Delete the monolithic path after migration.
- [x] Keep helper visibility narrow.
- [x] Keep Node execution with rendered output.
- [x] Keep Wasm parsing/validation with Wasm assertions.
- [x] Keep golden discovery/comparison with goldens.

## 3B — Exact diagnostic code multisets

- [x] Add typed `DiagnosticMatchMode::{Exact, Contains}`.
- [x] Make exact the canonical default.
- [x] Compare unordered code multisets including duplicate counts.
- [x] Report missing, unexpected, and count-mismatched codes separately.
- [x] Require an authored reason for contains mode.
- [x] Reject a contains reason in exact mode.
- [x] Preserve `message_contains` only as an additional rendering contract.
- [x] Add self-tests for exact success, unexpected extra, duplicate mismatch, and justified contains.

## 3C — Warning identity

- [x] Preserve `forbid` and `ignore`.
- [x] Replace count-only exact warnings with exact warning-code multisets.
- [x] Require count consistency when a transitional count field still exists.
- [x] Report missing and unexpected warning codes.
- [x] Cover successful and failed compilation results.
- [x] Delete obsolete count-only schema after canonical migration.

## 3D — Canonical migration

Migrate one semantic family per slice:

1. borrow and access
2. syntax/tokenization
3. types, collections, and maps
4. functions, results/options, and calls
5. generics and traits
6. imports, modules, and config
7. backend target validation
8. templates/TIR diagnostics

- [x] Inventory actual diagnostic and warning multisets before editing expectations.
- [x] Use contains mode only for intentional independent cascades/recovery.
- [x] Update suite policy so unjustified contains mode is hard-failing.

## Phase 3 acceptance

- [x] Assertion modules have one responsibility each.
- [x] Unexpected diagnostics and warnings fail.
- [x] Contains mode is rare, justified, and reported.
- [x] No warning-count compatibility path remains.
- [x] `testing.bd`, examples, and `index.md` are current.
- [x] Full validation passes.

---

# Phase 4 — Add compiler-owned diagnostic reason and source-location assertions

## Goal

Distinguish broad-code reasons and protect source remapping without parsing prose or duplicating compiler payload semantics in the runner.

## 4A — Compiler-owned reason identity

- [x] Add one crate-private structured identity path beside typed diagnostic payload/reason definitions.
- [x] Expose code, severity, and optional stable reason key.
- [x] Use qualified keys such as `invalid_collection_type.zero_capacity`.
- [x] Keep keys independent of rendered wording and `Debug` output.
- [x] Add representative units and uniqueness coverage.
- [x] Document stability expectations.

## 4B — Structured expectation tables

- [x] Add `diagnostic_assertions` matched by code plus deterministic occurrence.
- [x] Support optional reason, normalized path, line, count, and narrow secondary-label assertions.
- [x] Support column only where token-span precision is the contract.
- [x] Reject assertions for codes absent from `diagnostic_codes`.
- [x] Reject ambiguous occurrence selection.
- [x] Produce actionable mismatch reports without full snapshots.

## 4C — High-risk migration

Prioritize:

- fixed collection capacity reasons
- call-shape and mutable-access reasons
- trait-name misuse
- import path/visibility reasons
- fallible handling reasons
- backend unsupported-feature reasons
- imported/generated source remapping
- declaration/conflict secondary labels

Add the real diagnostic-location owner deferred from `lifetime_inference_error_precision` here, under source-semantic naming.

## Phase 4 acceptance

- [x] Reason identity has one compiler owner.
- [x] Runner contains no diagnostic payload taxonomy.
- [x] Broad-code clusters assert reasons where needed.
- [x] Paths and lines are assertable across file boundaries.
- [x] Message fragments remain rare and purposeful.
- [x] Documentation and progress coverage are current.
- [x] Full validation passes.

---

# Phase 5 — Preserve ordered runtime events and audit goldens

## Goal

Extend the existing Node harness to protect chronology, exact output, and exact-once behavior without adding another executor.

## 5A — Ordered event model

- [x] Define typed console and fragment-insert events.
- [x] Record one chronological event array at event time.
- [x] Derive channel-specific views from that array.
- [x] Preserve one documented microtask flush policy.
- [x] Preserve temporary-file cleanup and retry behavior.
- [x] Do not invoke Node for cases without runtime assertions.
- [x] Add units for extraction, decoding, ordering, and infrastructure failures.

## 5B — Stronger runtime fields

- [x] Add exact output.
- [x] Add ordered fragments.
- [x] Add exact-once fragments.
- [x] Retain contains/not-contains.
- [x] Validate empty or incompatible combinations.
- [x] Normalize line endings only; do not broadly collapse whitespace in exact mode.
- [x] Give exact/ordered mismatches distinct triage kinds.

## 5C — Order-sensitive migrations

At minimum protect:

- active root exactly once
- imported root suppression
- compile-time/runtime fragment order
- loop iteration order
- output before `break`/`continue`
- map insertion/replacement/removal order
- reactive mount exactly once
- runtime helper output not duplicated

## 5D — Golden audit

- [x] Inventory every current golden using the Phase 2R file inventory.
- [x] Keep exact or normalized goldens only where generated structure itself is contractual.
- [x] Convert behavior-only goldens to runtime assertions.
- [x] Convert structural contracts to narrow artifact assertions.
- [x] Delete unused files and empty directories.
- [x] Record conversions/removals in the pruning ledger.

### Phase 5D1 accepted audit disposition

The schema-6 inventory found 38 normalized HTML golden blocks. Every block is a 483–698 line
whole-page snapshot, for 20,493 golden lines total. Each snapshot mixes global document styling,
runtime glue, and case-local behavior. No current case requires that complete generated page shape,
so Phase 5D retains no whole-page golden.

| Conversion batch | Cases | Replacement owner |
|---|---|---|
| 5D2 runtime values | `function_call_named_args_mutable_success`, `collection_ordered_runtime_operations`, `comparison_and_logical`, `function_call_named_and_default_args_success`, `functions`, `structs_and_collections`, `char_basic`, `char_equality`, `char_ordering`, `char_in_template`, `struct_constructor_all_defaults`, `struct_nested_field_mutation_success`, `short_circuit_place_rhs_later_plain_use` | Exact runtime output over existing context-rich markers. |
| 5D3 receiver behavior | `struct_chained_immutable_receiver_method_call`, `receiver_method_exported_cross_file_success`, `receiver_method_mutable_struct_success`, `receiver_method_nested_struct_success`, `receiver_method_chained_mutable_and_field_access`, `receiver_method_field_write_then_mutable_call`, `receiver_method_alias_return_success`, `receiver_method_exported_cross_file_mutable_success`, `receiver_method_aliasing_borrow_after_mutable_call` | Exact runtime output; add context-rich fixture markers where the current console value is ambiguous. |
| 5D4 template/import runtime behavior | `imported_start_function_callable_not_auto_run`, `relative_import_dot_segments`, `template_output_test`, `template_slots_const_folding`, `import_path_separator_normalized_diagnostic`, `template_positional_named_mixed`, `template_positional_slot_children_body`, `template_style_css_success`, `template_style_markdown_success` | Exact or ordered runtime output, plus narrow static artifact/absence assertions where a mixed case owns both lanes. |
| 5D5 static artifact and absence | `basic_page_output`, `html_tracked_asset_unreferenced_not_emitted`, `top_level_const_template`, `top_level_const_template_multiple_ordered`, `top_level_const_template_single_item` | Narrow HTML contains/order and unreferenced-asset absence assertions. |

## Phase 5 acceptance

- [x] One Node harness remains.
- [x] Runtime chronology is captured, not reconstructed.
- [x] Exact-once protects activation/mount contracts.
- [x] Exact mode is not over-normalized.
- [x] Every retained golden has an explicit owner.
- [x] Documentation is current.
- [x] Full validation passes.

---

# Phase 6 — Add primary integration coverage for hashmap access and ownership semantics

## Goal

Add end-to-end owners before pruning source-shaped borrow/map units.

## Required coverage

### Live lookup alias conflicts

- [x] `get` result live across `set` fails.
- [x] Add separate `remove` and `clear` failures only when transfer path or reason differs.
- [x] Assert exact code, reason, mutation location, and earlier shared-access label.

### Alias final use and copy independence

- [x] Mutation after final alias use succeeds and output is observed.
- [x] An explicit copy remains usable after mutation; assert copy and final map state.
- [x] Keep alias-final-use and copy-independence contracts distinct.

### Removed-value ownership and key behavior

- [x] Removed value remains usable after later map mutation/clear.
- [x] Lookup keys are borrowed, not consumed.
- [x] Inserted non-copy keys/values follow current consumption rules.
- [x] Explicit-copy success exists where independence is required.
- [ ] Reuse the existing runtime map ordering case rather than duplicate it.

### Backend matrix

- [x] Use one input with HTML success and HTML-Wasm structured rejection where map reachability matters.
- [x] Do not require unsupported Wasm success.

## Phase 6 acceptance

- [x] Map access/ownership contracts have primary integration owners.
- [x] Negative cases fail for the intended reason.
- [x] Positive cases observe exact runtime values.
- [x] No unit has been deleted yet.
- [x] No deferred map feature was introduced.
- [x] Full validation passes.

---

# Phase 7 — Prune and narrow borrow-checker source-shaped units

## Goal

Remove unit tests that only re-prove user-visible language behavior after Phase 6 owners exist, while retaining hidden transfer, state, summary, malformed-HIR, drop, and invalidation facts.

## Checklist

- [x] Review every test in `borrow_checker_map_tests.rs` against Phase 6 contracts.
- [x] Delete fully replaced source-only acceptance/rejection tests.
- [x] Retain narrow access-root/classification units where output cannot expose the fact.
- [x] Review branch/match/loop scope units; retain snapshot/merge or malformed-HIR invariants only.
- [x] Keep minimal pipeline failure-propagation and report-storage sentinels.
- [x] Preserve call summaries, value/statement/terminator facts, drop sites, liveness, reactive invalidation, and malformed-HIR rejection.
- [x] Delete helpers used only by removed tests.
- [x] Rename remaining tests after the stage invariant.
- [x] Record every removal and replacement owner.

## Phase 7 acceptance

- [x] No user-visible borrow behavior is unit-only.
- [x] Hidden borrow invariants remain strong.
- [x] Pipeline coverage is minimal and boundary-focused.
- [x] No production API survives only for deleted tests.
- [x] `index.md` and progress coverage are current where ownership moved.
- [x] Full validation passes.

---

# Phase 8 — Reassign function-call, type-resolution, and generic ownership

## Goal

Preserve parser shape, source locations, canonical type identity, binding, substitution, and unification; move whole-program behavior to integration.

## Function-call ownership

Keep units for parsed shape, source locations, access classification, parser conversion, and hidden binding/order algorithms.

Ensure integration ownership for:

- positional-after-named
- duplicate/unknown named target
- missing required argument
- missing `~`, immutable `~`, and non-place `~`
- fresh template/collection/struct/computed mutable arguments
- named mutable success

Then remove whole-source units that only compile or fail.

## Type-resolution ownership

Keep units for canonical `TypeId`, collection/map shapes, alias transparency, imported canonical types, field/variant registration, and invalid internal parsed-type handling.

Ensure integration ownership for invalid capacities across signatures/fields/aliases/returns, cross-file capacity resolution, and current backend rejection.

## Generic ownership

Keep units for parameter identity, binding consistency, argument order, type keys, substitution/unification, generated identity, and impossible states.

Ensure integration ownership for duplicate/invalid/colliding/unused parameters, inference ambiguity/conflict, cross-file/facade visibility, and trait-bound evidence/privacy.

## Phase 8 acceptance

- [x] Parser units stop at parser facts.
- [x] Type units stop at semantic identity.
- [x] Generic units stop at identity/binding/substitution.
- [x] User behavior is integration-owned with exact output/diagnostics.
- [x] No renderer wording is pinned in datatype units.
- [x] No test-only production API remains.
- [x] Full validation passes.

---

# Phase 9 — Add cross-module effect-summary and diagnostic-remapping coverage

## Goal

Prove public access, mutation, consumption, return-alias, and source-remapping facts across currently supported module boundaries while retaining focused summary units.

## Required coverage

- [x] Exported return alias blocks caller mutation while live; succeeds after final use.
- [x] Exported fresh return permits caller mutation.
- [x] Cross-module mutable parameter succeeds with `~` and rejects missing `~` at the consumer.
- [x] Facade/re-export preserves access/effect semantics and origin identity.
- [x] Generated generic instance coverage is added only where current support permits.
- [x] Provider, consumer call-site, facade, generated call-site, and cross-file borrow labels use normalized paths/lines.

## Deferred architecture policy

When current implementation does not support the accepted canonical-module end state:

- check the progress matrix
- assert current structured rejection
- record the future success owner in the canonical module plan
- do not implement deferred module architecture here

## Phase 9 acceptance

- [x] Current cross-module effect behavior has integration owners.
- [x] Unit summary facts remain.
- [x] Source remapping is structured.
- [x] Facades do not bypass visibility.
- [x] Deferred architecture remains deferred.
- [x] Full validation passes.

---

# Phase 10 — Strengthen `check`/`build` parity and root/fragment activation

## `check`/build frontend parity

- [x] Run one reusable fixture through direct `check` and build frontend seams.
- [x] Compare exact frontend diagnostic codes, reason keys, paths, and lines.
- [x] Compare warning identity.
- [x] Prove `check` writes no backend artifacts.
- [x] Keep command formatting tests separate from semantic parity.
- [x] Add target-planning parity only for current implementation; otherwise record a handoff.

## Root and fragment activation

- [x] Active root executes exactly once.
- [x] Imported root runtime never executes while public APIs remain usable.
- [x] API-only roots emit no route/artifact where currently supported.
- [x] Compile-time/runtime fragments preserve source-defined order.
- [x] Output before loop control is preserved.
- [x] Hydration/mount is not duplicated.

## Phase 10 acceptance

- [x] `check` uses the shared frontend contract and remains a no-artifact overlay.
- [x] Root activation and fragment order are externally observed with exact/ordered assertions.
- [x] Build-system units remain for hidden graph/output policy.
- [x] Progress and command documentation are current.
- [x] Full validation passes.

---

# Phase 11 — Add runtime integration ownership for reactivity after subscription

## Required coverage

- [x] Mount a template subscribed to a reactive source.
- [x] Mutate the source after subscription; prove mutation is accepted and output updates.
- [x] Assert initial output, update order, one mount, and defined rerender count.
- [x] Do not make incidental microtask counts contractual.
- [x] Reject reactive-parameter mutation permission with exact reason/location.
- [x] Retain hidden invalidation/source-identity units.
- [x] Add map/place invalidation runtime coverage only where current progress says it works.
- [x] Use HTML-JS success and HTML-Wasm structured rejection in one matrix.
- [x] Do not introduce deferred field/path subscriptions or broader reactivity.

## Phase 11 acceptance

- [x] Runtime semantics and hidden invalidation facts each have one owner.
- [x] Subscription is not modeled as an active borrow.
- [x] Ordered assertions avoid scheduler internals.
- [x] No TIR identity or view leaks into runtime expectations.
- [x] Progress wording is current.
- [x] Full validation passes.

---

# Phase 12 — Consolidate redundant positive integration scenarios

## Candidate clusters

- fresh mutable argument forms
- choice equality truth table
- named/default argument success
- collection/map ordered runtime operations

## Rules for every consolidation

- [x] Identify one exact shared semantic contract.
- [x] Choose the strongest existing primary case.
- [x] Add distinct semantic output markers and exact/ordered assertions.
- [x] Run the combined case before deletion.
- [x] Keep negative reason cases separate.
- [x] Delete superseded folders, expectations, goldens, and manifest entries atomically.
- [x] Update tags/contracts/roles.
- [x] Record every deletion.
- [x] Confirm failure localization remains acceptable.
- [x] Do not create cross-feature mega-fixtures.

## Phase 12 acceptance

- [x] Positive coverage is denser without becoming ambiguous.
- [x] Negative diagnostics remain isolated.
- [x] No stale files or manifest entries remain.
- [x] No semantic coverage was lost.
- [x] Full validation passes.

---

# Phase 13 — Normalize backend units around lowering and ABI contracts

## Keep backend owners for

- helper selection
- ABI/import/export shape
- operation-to-target mapping
- stable carrier representation
- target planning not observable from artifacts
- malformed-HIR/backend invariant handling

## Checklist

- [x] Classify every backend test as ABI, helper, mapping, planning, malformed HIR, artifact, or semantic substitute.
- [x] Locate the integration runtime owner for every semantic substitute.
- [x] Keep stable JavaScript helper/ABI and deliberate carrier contracts.
- [x] Remove source-text assertions replaced by execution.
- [x] Replace incidental generated names/indexes with semantic fragments or structured facts.
- [x] Keep Wasm binary validation, imports/exports, LIR/emission invariants, and structured unsupported-feature validation.
- [x] Move project-assembly artifact contracts to integration.
- [x] Keep backend-local facts that final artifacts cannot expose.
- [x] Do not broaden Wasm support.
- [x] Record every deletion/replacement.

## Phase 13 acceptance

- [x] Language semantics are integration-owned.
- [x] Target representation contracts remain.
- [x] No backend reparses source or reconstructs semantics.
- [x] Exact generated names remain only where ABI requires them.
- [x] Malformed-HIR coverage remains.
- [x] Backend documentation/indexes are current.
- [x] Full validation passes.

---

# Phase 14 — Backfill ownership, enforce final policy, run mutation probes, and close

## Contract and role classification

- [x] Backfill `contract` and `role` for every canonical non-harness case, with contractless whole-case smoke as the explicit exception.
- [x] Classify adversarial cases explicitly.
- [x] Classify whole-case acceptance-only cases as smoke.
- [x] Reject duplicate primary contracts and primary-without-contract through the single policy owner.
- [x] Report contracts with no primary owner.
- [x] Review every boundary/backend secondary owner.
- [x] Normalize tag spelling and ordering; remove obsolete tags.

## Final audit hard failures

- missing `expect.toml`
- baseline-only success
- duplicate case ID/path
- unsafe or noncanonical fixture/entry path
- empty/unknown role or invalid metadata
- duplicate primary contract
- unjustified diagnostic contains mode
- failure without diagnostic codes
- expected warnings without warning identity
- invalid backend expectation shape
- undeclared fixture folder or stale manifest path
- inconsistent golden presence/mode

Resolve or explicitly document advisories for:

- strict golden without exact-artifact rationale
- message fragment without wording/label rationale
- duplicate expectation fingerprints
- acceptance-only case with observable output
- unclassified source-shaped unit
- contract with excessive secondary owners

## Put policy in the normal gate

- [x] Keep `bean tests --audit` as the JSON report command.
- [x] Add the fast hard-policy check to `just validate` before full integration execution, or make canonical suite loading enforce the same evaluator.
- [x] Update `validation.bd`.
- [x] Keep reports under `target/` and never mutate tracked fixtures.

## Unit ownership final pass

- [ ] Re-run the source-shaped-unit inventory.
- [ ] Record why every remaining full-source unit exists.
- [ ] Delete stale test-only helpers and production APIs.
- [ ] Preserve final TIR exact-view/preparation/handoff owners without reopening architecture.
- [ ] Confirm build-system units are policy-focused and HIR units assert semantic relationships.

## Targeted mutation probes

Use a temporary branch/worktree and never commit deliberate faults. Probe at least:

- mutable call-site access classification
- map `get` alias creation
- return-alias summary propagation
- fixed-capacity identity
- match exhaustiveness
- checked numeric failure mode
- imported-root suppression
- target reachability for unsupported features
- reactive invalidation

For each probe:

- [ ] introduce one semantic defect
- [ ] run the expected primary filtered test
- [ ] confirm failure for the intended contract
- [ ] revert the defect
- [ ] record the primary owner and result
- [ ] do not claim a general mutation score

A permanent mutation-testing dependency requires separate approval.

## Final measurements

Record under the same method as Phase 0:

- Rust test count by major owner
- canonical case and backend-execution counts
- diagnostic exact/contains counts
- acceptance-only count
- strict/normalized golden counts
- role counts
- median unit and integration wall times
- added, removed, merged, and strengthened cases
- units removed and retained by invariant category

Do not present lower counts or faster time as proof of correctness.

## Documentation and roadmap closure

- [ ] Finalize `testing.bd`, `validation.bd`, and `CONTRIBUTING.md`.
- [ ] Update `index.md` for moved/removed test modules.
- [ ] Update progress-matrix coverage statements.
- [ ] Update compiler diagnostic identity wording only if Phase 4 made reason keys durable.
- [ ] Rebuild generated documentation.
- [ ] Mark this plan complete and update `docs/roadmap/roadmap.md`.
- [ ] Refresh the canonical module plan against the final harness and test owners.
- [ ] Set the next active plan and checkpoint.

## Phase 14 acceptance

- [ ] Audit hard findings are zero.
- [ ] Every canonical case has explicit ownership.
- [ ] Every removed test has replacement evidence.
- [ ] Representative mutations are caught by primary owners.
- [ ] Documentation and roadmap are current.
- [ ] Full validation passes.
- [ ] Canonical module work can begin against the hardened suite.

---

## Persistent ledgers

### Pruning ledger

| Removed or renamed test/case | Previous intended contract | Replacement primary owner | Retained secondary owner | Commit |
|---|---|---|---|---|
| `lifetime_inference_conflict_detection` → `live_shared_alias_blocks_mutable_rebinding` | Live shared alias prevents mutation-capable rebinding | `live_shared_alias_blocks_mutable_rebinding` | none | `5fbf183a4` |
| `lifetime_inference_control_flow` | Branch-local alias final use permits mutation-capable rebinding | `branch_reborrow_after_last_use` | none | `5fbf183a4` |
| `lifetime_inference_drop_insertion` | Hidden advisory drop-site placement | `borrow_checker_drop_site_tests::{emits_advisory_return_drop_sites, emits_advisory_break_and_region_exit_drop_sites}` | none | `5fbf183a4` |
| `lifetime_inference_error_precision` | Claimed precise diagnostics without a negative or location assertion | none; Phase 4 owns the real structured path/line gap | none | `5fbf183a4` |
| `lifetime_inference_integration_basic` | Basic end-to-end borrow acceptance and pipeline report storage | `borrow_checker_basic_variables` | `borrow_checker_pipeline_tests::successful_borrow_report_can_be_stored_on_module` | `5fbf183a4` |
| `lifetime_inference_move_refinement` | Hidden inferred-assignment move transition | `borrow_checker_fact_tests::statement_entry_state_marks_source_uninitialized_after_inferred_assignment_move` | none | `5fbf183a4` |
| `lifetime_inference_use_after_move` | Mutable alias blocks later source access | `mutable_alias_blocks_later_source_access` | none | `5fbf183a4` |
| `last_use_precision` | Alias final use permits mutation-capable rebinding | `borrow_conflict_resolved_by_reordering` | none | `5fbf183a4` |
| `borrow_checker_use_after_move` → `mutable_alias_blocks_later_source_access` | Mutable alias blocks later source access | `mutable_alias_blocks_later_source_access` | none | `5fbf183a4` |
| `choice_import_visibility_non_exported` → `choice_same_module_private_import_success` | Same-module import and construction of a private choice declaration | `choice_same_module_private_import_success` | none | `4e0b4ca7b` |
| `borrow_checker_map_tests::map_get_alias_blocks_set_when_used_after_mutation` | Live `get` alias blocks mutation | `map_get_alias_blocks_set_when_used_after_mutation` | `borrow_checker_fact_tests::map_get_operation_result_alias_retains_receiver_root` | `f6c514180` |
| `borrow_checker_map_tests::map_get_alias_allows_set_when_used_before_mutation` | Final alias use permits mutation | `map_alias_final_use_allows_mutation` | none | `f6c514180` |
| `borrow_checker_map_tests::{map_get_alias_blocks_remove_when_used_after_mutation,map_get_alias_blocks_clear_when_used_after_mutation}` | Live `get` alias blocks equivalent map mutations | `map_get_alias_blocks_set_when_used_after_mutation` | none; shared mutable-receiver conflict path | `f6c514180` |
| `borrow_checker_map_tests::map_contains_and_length_do_not_create_long_lived_aliases` | Shared map queries do not retain mutation-blocking aliases | `map_borrowed_lookup_key` and `map_alias_final_use_allows_mutation` | none | `f6c514180` |
| `borrow_checker_map_tests::map_remove_result_is_owned_and_can_outlive_mutation` | Removed value owns independent state | `map_remove_returns_owned_value` | `borrow_checker_fact_tests::map_remove_result_is_fresh_owned` | `f6c514180` |
| `borrow_checker_map_tests::map_set_consumes_inserted_non_copy_values` | Last-use-sensitive `set` ownership transfer | `map_set_immutable_non_copy_value_later_use_rejected` and `map_explicit_copy_insertion_independent` | `borrow_checker_fact_tests::{map_set_final_use_moves_inserted_non_copy_roots,map_set_later_use_keeps_mutable_inputs_borrowed}` | `f6c514180` |
| `borrow_checker_map_tests::{map_literal_consumes_inserted_non_copy_values,map_literal_consumes_inserted_non_copy_keys}` | Map literal owns inserted key/value roots | `map_literal_consumes_inserted_non_copy_value` and `map_literal_consumes_inserted_non_copy_key` | none | `f6c514180` |
| `borrow_checker_map_tests::nested_map_literal_consumes_inner_inserted_values` | Recursive aggregate ownership transfer | none; hidden state fact | `borrow_checker_fact_tests::nested_map_literal_moves_inner_non_copy_value_root` | `f6c514180` |
| `borrow_checker_map_tests::map_literal_allows_copy_values` | Copyable values remain usable after literal construction | `map_explicit_copy_insertion_independent` plus scalar map runtime coverage | none | `f6c514180` |
| `borrow_checker_map_tests::{map_get_key_is_not_consumed,map_contains_key_is_not_consumed,map_remove_key_is_not_consumed}` | Lookup keys are borrowed | `map_borrowed_lookup_key` | none | `f6c514180` |
| `borrow_checker_map_tests::map_set_after_nothing_passes` | Baseline map mutation success | `hashmap_js_runtime_contract` | none | `f6c514180` |
| `function_call_tests::{rejects_positional_after_named,rejects_duplicate_named_target,rejects_unknown_named_parameter,rejects_missing_required_parameter}` | User-visible named/positional call routing failures | `function_call_named_args_{positional_after_named,duplicate,unknown,missing_required}` | Raw positional/named parser-shape units remain | `720c23e9a` |
| `function_call_tests::{rejects_missing_tilde_for_mutable_positional_parameter,rejects_missing_tilde_for_mutable_named_parameter}` | Existing mutable places require authored `~` | `function_call_mutable_param_requires_explicit_tilde` and `function_call_named_args_mutable_missing_tilde` | Authored-marker/value-location units remain | `720c23e9a` |
| `function_call_tests::{accepts_fresh_rvalue_for_mutable_positional_parameter,accepts_fresh_rvalue_for_mutable_named_parameter}` | Fresh computed values satisfy mutable parameters without `~` | `function_call_mutable_param_fresh_computed_arg` | Named parser routing remains; no duplicate named-computed fixture | `720c23e9a` |
| `function_call_tests::{accepts_fresh_template_for_mutable_parameter,accepts_fresh_collection_for_mutable_parameter,accepts_fresh_struct_constructor_for_mutable_parameter,accepts_explicit_copy_as_fresh_mutable_argument}` | Fresh template, collection, struct, and explicit-copy values satisfy mutable parameters | `function_call_mutable_param_{fresh_template_arg,fresh_collection_arg,fresh_struct_arg,copy_arg}` | none | `720c23e9a` |
| `function_call_tests::{rejects_tilde_on_immutable_place_argument,rejects_tilde_on_non_place_argument_literal,rejects_immutable_place_without_tilde_for_mutable_parameter,rejects_tilde_on_explicit_copy_argument}` | Invalid mutable call-site access forms | `function_call_tilde_on_immutable_place`, `function_call_tilde_on_non_place_expression`, and `function_call_immutable_place_missing_tilde` | Three source-location units retain marker/value precision | `720c23e9a` |
| `function_call_tests::{duplicate_named_parameter_uses_canonical_diagnostic_text,unknown_named_parameter_lists_known_parameter_hint}` | Claimed call-diagnostic wording/hint coverage but asserted only reason identity | `function_call_named_args_duplicate` and `function_call_named_args_unknown` | none | `720c23e9a` |
| `type_resolution_tests::zero_capacity_rejected` | Zero fixed capacity is rejected | `fixed_collection_zero_capacity_declaration_rejected`, with distinct signature/alias/field/return owners | none | `53039a0e4` |
| `type_resolution_tests::negative_capacity_rejected` | Negative fixed capacity is rejected | `fixed_collection_negative_capacity_rejected` | none | `53039a0e4` |
| `type_resolution_tests::runtime_int_binding_is_rejected_as_fixed_collection_capacity` | Runtime bindings cannot supply fixed capacity | `fixed_collection_capacity_runtime_variable_rejected` | none | `53039a0e4` |
| `type_resolution_tests::invalid_function_signature_capacity_is_not_erased_to_growable` | Invalid signature capacity remains a structured rejection | `fixed_collection_zero_capacity_signature_rejected` | none | `53039a0e4` |
| `generics_tests::{generic_scope_rejects_duplicate_parameter_names,generic_scope_rejects_collisions_with_forbidden_names,generic_scope_rejects_non_type_style_parameter_names}` | User-visible generic parameter declaration rejection | `generic_duplicate_parameter_rejected`, `generic_parameter_visible_type_collision_rejected`, and `generic_parameter_invalid_name_rejected` with structured reason/location assertions | `generic_scope_accepts_pascal_case_and_single_uppercase_names` retains hidden scope membership | `034f919ed` |
| `generics_tests::generic_display_uses_beanstalk_surface_style` | Legacy `DataType` renderer wording across generic shapes | none; datatype units do not own rendered compiler wording | canonical `TypeId` display and diagnostic render owners remain | `034f919ed` |
| `ReturnSlot::error` test-only helper | Constructed legacy error-return shape for the removed datatype renderer test | none | ordinary parsed/resolved return-channel paths | `034f919ed` |
| `function_call_mutable_param_fresh_computed_arg` -> `function_call_mutable_param_fresh_values`; removed `function_call_mutable_param_fresh_{template_arg,collection_arg,struct_arg}` | Fresh computed, template, collection, and struct values satisfy mutable parameters without authored `~` | `function_call_mutable_param_fresh_values` | `function_call_mutable_param_copy_arg` retains explicit-copy independence; negative access forms remain isolated | `73f13864c` |
| `choice_payload_equality_nested_choice_true` -> `choice_equality_truth_table`; removed `choice_unit_equality_success`, `choice_payload_equality_{same_payload_true,different_payload_false,unit_vs_payload_false,same_choice_different_variants_false,nested_choice_false}`, and `choice_payload_structural_equality_success` | Local structural equality truth table across unit, scalar-payload, variant, and nested-choice dimensions | `choice_equality_truth_table` | `choice_payload_equality_side_effects_evaluated_once` retains single-evaluation behavior; imported, aliased, generic, constructor-routing, and negative cases remain separate | `f601ebe2c` |
| `function_call_named_args_success` -> `function_call_named_and_default_args_success`; removed `function_call_named_args_{all_named,default_skip}` | Ordinary mixed positional/named, all-named, and skipped-default call binding | `function_call_named_and_default_args_success` | `function_call_named_args_mutable_success` retains exclusive-access routing; constructor defaults and all diagnostics remain separate | `4b9bd49e7` |
| `collection_methods_end_to_end` -> `collection_ordered_runtime_operations`; removed `collection_set_end_to_end` and `collection_literal_smoke` | Growable collection literal plus ordered push, set, remove, complete readback, and length semantics | `collection_ordered_runtime_operations` | `hashmap_js_runtime_contract` retains map ordering; collection helper/artifact, empty/fixed/control-flow, borrow, and error owners remain separate | `7c4e33a99` |
| `runtime_helpers::collection_push_returns_ok_carrier_on_success` | JavaScript collection-push helper returns the success carrier | `runtime_helpers::collection_push_returns_ok_after_mutation` | `explicit_empty_collection_can_be_pushed` and `fixed_collection_mutable_empty_push_success` own source-visible push success | `b42e9ab6f` |
| 14 HTML document-shell, fragment-order, output-plan, and tracked-asset units removed in Phase 13E | Final title/body style, const/runtime fragment order, HTML-Wasm colocation, logical route mapping, and tracked-asset output/reference behavior | `html_builder_page_title`, `html_builder_body_style`, `top_level_fragment_mixed_const_runtime_order`, `html_docs_site_proving_ground`, `hash_root_config_namesake_is_module_root`, `basic_page_output`, `html_tracked_asset_entry_root_basic`, `html_tracked_asset_relative_basic`, `html_tracked_asset_relative_parent_normalization` | 41 focused HTML-project units retain hidden config precedence, escaping, malformed path, registry, deduplication, resolution, and warning facts | `ca89c014e` |
| Phase 5D2 15 normalized HTML whole-page goldens | Runtime values for calls, collections, logical expressions, chars, structs, and short-circuit behavior | The same 15 canonical cases through `rendered_output_exact` | none | `d5256868b` |
| Phase 5D3 nine normalized receiver HTML whole-page goldens | Immutable, mutable, nested, chained, exported, alias-return, and post-mutation receiver behavior | The same nine canonical cases through exact context-rich runtime output | none | `914c1c131` |
| Phase 5D4 nine normalized template/import HTML whole-page goldens | Import execution/suppression, runtime templates, const slots, CSS, Markdown, and positional slot behavior | Runtime exact/order/exact-once plus narrow static `index.html` assertions in the same nine cases | none | `71f75c220` |
| Phase 5D5 five normalized static HTML whole-page goldens | Page/static const-template content, source order, and unreferenced tracked-asset non-emission | Narrow `index.html` content/order/exact-once assertions plus `artifacts_must_not_exist` | none | `952bd3134` |
| 14 AST collection diagnostic units in `collections_tests.rs` | Empty inferred collection/map rejection, immutable collection mutation, fixed-capacity overflow, mixed/duplicate/unsupported/const map literals, map length/get handling, and missing map mutation markers | Canonical collection and hashmap primary/boundary cases with exact reason, reason-specific rendered wording, or unique diagnostic-family ownership | 50 AST units retain parser shape, TypeId, builtin operation identity, source-path precision, and broad-code payload context | `eb6c92d9e` |
| `branching_tests::{rejects_same_line_second_match_arm,rejects_missing_match_arm_header}` | Match-arm newline and missing-header diagnostics | `diagnostic_match_arm_must_start_new_line` and `diagnostic_match_expected_arm_header` with exact code plus reason-specific rendered text | 38 AST units retain shape, precedence, parser-boundary, exhaustiveness, keyword-boundary, and unmirrored payload facts | `329cf59f5` |
| Four function-parsing diagnostic units | Mutable receiver missing-marker/temporary rejection, catch fallthrough, and multi-return trailing comma | `struct_mutable_receiver_requires_explicit_receiver_tilde`, `struct_mutable_receiver_temporary_receiver_rejected`, `result_handler_without_fallback_fallthrough_rejected`, and `function_trailing_comma_returns` | 43 AST units retain signature/return-slot shape, generic identity, call/fallible node form, and broad-code payload context | `5962d65b3` |
| Four declaration diagnostic units | Regular division in an Int declaration plus shorthand-empty, shorthand-nonliteral, and immutable-empty fixed collection rejection | `int_declaration_regular_division_rejected`, `fixed_collection_shorthand_empty_literal_rejected`, `fixed_collection_shorthand_non_literal_rhs_rejected`, and `fixed_collection_immutable_empty_binding_rejected` | 19 units retain declaration/value mode, named type, reserved-name, TypeMismatch context, multiline-token, and fixed/growable identity facts | `4fa61e817` |

### Coverage gap and handoff ledger

| Gap | Current accepted behavior | Planned owner | Phase/status |
|---|---|---|---|
| Hashmap `get` alias blocks live mutation | Shared map value access conflicts with overlapping mutation | Integration plus focused borrow facts | Phase 6 |
| Cross-module return-alias/effect summaries | Public summaries govern caller transfer where current modules support it | Integration plus summary units | Phase 9 accepted (`6d92cbd1e`) |
| Reactive update after subscription | Subscription is read-only dependency, not active borrow | Integration plus invalidation units | Phase 11 accepted (`780d0419c`) |
| `check`/build frontend parity | Same frontend diagnostics; `check` emits no artifacts | Command tests | Phase 10 accepted (`6a2c2e6b0`) |
| `check` target-planning parity | Current `check` stops after the shared frontend and does not yet run the accepted build-owned target-planning sequence | Canonical module compilation and scoped packages plan | Phase 10 handoff |
| Ordered root/fragment execution | Active root once; imported root dormant; source order preserved | Integration | Phases 5 and 10 accepted (`68fb90fb6`) |
| Structured diagnostic remapping | Codes/reasons/locations survive supported file boundaries | Integration | Phases 4 and 9 |
| Removed misleading lifetime precision case | No valid location contract existed | Real negative path/line case | Phase 4 |

### Durable decision log

- The backend baseline is universal harness protection; it is not authored case semantics.
- Acceptance-only means no stronger case-specific contract exists; it does not disable the baseline.
- Cross-backend matrices use backend-local contracts rather than artificial symmetry.
- Audit policy has one evaluator and reporting only serializes it.
- Stronger coverage that exposes a compiler bug stops migration until a separate root-cause fix is accepted.
- Integration owns source-visible behavior; units remain for hidden facts and stage boundaries.
- No arbitrary test-count or coverage-percentage target is used.
- Final TIR architecture is accepted and is not reopened by this plan.

---

## Final plan acceptance criteria

### Harness

- [ ] IDs, tags, contracts, and roles are retained and canonical.
- [ ] Case/tag/contract/backend filtering, list, and audit work.
- [ ] Canonical expectations and contained paths are mandatory.
- [ ] Acceptance-only intent is explicit and truthfully reported beside the backend baseline.
- [ ] Goldens are file-backed and consistently inventoried.
- [ ] Diagnostic and warning codes are exact by default.
- [ ] Structured diagnostic reasons and locations are supported.
- [ ] Runtime exact/order/exact-once assertions are supported.
- [ ] One suite-policy evaluator is part of normal validation.

### Coverage

- [ ] Hashmap access/ownership semantics have integration owners.
- [ ] Cross-module effects have current owners or explicit deferred handoffs.
- [ ] Reactive post-subscription mutation/update has HTML-JS integration ownership.
- [ ] `check`/build frontend parity is covered.
- [ ] Root activation and fragment ordering are strongly asserted.
- [ ] Diagnostic source remapping is covered.
- [ ] Cross-backend cases share one input with intended backend-local contracts.

### Pruning

- [ ] Source-shaped borrow/map duplicates are removed.
- [ ] Whole-source call/type/generic duplicates are removed.
- [ ] Backend semantic substitutes are removed.
- [ ] Positive micro-fixtures are consolidated only where one contract permits it.
- [ ] Retained goldens are deliberate.
- [ ] Hidden facts, malformed HIR, canonical identity, transfer, and backend ABI tests remain.

### Governance

- [ ] Every canonical case has one role.
- [ ] Every primary case has one contract.
- [ ] No contract has multiple primary owners.
- [ ] Contains-mode diagnostics are justified.
- [ ] Pruning and coverage ledgers are complete.
- [ ] Representative mutation probes fail as expected.
- [ ] Documentation and roadmap are current.
- [ ] `just validate` passes.
