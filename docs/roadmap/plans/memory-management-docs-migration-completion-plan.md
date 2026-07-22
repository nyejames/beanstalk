# Memory Management Documentation Migration Completion Plan

**Status:** complete  
**Repository:** `nyejames/beanstalk`  
**Baseline reviewed:** `main` at `7f9ab487aecc64ef31f68d5d615d6f07794ecaf9`  
**Primary scope:** Documentation architecture, accepted design clarification, migration completion, and retirement of the legacy memory monolith  
**Change class:** Documentation-only  
**Required final gate:** `bean build docs --release` or `cargo run --quiet -- build docs --release`  
**Follow-up work:** A separate compiler-drift implementation plan must be produced after this documentation plan lands

## Current state

ACTIVE_PLAN: `docs/roadmap/plans/memory-management-docs-migration-completion-plan.md`
STATUS: complete
CURRENT_SLICE: none
LAST_ACCEPTED_COMMIT: pending parent commit
WORKTREE: main; unrelated compiler/src changes preserved and not staged
REQUIRED_RELOADS: startup files, this plan, current source/diff
RELEVANT_CONTEXT_NOW:
- docs: split memory authority complete; legacy monolith deleted
- code: none (documentation-only)
ACCEPTANCE_CRITERIA:
- all phase acceptance criteria satisfied
VALIDATION_STATE:
- `bean check docs`: passed
- `bean build docs --release`: passed; 64 files
DOCS_IMPACT: memory leaf, compiler/build/language/progress/roadmap/index updated; `docs/release/**` regenerated
BLOCKERS_OR_OPEN_DECISIONS: none
DELEGATION_DECISION: parent-direct - documentation-only parent-owned work
NEXT_WORKER_ORDER: none
STOP_REASON: active work source and final review complete
NEXT_RESUME_ACTION: none; produce separate compiler-drift implementation plan later

---

## 1. Purpose

Complete the migration of Beanstalk's memory-management design from the legacy consolidated file:

```text
docs/memory-management-design.md
```

into the focused canonical documentation under:

```text
docs/src/docs/codebase/memory-management/
```

The completed documentation must:

- preserve every accepted decision from the legacy reference;
- correct the inaccuracies already identified in the split pages;
- encode the design decisions agreed during the memory-design interview;
- add a canonical lifetime-region and escape-validation area;
- make the source language's reference semantics impossible to miss;
- clearly separate mandatory semantic validation from optional ownership optimisation;
- define the accepted-but-deferred grouped-memory surface;
- define external memory-boundary profiles without inventing unrestricted foreign lifetime graphs;
- give diagnostics first-class architectural status;
- synchronize the memory, compiler, build-system, language, roadmap, progress, and educational authorities;
- remove all repository references that still treat the legacy monolith as authoritative;
- delete the legacy monolith only after complete coverage and successful documentation validation.

The end state must leave one unambiguous memory-design authority. A coding agent must not need to infer design from current Rust code or choose between competing documents.

---

## 2. Definition of completion

This plan is complete only when all of the following are true:

- [ ] The split memory-management pages explicitly declare themselves the canonical authority for program memory semantics.
- [ ] The exact sentence below appears prominently in all required locations:

  > **Beanstalk is reference-semantic by default, copy-explicit and move-inferred. It omits explicit reference types and lifetime syntax, not references themselves.**

- [ ] Access semantics, lifetime topology, ownership responsibility, and backend representation are described as distinct concepts.
- [ ] A new `lifetime-regions-and-escape-validation` documentation leaf exists and is linked from the memory landing page and task-reading guide.
- [ ] Optional inferred transfer is documented as an optimisation that always backs off to borrowing when proof is unavailable.
- [ ] Mandatory lifetime-topology validation is documented as a source-legality check that GC cannot bypass.
- [ ] Aggregate storage is documented as shared-aliasing by default, with explicit `copy` for independent storage and optional final-use transfer.
- [ ] `copy` has a complete graph-level semantic definition.
- [ ] Mutable aliases, non-lexical alias activity, interior projections, returns, cycles, reactive lifetimes, and builder-owned lifecycle roots are specified.
- [ ] `group` / `into` is documented as accepted end-state syntax with implementation explicitly deferred.
- [ ] The grouped-memory draft contains no unresolved design question.
- [ ] External bindings use closed semantic boundary profiles, including the value-only WIT component profile and restricted host-binding profile.
- [ ] Diagnostics distinguish proven-invalid topology from topology that conservative analysis cannot prove legal.
- [ ] Compiler and build-system documents place lifetime-topology analysis at the correct boundaries.
- [ ] Educational compiler pages no longer teach stale memory-stage ownership.
- [ ] The progress matrix clearly distinguishes current borrow-checker support from deferred lifetime-region/group implementation.
- [ ] `index.md` and every other live reference point to the split memory authority.
- [ ] `docs/memory-management-design.md` is deleted.
- [ ] Repository-wide searches find no stale legacy references or contradictory wording.
- [ ] The documentation release build succeeds.
- [ ] Every changed memory route and relevant generated diff is manually inspected.

---

## 3. Scope boundaries

### 3.1 In scope

- Canonical memory design prose.
- Documentation information architecture.
- Exact source-semantic rules.
- Compiler-stage and build-stage ownership as documented architecture.
- Accepted-but-deferred syntax and implementation direction.
- Status wording and roadmap sequencing.
- Progress-matrix clarity.
- Documentation navigation and generated documentation output.
- Deletion of the legacy documentation source.

### 3.2 Out of scope

Do not make any Rust, test, fixture, compiler, backend, runtime, manifest, or build-script change in this plan.

Specifically, do not:

- fix aggregate forced-move behaviour;
- change scalar copy behaviour;
- change current move diagnostics;
- implement lifetime-region analysis;
- implement `group` or `into`;
- implement hidden result destinations;
- implement WIT component importing;
- change current external-package metadata;
- implement graph-preserving deep copy;
- change the ownership bit or Wasm runtime ABI;
- implement page, request, frame, mount, or arena regions;
- add source-visible reference counting;
- directly edit generated files under `docs/release/**`;
- run the code-bearing `just validate` gate for this documentation-only slice.

All current compiler mismatches identified by this plan must be captured in the drift ledger for the later compiler plan. They must not be silently normalized into the documentation.

---

## 4. Authority and required reading

### 4.1 Authority order

When sources disagree, use this order:

1. The explicit decisions recorded in this approved plan.
2. The focused canonical memory-management pages after this migration.
3. `docs/compiler-design-overview.md` for compiler-stage and artefact ownership.
4. `docs/build-system-design.md` for project, linking, target planning, builder lifecycle, and output ownership.
5. `docs/language-overview.md` and `docs/src/docs/codebase/language/**` for source syntax and user-visible semantics.
6. `AGENTS.md` and the codebase documentation standards.
7. `docs/src/docs/progress/#page.bst` for current implementation status.
8. `docs/roadmap/roadmap.md` and accepted roadmap plans for sequencing.
9. Current implementation code, used only to identify status and drift.
10. Educational compiler-design pages, which explain but do not define architecture.
11. The legacy monolith, used only as a migration coverage source until deleted.

### 4.2 Read before implementation

The agent executing any phase must read the following from the current worktree, in this order:

- [ ] `AGENTS.md`
- [ ] `docs/compiler-design-overview.md`
- [ ] `docs/build-system-design.md`
- [ ] `docs/language-overview.md`
- [ ] `docs/src/docs/codebase/style-guide/style-guide.bd`
- [ ] `docs/src/docs/codebase/style-guide/validation.bd`
- [ ] `docs/src/docs/codebase/memory-management/overview.bd`
- [ ] Every existing memory-management leaf overview and detailed file
- [ ] `docs/roadmap/plans/grouped-memory-design.md`
- [ ] `docs/src/docs/progress/#page.bst`
- [ ] `docs/roadmap/roadmap.md`
- [ ] `docs/memory-management-design.md`, until its deletion phase
- [ ] The agreed external-boundary answer supplied with this plan

Before changing an educational page, also read the corresponding current compiler-design educational article.

### 4.3 Repository refresh rule

The baseline SHA in this plan is an audit anchor, not permission to ignore newer commits.

At the start of every phase:

- [ ] Refresh `main` or the active branch.
- [ ] Record the current `HEAD` SHA in the phase notes.
- [ ] Re-read every file owned by that phase.
- [ ] Re-run targeted searches for terms owned by that phase.
- [ ] Treat newer accepted documentation as authoritative unless it conflicts with this approved design ledger.

---

## 5. Locked design ledger

The following decisions are approved. The implementation agent must document them and must not reinterpret, weaken, or replace them.

### 5.1 Core source model

1. **Beanstalk is reference-semantic by default, copy-explicit and move-inferred. It omits explicit reference types and lifetime syntax, not references themselves.**
2. Reading, binding, passing, returning, or storing an existing value uses shared reference semantics unless explicit exclusive access or explicit copy semantics apply.
3. Shared access is a source-semantic relationship. It does not require every scalar to be heap allocated or represented by a pointer.
4. `copy place` creates independent value semantics.
5. `~place` requests exclusive mutation-capable access to an existing mutable place. It is not a move marker and not a type constructor.
6. Consumption and destruction-responsibility transfer are compiler-inferred only. There is no move keyword, consuming parameter spelling, owned signature variant, or lifetime syntax.
7. Source legality is backend-independent and build-profile-independent.

### 5.2 Alias activity and mutation

8. Shared and exclusive aliases use non-lexical, control-flow-sensitive activity.
9. A shared alias blocks overlapping mutation only until its last potential use on the relevant control-flow path.
10. An unused shared alias does not block later mutation.
11. Branches are analysed independently; joins conservatively retain activity when any incoming path may still use the alias.
12. Loops use fixed-point future-use reasoning.
13. Reactive subscriptions are read-only dependencies, not active borrow lifetimes.
14. `alias = source` creates a shared read-only alias.
15. `alias ~= source` or an equivalent typed mutable declaration from an existing place creates an exclusive mutable alias.
16. The source place must permit mutation and no conflicting live access may exist.
17. Assignment through a mutable alias writes through to the referent. It must not silently detach or rebind the alias.
18. A mutable declaration initialized from a fresh value creates an independent mutable slot.
19. Mutable receiver methods require an existing mutable place. Fresh-rvalue materialisation for ordinary mutable parameters does not make temporaries valid mutable receivers.

### 5.3 Aggregate storage

20. Existing values stored in structs, choices, collections, maps, tuples, templates, or other aggregates are shared aliases by default.
21. Storing a value does not automatically make the aggregate an independent owner of a duplicated child.
22. A proven final-use store may be lowered as an inferred transfer without changing source meaning.
23. `copy` is required when the stored value must have an independent lifetime or independent mutable graph.
24. Maps semantically own their entry structure while keys and values follow the same shared/copy/inferred-transfer rules as other stored values.
25. Map lookup keys are borrowed.
26. `get` returns a shared alias to stored data.
27. `remove` removes an entry and returns the removed value under the normal lifetime and ownership rules; it does not define a general source-level consuming argument mode.
28. Backend scalar representation must not create a separate source-semantic implicit-copy exception.

### 5.4 Inferred transfer

29. Optional move inference must never reduce source-program acceptance.
30. Transfer is legal only when no future source use exists on every relevant path and all alias/lifetime requirements are satisfied.
31. When later use exists, the outcome is path-dependent, or proof is imprecise, the operation remains a borrow.
32. Lack of transfer proof may cause GC-backed or otherwise conservative lowering. It must not cause a source rejection by itself.
33. There is no mandatory-consuming operation for ordinary values in the current source language.
34. A user-facing use-after-consumption diagnostic is valid only for a semantically mandatory consumption operation. No such ordinary-value operation is currently accepted.
35. Immutable and mutable parameters may both receive inferred destruction responsibility at a proven final-use call site.
36. Parameter mutability controls access; ownership metadata controls destruction responsibility.
37. An immutable parameter receiving ownership remains read-only.
38. A mutable parameter receiving ownership may mutate according to the source contract.
39. `MayConsume`, `NeverConsumes`, and `AlwaysConsumes` are analysis/lowering categories, not source-visible signature categories.
40. `AlwaysConsumes` may describe a specialised already-proven call path; it must not turn a source function into a mandatory-consuming API.
41. Group-owned values are normally passed as borrowed because the group owns the allocation family's destruction boundary.

### 5.5 Lifetime topology

42. Every runtime allocation has exactly one semantic lifetime owner.
43. Multiple bindings, fields, elements, or returned values may alias one allocation without becoming additional lifetime owners.
44. A retained reference is legal only when the referenced allocation belongs to the same lifetime region as the retaining object or to a region statically known to outlive it.
45. Use this formal rule in the documentation:

    ```text
    R_value >= R_container
    ```

    where `R_value` lives at least as long as `R_container`.
46. Shared aliases may outlive the lexical binding that first named the storage.
47. Lexical scope does not define allocation lifetime.
48. Escaping or retained aliases do not create unrestricted dynamic shared ownership.
49. Fresh escaping values should be allocated directly into the destination lifetime where possible.
50. Existing values may transfer destruction responsibility at a proven final use.
51. Interior projections remain rooted in their containing allocation family and may keep that family alive.
52. A projection does not silently become an independent allocation.
53. Implicit region inference may widen an allocation only along one ordered existing owner chain.
54. The selected owner is the nearest existing ancestor that outlives every retained observer.
55. The compiler must not widen farther than necessary.
56. The compiler must not invent a page-, application-, or process-lifetime owner merely to avoid a diagnostic.
57. Independently ending sibling lifetime domains do not form one ordered chain.
58. Sharing across sibling domains requires one of:
    - an already-existing common semantic owner;
    - an enclosing declared memory group;
    - a builder-declared common lifecycle;
    - independent storage created by `copy`.
59. Cross-region cycles are invalid.
60. Every strongly connected allocation graph must belong to one lifetime region.
61. Same-region cycles are lifetime-safe, while direct source construction of cyclic reference graphs remains deferred.
62. Multiple return values may alias one allocation when they remain under one caller lifetime owner.
63. Lifetime-topology legality is mandatory in every build and for every backend.
64. Development and release profiles may differ only in optimisation effort and physical representation.
65. GC is a permitted physical representation for a statically legal topology. GC cannot make an invalid or unproven topology legal.

### 5.6 Two distinct proof layers

The docs must explicitly separate these two proof obligations:

**Mandatory semantic proof**

- access and exclusivity legality;
- alias relationships;
- lifetime owner and region legality;
- retained-edge outlives relationships;
- escape legality;
- cycle legality;
- external-boundary legality.

Failure to prove mandatory semantic legality may produce a source diagnostic.

**Optional optimisation proof**

- final-use transfer;
- deterministic drop placement;
- retain/drop elimination;
- stack, arena, or region allocation;
- collector elision;
- ownership specialisation.

Failure to prove an optimisation must preserve semantics through a conservative representation such as GC. It must not reject the program.

### 5.7 Ownership metadata

66. The ownership bit records destruction responsibility only.
67. `borrowed` means the runtime path must not destroy the value.
68. `owned` means the runtime path carries release or safe-transfer responsibility.
69. The bit does not prove uniqueness.
70. The bit does not identify the last observer.
71. The bit does not establish lifetime-topology legality.
72. The bit does not imply individual releasability when the value belongs to a region-owned allocation family.
73. Additional runtime metadata may optimise already-valid programs but must not become a hidden dynamic-sharing contract.
74. Internal reference counting is permitted as a backend representation for a statically legal topology. Source-visible RC, retain/release, weak ownership, or dynamic shared-ownership semantics remain outside the language design.

### 5.8 Explicit copy

75. `copy place` performs a semantic deep copy of the complete copyable runtime value graph reachable from the place.
76. Every copyable allocation in the graph is duplicated into the destination lifetime region.
77. Internal alias topology is preserved: repeated source references to one allocation become repeated references to one copied allocation.
78. Same-region cycles are preserved in the copied graph.
79. The copied graph shares no mutable allocation with the source graph.
80. Immutable scalar storage may be physically reused only where identity and alias behaviour are unobservable.
81. A reactive source is copied as its current value, not as the same reactive source identity.
82. External opaque handles and resources are non-copyable unless their semantic profile defines a genuine independent-copy operation.
83. A non-copyable graph member produces a source diagnostic. The compiler must not silently retain an alias.
84. A copy result may be allocated directly into an inferred or declared destination region.

### 5.9 Grouped memory

85. `group name:` and `into group` are accepted end-state syntax.
86. Their implementation remains deferred and belongs to a separate future compiler plan.
87. Grouped-memory implementation should be evaluated as a likely prerequisite for the full ownership-aware Wasm completion plan, but this docs plan must not mark it active or implemented.
88. A memory group is a declared semantic lifetime region, not an allocator object or value.
89. Groups are not types, fields, parameters, generic arguments, traits, or lifetime annotations.
90. V1 placement syntax is:

    ```text
    name [access/type] into group_name = expression
    ```
91. V1 placement is allowed only on declaration receiving boundaries.
92. V1 has no expression-site placement, reassignment placement, extraction, or unrestricted group-to-group adoption.
93. Placement may target the current group or a lexically enclosing ancestor group, never a sibling or unrelated group.
94. A binding's lexical owner follows its destination group.
95. A binding declared inside a nested block but placed into an ancestor group remains visible from its declaration point through the remainder of that destination group.
96. Name collisions are checked in the destination group scope.
97. Ordinary declarations without `into` retain normal lexical visibility.
98. Declared groups are hard boundaries and cannot be silently widened.
99. Group-owned values may retain same-group and ancestor-region aliases.
100. Parent, sibling, or otherwise longer-lived regions must not retain child-group values.
101. Group values, projections, and aliases must not escape beyond the group unless independent storage is created in the destination lifetime.
102. Fresh results may use hidden destination-directed allocation into a selected group.

### 5.10 Lifetime analysis ownership

103. Lifetime-topology legality belongs to a distinct backend-neutral analysis after borrow validation and before target planning.
104. Do not call the backend-lowering area “Stage 7.”
105. Do not invent a numbered Stage 7 in this documentation change. Add an explicit unnumbered analysis boundary after Stage 6 unless a separate architecture decision changes stage numbering.
106. Per-function/module analysis produces local allocation, alias, retention, escape, result, and outlives constraints and exported summaries.
107. Build/link analysis instantiates those summaries over the reachable call graph and builder-supplied lifecycle roots.
108. The analysis reads validated HIR and read-only borrow/effect facts.
109. It writes immutable side-table facts and summaries.
110. It does not rewrite HIR.
111. It does not choose JS versus Wasm.
112. It does not choose physical allocation representation.
113. It does not make source syntax decisions.
114. Backends receive a validated topology and may not reconsider source legality.

### 5.11 Diagnostics

115. High-quality diagnostics are a first-class part of the memory architecture.
116. Diagnostics are a central reason Beanstalk can reject hidden dynamic shared ownership without exposing source-level RC.
117. Lifetime diagnostics must distinguish:
    - topology proven invalid;
    - topology not proven legal by conservative analysis;
    - invalid group syntax or placement;
    - non-copyable graph contents;
    - unsupported external boundary profile;
    - missing or inconsistent compiler-owned metadata.
118. User-facing diagnostics use stable codes and structured reason payloads.
119. Internal impossible or inconsistent metadata uses `CompilerError`.
120. Every lifetime diagnostic should identify, where applicable:
    - allocation or value origin;
    - retaining object or escaping use;
    - relevant source group or builder lifecycle owner;
    - shorter- and longer-lived regions;
    - retained-edge or projection path;
    - external boundary profile;
    - failed rule.
121. Remedies must be ranked in this order:
    1. allocate directly into the required destination region;
    2. place observers under one common group;
    3. create independent storage with `copy`;
    4. shorten the alias or retained edge;
    5. repair package-owned external lifetime metadata.
122. There is no backend-specific escape from semantic lifetime diagnostics.

### 5.12 External boundary profiles

123. External bindings use closed semantic boundary profiles. They do not expose arbitrary user-defined lifetime graphs.
124. General Wasm libraries use a strict value-only WIT component profile in V1.
125. Every supported argument crosses from a shared read of a Beanstalk value into an independent component value.
126. Every supported result is lifted into a fresh Beanstalk value in the destination lifetime.
127. No Beanstalk reference, alias, lifetime-region identity, ownership state, or destruction responsibility crosses the component boundary.
128. The source call is not consuming.
129. The component may use any private allocation strategy without declaring it to Beanstalk.
130. WIT resources, owned or borrowed resource handles, callbacks, async operations, futures, streams, shared-memory views, raw pointer escapes, returned aliases, and retained Beanstalk references are rejected by the V1 profile.
131. Unsupported WIT features receive structured import diagnostics and may suggest a wrapper component that presents a value-only interface.
132. Existing Core, Builder, JavaScript-provider, and curated platform bindings use a separate restricted host-binding profile.
133. Ordinary Beanstalk values cross that profile by value.
134. Host code may not retain references into ordinary Beanstalk storage.
135. Opaque handles represent foreign identity, not Beanstalk reference types.
136. Shared or mutable access to a handle controls legal access to the foreign identity, not direct access to host object storage.
137. Observable external resources require explicit close or teardown operations.
138. Host or component finalization timing must not define observable language behaviour.
139. Imported components own private runtime memory. The one-page/one-memory rule applies to Beanstalk-linked Wasm variants, not arbitrary imported components.
140. A future resource-capable profile requires a separate complete design for identity, transfer, teardown, callbacks, async suspension, reentrancy, cycles, and failures.

---

## 6. Current-state gap and drift audit

The following findings are the minimum issues this plan must address.

| ID | Current state | Required correction | Primary owner |
|---|---|---|---|
| M1 | Split docs do not state the reference-semantic model strongly enough. | Repeat the approved canonical sentence in the top overview, access leaf, lifetime leaf, language authority, and website landing page. | Memory overview / access / language |
| M2 | The access leaf allows a caveat for temporary mutable receivers. | State that mutable receivers always require an existing mutable place; fresh-rvalue materialisation applies only to ordinary mutable parameters. | Access leaf |
| M3 | Aggregate wording says aggregates own stored values without defining shared child aliases. | Define shared-by-default storage, explicit copy, and optional final-use transfer. | Access and lifetime leaves |
| M4 | Current compiler forces non-scalar aggregate children to move and implicitly copies scalars. | Record as implementation drift for the later compiler plan; do not make it canonical. | Progress/drift ledger only |
| M5 | Alias liveness is not explicitly non-lexical. | Define last-potential-use activity over branches, joins, and loops. | Access and borrow leaves |
| M6 | Mutable declarations initialized from existing places are ambiguous. | Define exclusive write-through mutable aliases versus fresh mutable slots. | Access and language authorities |
| M7 | `copy` lacks graph-level topology, cycle, reactive, and non-copyable-resource rules. | Add the complete approved copy contract. | Access and lifetime leaves |
| M8 | Ownership overview says lowering “decides inferred move points.” | Borrow/analysis proves optional transfer; lowering only realises validated facts. | Ownership overview |
| M9 | Path-dependent move wording permits rejection. | Optional transfer backs off to borrowing; source rejection is not permitted merely because transfer is ambiguous. | Borrow and ownership leaves |
| M10 | Current checker may emit move-sensitive rejection and treats only mutable user parameters as consumable. | Record as compiler drift; accepted docs allow inferred transfer for immutable and mutable parameters. | Progress/drift ledger |
| M11 | No canonical lifetime-topology model exists in the split memory area. | Add the new lifetime-region and escape-validation leaf. | New lifetime leaf |
| M12 | GC is described as fallback without distinguishing optimisation proof from legality proof. | State that GC preserves legal semantics but cannot legalise invalid or unproven lifetime topology. | Overview / lifetime / runtime |
| M13 | Region inference, sibling sharing, cycles, projections, and common owners are under-specified. | Add exact nearest-existing-ancestor, no-lateral-promotion, SCC, and allocation-family rules. | New lifetime leaf |
| M14 | Diagnostics are treated mainly as an implementation concern. | Give lifetime diagnostics an architectural contract and ranked remedies. | Lifetime leaf / compiler overview |
| M15 | The grouped-memory plan contains an unresolved sibling-sharing question. | Replace it with the approved nearest-existing-ancestor rule and remove all unresolved-status wording. | Grouped-memory plan |
| M16 | Grouped-memory plan assigns topology legality to borrow validation. | Move topology ownership to the distinct lifetime-region analysis; borrow validation remains access/transfer safety. | Grouped-memory plan / compiler overview |
| M17 | Group binding visibility into ancestor groups is unspecified. | Define destination-group lexical ownership and visibility. | Grouped-memory plan / language overview |
| M18 | Grouped-memory non-goals appear to reject RC categorically. | Reject source-visible RC while permitting internal RC as a backend representation of a statically legal topology. | Grouped-memory and runtime docs |
| M19 | Runtime overview refers to nonexistent “Stage 7.” | Link to Stage 6, the unnumbered lifetime analysis boundary, backend handoff, and build-owned target planning. | Runtime overview |
| M20 | Runtime input contract omits selected functions, target assignments, import/export/capability plans, layout identities, and lifetime facts. | Expand the compiler/build/backend handoff. | Runtime leaf / compiler / build |
| M21 | Page memory topology is not connected to memory docs. | Link the build-owned one-page runtime/memory contract and distinguish imported components. | Runtime leaf / build design |
| M22 | External function metadata discussion assumes arbitrary retention graphs. | Replace with closed WIT-value and host-binding profiles; defer richer resources. | Lifetime / runtime / compiler / language |
| M23 | Reactivity lacks the full lifetime-owner relation. | Distinguish invalidation facts from builder-owned page/mount lifetime roots. | Borrow / lifetime / runtime |
| M24 | Checked-operation failure paths are not clearly part of HIR before memory analyses. | State that recoverable checked paths are explicit HIR control flow before borrow/lifetime validation. | Borrow leaf / compiler overview |
| M25 | Educational memory pages omit lifetime-topology analysis. | Update pipeline teaching and link to canonical leaves. | Compiler educational pages |
| M26 | Progress matrix marks borrow/ownership broadly supported but has no separate deferred lifetime/group row. | Keep current checker status accurate and add explicit deferred topology/group status. | Progress matrix |
| M27 | `index.md` still links to the legacy monolith. | Link the canonical split overview. | `index.md` |
| M28 | Legacy monolith remains in the repository. | Delete only after all coverage, cross-links, searches, and validation pass. | Final retirement phase |

---

## 7. Target documentation architecture

### 7.1 Canonical memory tree

```text
docs/src/docs/codebase/memory-management/
├── #page.bst
├── overview.bd
├── access-and-aliasing/
│   ├── #page.bst
│   ├── overview.bd
│   └── access-and-aliasing.bd
├── borrow-validation/
│   ├── #page.bst
│   ├── overview.bd
│   └── borrow-validation.bd
├── lifetime-regions-and-escape-validation/
│   ├── #page.bst
│   ├── overview.bd
│   └── lifetime-regions-and-escape-validation.bd
├── ownership-and-drops/
│   ├── #page.bst
│   ├── overview.bd
│   └── ownership-and-drops.bd
└── runtime-and-backend-lowering/
    ├── #page.bst
    ├── overview.bd
    └── runtime-and-backend-lowering.bd
```

### 7.2 Topic ownership

| Topic | Canonical owner |
|---|---|
| Reference-semantic source model | `access-and-aliasing` |
| `~`, mutable aliases, copies, aggregate storage | `access-and-aliasing` |
| Shared/exclusive conflict analysis and optional transfer safety | `borrow-validation` |
| Lifetime owners, region topology, escapes, retained edges, cycles | `lifetime-regions-and-escape-validation` |
| Ownership bit, responsibility transfer, conditional destruction | `ownership-and-drops` |
| GC/RC/arena/handle representations and backend handoff | `runtime-and-backend-lowering` |
| `group` / `into` accepted deferred design | `docs/roadmap/plans/grouped-memory-design.md`, summarized and linked from the lifetime leaf |
| Compiler-stage and artefact ownership | `docs/compiler-design-overview.md` |
| Project/link/builder lifecycle roots and page runtime memory | `docs/build-system-design.md` |
| Exact source syntax and deferred language surface | `docs/language-overview.md` |
| Current support | `docs/src/docs/progress/#page.bst` |
| Implementation order | `docs/roadmap/roadmap.md` |

### 7.3 Files expected to change

The implementation agent must review every file below and edit it when required by the phase instructions:

```text
docs/src/docs/codebase/memory-management/overview.bd
docs/src/docs/codebase/memory-management/#page.bst

docs/src/docs/codebase/memory-management/access-and-aliasing/overview.bd
docs/src/docs/codebase/memory-management/access-and-aliasing/access-and-aliasing.bd

docs/src/docs/codebase/memory-management/borrow-validation/overview.bd
docs/src/docs/codebase/memory-management/borrow-validation/borrow-validation.bd

docs/src/docs/codebase/memory-management/lifetime-regions-and-escape-validation/#page.bst
docs/src/docs/codebase/memory-management/lifetime-regions-and-escape-validation/overview.bd
docs/src/docs/codebase/memory-management/lifetime-regions-and-escape-validation/lifetime-regions-and-escape-validation.bd

docs/src/docs/codebase/memory-management/ownership-and-drops/overview.bd
docs/src/docs/codebase/memory-management/ownership-and-drops/ownership-and-drops.bd

docs/src/docs/codebase/memory-management/runtime-and-backend-lowering/overview.bd
docs/src/docs/codebase/memory-management/runtime-and-backend-lowering/runtime-and-backend-lowering.bd

docs/roadmap/plans/grouped-memory-design.md
docs/roadmap/roadmap.md
docs/compiler-design-overview.md
docs/build-system-design.md
docs/language-overview.md
docs/src/docs/progress/#page.bst

docs/src/docs/codebase/compiler-design/memory-management-and-gc/memory-management-and-gc.bd
docs/src/docs/codebase/compiler-design/borrow-validation-and-drops/borrow-validation-and-drops.bd
docs/src/docs/codebase/compiler-design/overview.bd

docs/src/docs/codebase/design-scope/overview.bd
docs/src/docs/codebase/overview.bd
index.md
```

Review but do not change unless a stale reference or contradiction is found:

```text
AGENTS.md
CONTRIBUTING.md
README.md
```

Delete in the final retirement phase:

```text
docs/memory-management-design.md
```

Never edit generated files directly:

```text
docs/release/**
```

---

## 8. Required canonical wording

The following wording is mandatory. Minor punctuation changes are allowed only when required by Beandown syntax; semantic wording must remain intact.

### 8.1 Reference semantics

> **Beanstalk is reference-semantic by default, copy-explicit and move-inferred. It omits explicit reference types and lifetime syntax, not references themselves.**

Required locations:

- [ ] Top-level memory overview, above the detailed model.
- [ ] Memory website landing page.
- [ ] Access-and-aliasing design contract.
- [ ] Lifetime-region detailed opening.
- [ ] Language overview's memory/reference section.

### 8.2 Aggregate storage

> Existing values stored in aggregates retain shared reference semantics by default. `copy` creates independent storage. At a proven final use, the compiler may realise the same source operation as an ownership transfer without changing source meaning.

### 8.3 Optional transfer

> Inferred transfer is optional. When safe transfer is not proven on every relevant path, the operation remains a borrow. Failure to prove an ownership optimisation must not make an otherwise valid source program invalid.

### 8.4 GC and legality

> GC is a permitted runtime representation for a statically legal lifetime topology. It is not a mechanism for accepting invalid retained edges, lateral lifetime promotion, unowned escapes or cross-region cycles.

### 8.5 Stored-edge rule

> Every allocation has exactly one semantic lifetime owner. An object may retain a reference only when the referenced allocation belongs to the same lifetime region or to a region statically known to outlive the retaining object.

### 8.6 Nearest-owner inference

> Implicit region inference may widen an allocation only to the nearest existing ancestor on the same ordered owner chain that outlives every retained observer. It may not silently promote an allocation laterally across independently ending sibling lifetime domains.

### 8.7 Ownership bit

> The ownership bit records destruction responsibility. It does not prove uniqueness, identify the final observer, establish lifetime legality or create dynamic shared ownership.

### 8.8 External WIT boundary

> Beanstalk imports general external Wasm libraries through a value-only WIT component profile. Supported arguments are lowered from shared reads into independent component values, and results are lifted into fresh Beanstalk values. No Beanstalk alias, lifetime owner, ownership state or destruction responsibility crosses the component boundary.

### 8.9 Host-binding boundary

> Existing Core, Builder and JavaScript-backed bindings use a restricted host-binding profile. Ordinary Beanstalk values cross by value, host code may not retain references into ordinary Beanstalk storage, and opaque handles represent foreign identities rather than Beanstalk reference types.

### 8.10 Diagnostic priority

> Lifetime diagnostics are part of the memory model, not an afterthought. They must identify the failed topology and steer the programmer toward direct destination allocation, a common lifetime group, independent storage through `copy`, or a shorter retained edge.

---

## 9. Wording that must be removed or avoided

Do not leave any sentence that says or implies:

- “Beanstalk has no references.”
- “Existing bindings are values rather than references.”
- “Aggregates always take ownership of stored values.”
- “Scalars are implicitly copied as a separate source rule.”
- “Path-dependent inferred moves must be rejected.”
- “The ownership lowerer decides whether source code moved a value.”
- “Only mutable parameters can receive inferred ownership.”
- “GC can accept any lifetime graph that deterministic lowering cannot prove.”
- “GC-only backends may skip lifetime topology validation.”
- “The ownership bit proves uniqueness.”
- “Reference counting is forbidden as an internal backend representation.”
- “Temporary values can be mutable receivers.”
- “The backend-lowering area is compiler Stage 7.”
- “The page's shared Wasm memory includes arbitrary imported components.”
- “Unknown external retention can be treated as GC fallback.”
- “Group identity is part of `TypeId`.”
- “Build profile changes source legality.”

---

## 10. Phase execution protocol

Each phase is intended to fit in one coding-agent context.

For every phase:

1. Re-read the phase's owned files from the current worktree.
2. Search for duplicate or contradictory wording before editing.
3. Make the smallest coherent documentation slice.
4. Keep the legacy monolith until the retirement phase.
5. Do not leave placeholder headings, unresolved questions, or “TODO” prose.
6. Use exact authority links rather than duplicating another document's full contract.
7. Run `bean check docs` or `cargo run --quiet -- check docs` as an iteration check when practical.
8. Inspect the source diff before ending the phase.
9. Update this plan's checkboxes or a copied worktree version.
10. Record any newly discovered compiler drift in the drift appendix; do not fix it in this plan.

A phase is not complete merely because the docs compile. Its acceptance criteria must also pass.

---

# Phase 0 — Refresh, inventory, and freeze the migration map

## Context and reasoning

The repository is changing quickly. The first phase prevents edits against stale content and produces a complete migration ledger before any canonical wording changes. It also ensures that no legacy reference is missed at retirement.

## Files

Read-only in this phase, except optionally adding this plan to:

```text
docs/roadmap/plans/memory-management-docs-migration-completion-plan.md
```

## Tasks

- [ ] Confirm the worktree is clean or record unrelated changes.
- [ ] Record `git rev-parse HEAD`.
- [ ] Add this plan to the repository plan directory if it is not already tracked.
- [ ] Read all required authorities listed in Section 4.
- [ ] Inventory every source file under `docs/src/docs/codebase/memory-management/**`.
- [ ] Inventory every route wrapper and generated route corresponding to the memory area.
- [ ] Search live source documentation for legacy file references:

  ```sh
  rg -n "memory-management-design|memory-management-overview|Beanstalk Memory Management Strategy|Legacy consolidated reference" \
      . --glob '!docs/release/**'
  ```

- [ ] Search for known contradictory wording:

  ```sh
  rg -n "Stage 7|decides: inferred move|path-dependent.*reject|temporary.*mutable receiver|temporaries.*mutable receiver|all heap values|GC fallback|use-after-move|MayConsume|AlwaysConsumes|reference counting" \
      docs AGENTS.md CONTRIBUTING.md README.md index.md
  ```

- [ ] Create a local migration matrix mapping every legacy monolith heading to one destination file.
- [ ] Mark each legacy section as one of:
  - preserved;
  - preserved but needs strengthening;
  - contradicted by an approved interview decision;
  - superseded by a new lifetime-region section;
  - implementation status only;
  - obsolete wording to remove.
- [ ] Confirm the current grouped-memory file in the repository matches or supersedes the supplied draft.
- [ ] Confirm the current roadmap ordering around grouped memory and full Wasm work.
- [ ] Confirm the current progress matrix contains no lifetime-region/group row.
- [ ] Record current compiler drift listed in Appendix A without editing code.

## Acceptance criteria

- [ ] Every legacy monolith heading has an explicit destination.
- [ ] Every current memory file has a planned action.
- [ ] No unknown migration gap remains.
- [ ] The current repository SHA and search results are recorded.
- [ ] No canonical documentation content has yet been deleted.

## Validation

No build is required if only the plan file was added. Inspect the plan diff and confirm it is documentation-only.

---

# Phase 1 — Add the canonical lifetime-region and escape-validation leaf

## Context and reasoning

The existing split cannot become complete without a dedicated owner for lifetime topology. Borrow validation owns access conflicts and optional transfer safety; it must not silently absorb the separate questions of allocation ownership, retained-edge legality, region widening, cycles, and cross-boundary escape.

This phase creates the new leaf before other pages link to it.

## Files

Create:

```text
docs/src/docs/codebase/memory-management/lifetime-regions-and-escape-validation/#page.bst
docs/src/docs/codebase/memory-management/lifetime-regions-and-escape-validation/overview.bd
docs/src/docs/codebase/memory-management/lifetime-regions-and-escape-validation/lifetime-regions-and-escape-validation.bd
```

## Tasks

### Short contributor overview

- [ ] Create `overview.bd` using the established short-leaf pattern.
- [ ] Use this title:

  ```text
  # Lifetime regions and escape validation
  ```

- [ ] Use this one-line description:

  ```text
  One lifetime owner, retained-edge legality, escape validation and region topology.
  ```

- [ ] Add a `## Contract` section with these exact ownership boundaries:
  - Input: validated HIR, borrow facts, function lifetime/effect summaries, external boundary profiles, reactive observability facts, and builder-supplied lifecycle roots.
  - Output: immutable lifetime-region, allocation-family, retained-edge, escape, cycle, and outlives facts.
  - Decides: semantic lifetime ownership and topology legality.
  - Must not decide: source access syntax, HIR shape, target partition, physical allocation strategy, ownership-bit encoding, or backend lowering.
  - Invariant: GC may realise only a topology already proven legal.
- [ ] Do not invent a Rust implementation path. State clearly that implementation is deferred and that the canonical design currently lives in this leaf plus `docs/roadmap/plans/grouped-memory-design.md`.
- [ ] Add links to borrow validation, ownership and drops, runtime lowering, compiler architecture, build-system lifecycle roots, and grouped-memory design.

### Detailed canonical file

- [ ] Begin the detailed file with the mandatory reference-semantics sentence.
- [ ] Add a `## Design contract` section that distinguishes mandatory topology proof from optional ownership optimisation.
- [ ] Add a `## Terminology` table containing at least:
  - allocation;
  - allocation family;
  - lifetime owner;
  - lifetime region;
  - lexical scope;
  - retained reference;
  - retained edge;
  - outlives relation;
  - implicit region;
  - declared group;
  - builder lifecycle root;
  - physical representation.
- [ ] Define an allocation family as the containing allocation plus projections that remain rooted in it.
- [ ] Add `## One semantic lifetime owner`.
- [ ] Add `## Stored-edge outlives rule` with `R_value >= R_container`.
- [ ] Add valid and invalid edge examples.
- [ ] Add `## Shared aliases may escape lexical bindings` and state that lexical scope does not define allocation lifetime.
- [ ] Add `## Implicit lifetime inference` with the nearest-existing-ancestor rule.
- [ ] State that widening may follow one ordered owner chain only.
- [ ] State that no lateral promotion is allowed across independently ending sibling regions.
- [ ] List the four legal sibling-sharing remedies from the design ledger.
- [ ] Add `## Fresh destination allocation and final-use transfer`.
- [ ] State that fresh escaping results should allocate directly into the destination lifetime when possible.
- [ ] State that existing values may transfer responsibility only at a proven final use.
- [ ] Add `## Interior projections and allocation families`.
- [ ] State that projections may keep the containing family alive and never silently become copies.
- [ ] Add `## Returns and multi-return aliasing`.
- [ ] Cover fresh results, parameter aliases, projection aliases, result-to-result aliases, retained-parameter constraints, and one caller lifetime owner.
- [ ] Add `## Cycles and strongly connected graphs`.
- [ ] State that cross-region cycles are invalid and every SCC belongs to one region.
- [ ] State that direct source construction of cyclic reference graphs remains deferred.
- [ ] Add `## Reactive and builder-owned lifetimes`.
- [ ] Distinguish read-only subscription facts from page/mount/request/frame lifetime roots.
- [ ] State that builders supply lifecycle roots but cannot change source legality.
- [ ] Add `## External lifetime boundaries`.
- [ ] Summarize the WIT value-only and host-binding profiles and link runtime/build/language authorities for detail.
- [ ] Add `## Analysis boundary`.
- [ ] Describe both local per-function summary production and project/link topology instantiation.
- [ ] State that facts are side tables and HIR is unchanged.
- [ ] Add `## Diagnostics are part of the model`.
- [ ] Include the proven-invalid versus not-proven distinction.
- [ ] Include required payload fields and ranked remedies.
- [ ] State why this diagnostic quality supports the decision not to expose source-level RC.
- [ ] Add `## Conservative precision`.
- [ ] State that stronger future analysis may prove more legal programs or narrower owners, but cannot change the meaning of already-accepted programs.
- [ ] Add `## What this analysis must not do`.
- [ ] Add `## Common mistakes` with at least:
  - treating GC as a legality escape;
  - treating every alias as another owner;
  - treating ownership bit as observer count;
  - widening laterally;
  - detaching projections from bases;
  - allowing cross-region cycles;
  - folding topology into `TypeId`;
  - making backend/profile choice affect validity.

### Route wrapper

- [ ] Create `#page.bst` following the existing leaf wrapper pattern.
- [ ] Import the detailed file as `reference_content`.
- [ ] Use page title `Lifetime regions and escape validation`.
- [ ] Use page description `How Beanstalk validates lifetime owners, retained references, escapes and region topology.`
- [ ] Add the breadcrumb back to memory management.
- [ ] Add a short opening sentence stating that every allocation has one semantic lifetime owner and GC cannot bypass topology validation.
- [ ] Render the detailed content in a final section.

## Acceptance criteria

- [ ] The new route has no placeholder content.
- [ ] The new leaf is explicitly canonical for lifetime topology.
- [ ] The leaf does not claim implementation exists.
- [ ] The leaf does not assign topology legality to borrow validation.
- [ ] The leaf does not invent a numbered Stage 7.
- [ ] The leaf fully encodes every lifetime-topology interview decision.
- [ ] The WIT and host profiles are represented without duplicating all runtime/build detail.

## Validation

- [ ] Run `bean check docs` or the Cargo equivalent.
- [ ] Inspect the new route source and ensure all internal links resolve.

---

# Phase 2 — Make the memory overview and website landing page authoritative

## Context and reasoning

The top-level overview is the entry point for contributors and the landing page is the entry point for readers. Both must teach the same model and route each task to the correct owner. The current four-step pipeline omits lifetime validation and build-owned target planning.

## Files

```text
docs/src/docs/codebase/memory-management/overview.bd
docs/src/docs/codebase/memory-management/#page.bst
docs/src/docs/codebase/overview.bd
```

## Tasks

### Canonical overview

- [ ] Replace the opening with an explicit authority declaration:
  - this directory is the single source of truth for accepted program memory semantics;
  - compiler overview owns stage/artefact architecture;
  - build-system design owns project/link/builder lifecycle orchestration;
  - language authorities own syntax;
  - progress owns current support;
  - roadmap owns sequencing.
- [ ] Insert the mandatory reference-semantics sentence immediately after the authority statement.
- [ ] Add `## Two proof layers` with mandatory legality proof versus optional optimisation proof.
- [ ] Replace any implication that “ownership unavailable” and “topology unproven” are the same fallback case.
- [ ] Expand `Rules every contributor must know` to cover:
  1. reference semantics;
  2. explicit copy;
  3. explicit exclusive access;
  4. optional inferred transfer;
  5. mandatory borrow validation;
  6. one lifetime owner and stored-edge legality;
  7. mandatory lifetime-region validation;
  8. optional backend representation and GC fallback for optimisation only.
- [ ] Add a short `## Goals and tradeoffs` section:
  - unconditional memory safety;
  - predictable source model;
  - stronger proof improves performance;
  - conservative topology analysis may reject unproven structures;
  - optional ownership proof may fall back without rejection;
  - runtime checks/collection may remain where optimisation is unavailable.
- [ ] Replace the pipeline with:

  ```text
  source access and copy rules
      -> AST access-mode, placement and freshness validation
      -> validated HIR with explicit places and control flow
      -> borrow validation and optional transfer facts
      -> lifetime-region and escape validation
      -> build-owned reachability, lifecycle roots and target planning
      -> GC, region, ownership-aware or other validated backend lowering
  ```

- [ ] Explain local lifetime-summary production versus project/link topology validation directly below the pipeline.
- [ ] Add a task-reading-guide row for:
  - returns and escaping aliases;
  - regions and groups;
  - reactive lifetime roots;
  - cross-region cycles;
  - external retention.
- [ ] Link that row to the new lifetime leaf.
- [ ] Update JS/Wasm/runtime task rows to include the lifetime leaf when retention or external boundaries are involved.
- [ ] Expand hard invariants with:
  - one owner;
  - GC cannot bypass topology;
  - build profile parity;
  - ownership bit is responsibility only;
  - backends receive validated topology.

### Website landing page

- [ ] Add the mandatory reference-semantics sentence near the top.
- [ ] Keep the existing shared-versus-copy example, but ensure it demonstrates last-potential-use alias activity rather than lexical lifetime.
- [ ] Add a concise `## Lifetime owners` section.
- [ ] State the stored-edge outlives rule in plain language.
- [ ] State that the compiler rejects illegal sibling sharing/cycles even under GC.
- [ ] Replace the pipeline with the same stages used by the canonical overview.
- [ ] Add the new lifetime page to `## Detailed pages` between borrow validation and ownership/drops.
- [ ] Keep `## Current support` pointing to the progress matrix.
- [ ] Do not present `group` / `into` as currently available syntax on the landing page; link the detailed lifetime page for accepted deferred direction.

### Codebase overview

- [ ] Add or update the memory-management summary so it names the new lifetime leaf and the canonical reference-semantic sentence.
- [ ] Keep this page navigational; do not duplicate the detailed model.

## Acceptance criteria

- [ ] Contributor overview and website landing page teach the same pipeline.
- [ ] The reference-semantic sentence is visible without opening a leaf.
- [ ] The difference between legality fallback and optimisation fallback is explicit.
- [ ] The new leaf is linked and correctly ordered.
- [ ] No current-support claim is added outside the progress matrix.

## Validation

- [ ] Run docs-source checking.
- [ ] Inspect the memory landing route after generation in the final phase.

---

# Phase 3 — Clarify access, aliases, aggregate storage, and copy

## Context and reasoning

The access leaf owns the most frequently misunderstood part of the model. It must make clear that Beanstalk omits reference syntax rather than reference semantics. It must also resolve mutable aliasing, non-lexical activity, aggregate storage, and graph-level copy semantics.

## Files

```text
docs/src/docs/codebase/memory-management/access-and-aliasing/overview.bd
docs/src/docs/codebase/memory-management/access-and-aliasing/access-and-aliasing.bd
```

Language synchronization is deferred to Phase 8 so this phase remains focused.

## Tasks

### Short overview

- [ ] Add the mandatory reference-semantics sentence.
- [ ] Update the contract so it decides source access/copy/alias semantics but not topology validation or backend ownership.
- [ ] Add `src/compiler_frontend/value_mode.rs` as a current navigation aid only, not design authority.
- [ ] Link the new lifetime leaf for storage and escape questions.

### Detailed access leaf

- [ ] Put the mandatory reference-semantics sentence in the design contract.
- [ ] Expand vocabulary with:
  - shared alias;
  - mutable alias;
  - retained reference;
  - independent copy;
  - allocation family.
- [ ] Replace wording that treats “reference” as merely explanatory vocabulary. State instead:

  ```text
  Reference relationships are real source semantics. Beanstalk does not expose reference type constructors or lifetime syntax.
  ```

- [ ] Add `## Alias activity and last potential use`.
- [ ] Define non-lexical activity for unused aliases, branches, joins, and loops.
- [ ] Add an example where a shared alias's final use occurs before later mutation.
- [ ] Add `## Mutable aliases versus mutable slots`.
- [ ] Define:
  - `alias ~= source` as exclusive write-through alias;
  - a mutable fresh initializer as independent slot;
  - assignment through alias as referent mutation;
  - no silent rebinding/detachment.
- [ ] Correct the receiver section:
  - mutable receivers require existing mutable places;
  - temporaries and rvalues cannot be mutable receivers;
  - fresh-rvalue handling applies only to ordinary mutable parameters.
- [ ] Replace aggregate wording with the approved shared-by-default rule.
- [ ] Add examples for:
  - two aggregate fields sharing one allocation;
  - explicit copies for independent children;
  - final-use transfer as lowering only.
- [ ] State explicitly that maps retain shared keys/values by default under the same rule as all aggregates.
- [ ] State that map entry structure belongs to the map while child allocation ownership follows lifetime topology.
- [ ] Expand `## Explicit copies` into the full graph-level contract:
  - deep copy;
  - internal alias preservation;
  - same-region cycle preservation;
  - no mutable sharing with source;
  - scalar physical reuse only when unobservable;
  - reactive snapshot semantics;
  - external non-copyability by default;
  - destination-region allocation.
- [ ] Add a non-copyable-resource example and diagnostic expectation.
- [ ] Preserve the no-source-level-reference/lifetime/move list, but precede it with the statement that references still exist semantically.
- [ ] Update common mistakes to include:
  - saying Beanstalk has no references;
  - treating aggregate insertion as implicit move;
  - treating scalar representation as source copy semantics;
  - treating mutable alias assignment as rebinding;
  - using lexical scope as alias lifetime;
  - assuming `copy` may preserve mutable sharing.
- [ ] Link the lifetime leaf from aggregate, return, and retained-reference sections.

## Acceptance criteria

- [ ] No sentence implies aggregate storage always takes ownership.
- [ ] No sentence implies scalar values have a distinct source copy rule.
- [ ] Mutable alias semantics are explicit.
- [ ] Mutable receiver semantics match the language authority.
- [ ] Non-lexical alias activity is explicit.
- [ ] `copy` has one complete semantic definition.
- [ ] The leaf routes topology questions to the new owner.

## Validation

- [ ] Run docs-source checking.
- [ ] Search the access leaf for the banned wording in Section 9.

---

# Phase 4 — Correct borrow validation and inferred-transfer semantics

## Context and reasoning

Borrow validation must remain the access-safety and optional-transfer-safety owner. It must not reject a program merely because an optimisation is unavailable, and it must not absorb lifetime topology. This phase corrects the current ambiguity around path-dependent moves, use-after-move, parameter eligibility, and analysis outputs.

## Files

```text
docs/src/docs/codebase/memory-management/borrow-validation/overview.bd
docs/src/docs/codebase/memory-management/borrow-validation/borrow-validation.bd
```

## Tasks

### Short overview

- [ ] Update the contract:
  - Input: validated HIR, access/effect metadata, and external boundary classifications.
  - Output: read-only access, alias, optional-transfer, reactivity, and advisory-drop facts.
  - Decides: shared/exclusive conflicts and whether transfer is safe as an optimisation.
  - Must not decide: lifetime topology, physical ownership, final allocation, or backend lowering.
- [ ] Replace “use-after-move safety” wording with “optional transfer safety and no-later-use proof.”
- [ ] Link the lifetime leaf as the next stage for escapes and retained edges.

### Detailed borrow leaf

- [ ] Keep validated HIR as the input authority.
- [ ] Add a sentence that recoverable numeric, cast, and fallible-operation paths are already explicit HIR branches/locals/exits before borrow validation.
- [ ] Add `## Alias activity` and define last-potential-use behaviour.
- [ ] Preserve branch and loop fixed-point analysis.
- [ ] Rewrite `## Future-use and move safety` so classifications mean:
  - no future use on every relevant path: transfer is eligible;
  - future use: borrow;
  - path-dependent or imprecise: borrow conservatively.
- [ ] Remove any statement that path-dependent optional transfer may reject the program.
- [ ] State explicitly that transfer proof failure is not a source diagnostic.
- [ ] Clarify that all runtime value parameters, immutable or mutable, may be eligible for inferred responsibility transfer.
- [ ] Explain that parameter access mode remains separate from optional ownership effect.
- [ ] Clarify function summaries:
  - access mode;
  - mutation effects;
  - transfer eligibility/effect category;
  - return aliases and projections;
  - reactive effects;
  - lifetime constraints are produced/validated by the separate lifetime analysis.
- [ ] State that a `MayConsume` summary means the call path can receive responsibility when the caller proves a safe transfer; it does not let a callee unpredictably invalidate a borrowed argument.
- [ ] Add a section `## No ordinary mandatory-consuming operation`.
- [ ] State that current source syntax has no mandatory-consuming ordinary-value operation.
- [ ] State that user-facing use-after-consumption is reserved for a future accepted mandatory-consuming surface.
- [ ] Preserve internal inconsistent-analysis errors as `CompilerError`.
- [ ] Update side-table outputs to distinguish:
  - source access facts;
  - transfer eligibility facts;
  - return alias facts;
  - reactive invalidation facts;
  - advisory drop candidates.
- [ ] State that advisory drop candidates are not lifetime topology and not exact release permission.
- [ ] Add reactivity V1 precision wording:
  - source-level invalidation today;
  - field/item/path granularity is future precision;
  - subscription observability is consumed by lifetime validation.
- [ ] Add `## Handoff to lifetime validation` describing the facts supplied to the new analysis.
- [ ] Update diagnostics:
  - access conflicts remain user diagnostics;
  - lack of optional move proof is not a diagnostic;
  - impossible summary/state inconsistency is internal.
- [ ] Update “must not” and common mistakes lists:
  - must not decide topology;
  - must not reject because optional transfer is unavailable;
  - must not restrict transfer eligibility to mutable parameters;
  - must not treat GC as permission to skip checks.

## Acceptance criteria

- [ ] Path-dependent optional transfer always backs off to borrowing in the accepted design.
- [ ] Borrow validation no longer owns lifetime topology.
- [ ] All parameter modes can be ownership-transfer eligible.
- [ ] Current source has no mandatory-consuming ordinary operation.
- [ ] Advisory drops are clearly weaker than validated release permission.
- [ ] Reactive invalidation and reactive lifetime are separated.

## Validation

- [ ] Run docs-source checking.
- [ ] Search the borrow leaf for `reject inconsistency`, `use-after-move`, and mutable-only consumption wording; every remaining occurrence must be intentionally qualified.

---

# Phase 5 — Correct ownership, the unified ABI, and conditional destruction

## Context and reasoning

Ownership is an optimisation state and runtime responsibility, not source type identity or lifetime legality. The current short ownership overview incorrectly says the lowering layer decides inferred move points. This phase fixes that boundary and integrates region-owned values.

## Files

```text
docs/src/docs/codebase/memory-management/ownership-and-drops/overview.bd
docs/src/docs/codebase/memory-management/ownership-and-drops/ownership-and-drops.bd
```

## Tasks

### Short overview

- [ ] Replace `Decides: inferred move points` with:

  ```text
  Decides: runtime realisation of validated transfer facts, ownership metadata encoding, unified ABI behaviour, and conditional destruction.
  ```

- [ ] State that source legality and lifetime topology are already fixed.
- [ ] Add lifetime facts to the input list.
- [ ] Link the lifetime leaf before runtime lowering.

### Detailed ownership leaf

- [ ] Preserve ownership as an optimisation state separate from `TypeId`.
- [ ] Add the mandatory ownership-bit wording from Section 8.7.
- [ ] Rewrite inferred transfer so the analysis proves eligibility and lowering only realises it.
- [ ] State that operations remain borrowed when proof is unavailable.
- [ ] Remove any implication that a later valid use can be rejected because the compiler chose an optional transfer.
- [ ] Clarify immutable and mutable parameter ownership cases.
- [ ] Clarify group-owned values:
  - group owns release responsibility;
  - ordinary calls normally borrow group-owned storage;
  - an individual callee cannot release one group child merely because an owned bit exists;
  - hidden destination allocation may produce fresh results directly into a group.
- [ ] Add `## Ownership responsibility versus lifetime ownership`.
- [ ] Contrast:
  - lifetime owner;
  - destruction-responsibility path;
  - shared aliases;
  - region-owned allocation family.
- [ ] Add `## Conditions for release`:
  - valid topology;
  - no observable alias remains;
  - responsibility is owned;
  - the allocation is individually releasable or its owning region is ending.
- [ ] Clarify `drop_if_owned`:
  - conditional responsibility check only;
  - not an alias count;
  - not a topology validator;
  - no-op or erased under GC where allowed;
  - region-owned values may use grouped release rather than individual drop.
- [ ] Update control-flow exit wording so joins create possible responsibility but are not themselves automatically release sites.
- [ ] Preserve exact exit classes: normal, return, error return, break, region/group exit, transfer-out.
- [ ] Update aggregate destruction:
  - aggregates may contain shared and independently owned children;
  - recursive destruction follows validated child responsibility/topology;
  - borrowed children are never destroyed through the aggregate.
- [ ] Clarify specialisation categories:
  - analysis/lowering facts only;
  - source API never gains mandatory consuming variants;
  - `AlwaysConsumes` is allowed only after call-path proof.
- [ ] Add internal RC to allowed physical representations, explicitly not a source contract.
- [ ] Update common mistakes with ownership-bit and region distinctions.

## Acceptance criteria

- [ ] Ownership lowering does not decide source moves.
- [ ] Ownership bit is responsibility only.
- [ ] Conditional drop is not portrayed as complete lifetime proof.
- [ ] Group-owned and individually owned storage are distinguished.
- [ ] No source-visible consuming API is introduced.

## Validation

- [ ] Run docs-source checking.
- [ ] Search for `decides: inferred move` and remove every stale occurrence.

---

# Phase 6 — Correct runtime/backend lowering and external boundaries

## Context and reasoning

Backend flexibility is allowed only after source access and lifetime topology are validated. This phase fixes the missing build/link inputs, clarifies GC as representation rather than legality, separates Beanstalk page memory from imported components, and records closed external boundary profiles.

## Files

```text
docs/src/docs/codebase/memory-management/runtime-and-backend-lowering/overview.bd
docs/src/docs/codebase/memory-management/runtime-and-backend-lowering/runtime-and-backend-lowering.bd
```

## Tasks

### Short overview

- [ ] Expand inputs to include:
  - validated HIR;
  - borrow facts;
  - validated lifetime topology;
  - selected function set;
  - target assignment;
  - import/export/capability plan;
  - semantic layout identity;
  - builder lifecycle/runtime plan.
- [ ] State that the backend must not decide source legality, borrow facts, region legality, target partition, or page lifecycle policy.
- [ ] Replace the nonexistent Stage 7 reference with links to:
  - Stage 6 borrow validation;
  - the unnumbered lifetime-region analysis boundary;
  - backend-facing compiler handoff;
  - build-system mixed-target planning and runtime/memory sections.

### Detailed runtime leaf

- [ ] Add the canonical GC/legality wording.
- [ ] Define GC baseline precisely:
  - deterministic destruction is not required for correctness;
  - valid topology may be represented by host/runtime GC;
  - mandatory access and lifetime validation still runs;
  - optional ownership operations may be erased.
- [ ] State that internal RC, arenas, handles, regions, stack storage, pools, and hybrid representations are permitted for valid topology.
- [ ] State that source-visible RC and hidden unrestricted dynamic sharing are not permitted.
- [ ] Update JS lowering:
  - host references realise validated aliases;
  - JS does not weaken topology;
  - explicit copy remains graph-independent semantically even if current implementation lags;
  - reactive mounts follow builder lifecycle roots.
- [ ] Update Wasm lowering:
  - borrow and lifetime facts precede LIR;
  - ownership bit is responsibility only;
  - group/region representation is separate;
  - missing optimisation proof may retain GC;
  - missing topology proof is not a backend fallback.
- [ ] Add `## Build-owned target and lifecycle planning`.
- [ ] State that the build system owns reachability, target partition, page/request/mount/frame roots, and page runtime topology.
- [ ] State that backends consume explicit plans rather than selecting them.
- [ ] Add `## Beanstalk-linked page memory`:
  - each page owns one runtime instance and one memory shared by its linked Beanstalk Wasm variants;
  - linked variants import the page runtime;
  - this is build architecture, not source memory semantics.
- [ ] Add `## Imported Wasm components`:
  - component-private memory/runtime;
  - WIT value conversion only;
  - no membership in Beanstalk page memory or lifetime graph;
  - no GC declaration required.
- [ ] Add the complete `## WIT value-only V1 profile` contract.
- [ ] List supported value-shape direction without claiming current implementation.
- [ ] List rejected V1 surfaces: resources, handles, callbacks, async, futures, streams, shared memory, pointers, aliases, retained refs.
- [ ] Require structured import diagnostics and wrapper-component guidance.
- [ ] Add `## Restricted host-binding profile`.
- [ ] Cover opaque foreign identities and explicit teardown.
- [ ] State that host finalization timing cannot define behaviour.
- [ ] Add `## Deferred resource-capable profiles` with the required future design domains.
- [ ] Update backend-fact consumption to include lifetime topology and builder lifecycle roots.
- [ ] Update “must not” and common mistakes lists:
  - no Stage 7;
  - no source legality in backend;
  - no page-memory assumption for arbitrary components;
  - no unknown retention;
  - no GC legality escape.

## Acceptance criteria

- [ ] Runtime docs contain every required handoff input.
- [ ] GC is clearly representation, not legality.
- [ ] Beanstalk-linked Wasm memory and imported component memory are distinct.
- [ ] WIT and host profiles are closed and explicit.
- [ ] No current implementation claim is made for WIT components or grouped memory.

## Validation

- [ ] Run docs-source checking.
- [ ] Search for `Stage 7` and ensure no stale backend reference remains.

---

# Phase 7 — Finalize the grouped-memory design and roadmap status

## Context and reasoning

The grouped-memory file now contains an accepted direction but still labels sibling persistent sharing unresolved and assigns topology checks to borrow validation. The interview resolved those points. This phase turns the file into a complete accepted design brief while keeping implementation deferred.

## Files

```text
docs/roadmap/plans/grouped-memory-design.md
docs/roadmap/roadmap.md
```

## Tasks

### Status and authority

- [ ] Change the grouped-memory status to:

  ```text
  Status: accepted end-state design; implementation deferred
  Scope: canonical grouped-memory and declared lifetime-region design, not an implementation plan
  ```

- [ ] Remove the self-referential replacement-target line.
- [ ] State that the canonical general topology rules live in the lifetime-region memory leaf and this file owns the accepted `group` / `into` surface.
- [ ] Remove every reference to an unresolved ownership question.

### Reference semantics and topology

- [ ] Insert the mandatory reference-semantics sentence in the relationship section.
- [ ] Replace the remaining sibling-sharing question with the approved nearest-existing-ancestor rule.
- [ ] State that the compiler cannot invent an overly broad owner.
- [ ] State that declared groups are never implicitly widened.
- [ ] Preserve one-owner, outlives, projection, cycle, and builder-root rules.
- [ ] Replace broad “reference counting” non-goal wording with:
  - no source-visible RC or dynamic shared-ownership surface;
  - internal RC remains a valid backend representation of already-legal topology.

### Syntax and lexical ownership

- [ ] Preserve `group name:` and declaration-site `into group` grammar.
- [ ] Add a dedicated `## Binding visibility and destination scope` section.
- [ ] Specify destination-group lexical ownership exactly as approved.
- [ ] State visibility begins at the declaration point, not at the group opening.
- [ ] State collision checks occur in destination scope.
- [ ] State placement may target current or ancestor group only.
- [ ] Preserve V1 exclusions for expression placement, reassignment placement, extraction, and group adoption.

### Copy and fresh results

- [ ] Replace the short copy statement with the complete graph-level copy contract or link the access leaf and summarize all non-obvious rules.
- [ ] Include topology preservation, cycles, reactive snapshot, external non-copyability, and destination allocation.
- [ ] Preserve hidden fresh-result destination direction.
- [ ] State that hidden destinations are not source lifetime parameters.

### Analysis ownership

- [ ] Replace `Borrow validation owns group safety and mandatory lifetime topology checks` with:
  - borrow validation owns access conflicts and optional transfer safety;
  - lifetime-region and escape validation owns group escape, retained-edge, outlives, cycle, and common-owner legality.
- [ ] Add local summary and project/link validation responsibilities.
- [ ] Preserve HIR structural metadata and side-table ownership.
- [ ] Do not invent a numbered Stage 7.

### External profiles

- [ ] Replace the unrestricted external metadata list with closed profiles.
- [ ] Add the WIT value-only V1 profile.
- [ ] Add the restricted host-binding profile.
- [ ] State that richer resource profiles are deferred.
- [ ] State that group-owned values cannot cross a value-only boundary as aliases; value conversion creates independent foreign values.

### Diagnostics

- [ ] Expand diagnostics to the approved proven-invalid/not-proven distinction.
- [ ] Add required payload fields and remedy ranking.
- [ ] State explicitly that diagnostic quality is part of the rationale for no source RC.
- [ ] Keep diagnostic family names conceptual unless the compiler diagnostic registry already owns exact names.

### Final decisions and roadmap order

- [ ] Update `## Final design decisions` with every approved interview decision.
- [ ] Remove `## Remaining design question` entirely.
- [ ] Keep implementation roadmap phases as future work, but update them to include the distinct lifetime analysis.
- [ ] In `docs/roadmap/roadmap.md`:
  - keep grouped memory deferred until an implementation plan is approved;
  - state that it is a likely prerequisite to final ownership-aware Wasm completion;
  - do not mark it active or queued merely because design is accepted;
  - remove stale wording implying debug and release may differ in legality;
  - state profiles differ only in optimisation effort.

## Acceptance criteria

- [ ] The grouped-memory document has no unresolved question.
- [ ] The document is accepted design, not an implementation plan or current-support claim.
- [ ] Stage ownership matches the new memory leaf.
- [ ] Destination-group lexical visibility is explicit.
- [ ] Source RC versus backend RC is correctly separated.
- [ ] Roadmap sequencing is informative but not falsely active.

## Validation

- [ ] Run docs-source checking.
- [ ] Search the grouped plan for `remaining design question`, `unresolved`, and `borrow validation owns group safety`; no stale instance may remain.

---

# Phase 8 — Synchronize compiler and build-system architecture

## Context and reasoning

The memory docs cannot be canonical while compiler and build-system authorities describe a different pipeline. This phase updates stage boundaries, artefact lanes, public summaries, project-level validation, and lifecycle roots without inventing implementation types.

## Files

```text
docs/compiler-design-overview.md
docs/build-system-design.md
```

## Tasks

### Compiler architecture

- [ ] Add lifetime-region facts to `ModuleExecutable` conceptually, beside HIR and borrow facts.
- [ ] Add exported lifetime/effect summaries to `PublicSemanticInterface`:
  - fresh result;
  - parameter/result aliasing;
  - projection aliases;
  - result-to-result aliases;
  - retained-parameter relationships;
  - outlives constraints;
  - external boundary profile.
- [ ] Keep process-local region IDs out of cross-module semantic identity.
- [ ] Add lifetime summaries/facts to generated function sidecars.
- [ ] Update the architectural invariants:
  - lifetime validation is mandatory and backend-independent;
  - GC cannot bypass topology;
  - ownership optimisation preserves accepted programs.
- [ ] Update Stage 4/AST ownership for accepted deferred group syntax:
  - parser/scope/placement/freshness when implemented;
  - no `TypeId` group identity;
  - clearly label implementation deferred.
- [ ] Update Stage 5/HIR future contract:
  - explicit group metadata and exits when implemented;
  - all checked failure paths explicit before memory analyses;
  - HIR still does not decide exact lifetime topology.
- [ ] Keep Stage 6 as borrow validation.
- [ ] Rewrite Stage 6 move wording to optional transfer eligibility and fallback.
- [ ] State that immutable and mutable parameters may receive transfer responsibility.
- [ ] Add an unnumbered section immediately after Stage 6:

  ```text
  ## Lifetime-region and escape validation
  ```

- [ ] In that section, describe:
  - local per-function/module constraint production;
  - read-only facts;
  - exported summaries;
  - project/link topology instantiation;
  - diagnostics;
  - no HIR rewriting;
  - no physical representation choice.
- [ ] Update target-contract validation so it runs after final lifetime-topology validation.
- [ ] Update backend-facing compiler handoff to include validated lifetime facts and external boundary classifications.
- [ ] Update binding-backed symbols section with conceptual closed boundary classification, without fixing exact Rust enum names.
- [ ] State that missing compiler-owned boundary classification is `CompilerError`; unsupported source-selected interface features are structured diagnostics.

### Build-system architecture

- [ ] Update the fixed bootstrap/compile-wave sequence:

  ```text
  AST
  -> validated HIR
  -> borrow validation
  -> local lifetime constraints and summaries
  -> immutable module artefact
  ```

- [ ] Update project/link planning sequence:

  ```text
  entry/package roots
  -> reachable function and effect union
  -> instantiate lifetime constraints with builder lifecycle roots
  -> validate complete lifetime topology
  -> target affinity and deterministic partition
  -> target validation
  -> lowering
  ```

- [ ] State that local module compilation cannot validate every cross-module or builder-lifecycle relationship by itself.
- [ ] Add conceptual project-level validated lifetime topology to `ProjectCompilation` or the link plan, while leaving exact Rust shape open.
- [ ] Define builder-supplied page, mount, request, frame, and arena roots as lifecycle inputs, not builder-specific source-law exceptions.
- [ ] State that builder lifecycles cannot change language validity.
- [ ] Update one-page runtime/memory wording:
  - applies to linked Beanstalk Wasm variants;
  - does not require imported WIT components to share page memory.
- [ ] Add imported component isolation and WIT value conversion to the build/runtime boundary.
- [ ] Add external boundary profile/capability metadata to the builder surface conceptually.
- [ ] Update fingerprint/reuse wording so exported lifetime summaries are part of the public-interface fingerprint and topology-relevant implementation/link facts invalidate affected assemblies.
- [ ] Keep exact persistent encoding deferred.

## Acceptance criteria

- [ ] Compiler and build documents show one consistent lifetime-validation pipeline.
- [ ] No numbered Stage 7 is invented.
- [ ] Module-local and project-level lifetime work are both represented.
- [ ] Target planning occurs after semantic topology validation.
- [ ] Builder lifecycle roots do not change source legality.
- [ ] Imported components are isolated from page memory.
- [ ] Public and generated artefacts carry enough conceptual facts for later implementation.

## Validation

- [ ] Run docs-source checking.
- [ ] Search both authorities for stale `Stage 7`, mutable-only consumption, and GC-legality wording.

---

# Phase 9 — Synchronize language, design-scope, educational, progress, and navigation docs

## Context and reasoning

The canonical memory pages are maintainer-oriented, but the language authority and educational pages are where readers are most likely to misunderstand the absence of explicit reference syntax. This phase propagates concise, status-correct explanations without duplicating the full architecture.

## Files

```text
docs/language-overview.md
docs/src/docs/codebase/design-scope/overview.bd
docs/src/docs/codebase/compiler-design/memory-management-and-gc/memory-management-and-gc.bd
docs/src/docs/codebase/compiler-design/borrow-validation-and-drops/borrow-validation-and-drops.bd
docs/src/docs/codebase/compiler-design/overview.bd
docs/src/docs/progress/#page.bst
docs/src/docs/codebase/overview.bd
index.md
```

Review and edit only if required:

```text
AGENTS.md
CONTRIBUTING.md
README.md
```

## Tasks

### Language overview

- [ ] Add the mandatory reference-semantics sentence to the syntax/memory area.
- [ ] Change the syntax-summary `References` row so it says shared immutable reference semantics are default, not that references are absent.
- [ ] Add a concise memory-semantics section that links canonical memory pages.
- [ ] Define non-lexical shared alias activity.
- [ ] Define mutable aliases versus fresh mutable slots.
- [ ] Correct aggregate storage rules for structs, choices, collections, and maps.
- [ ] Add the complete `copy` summary and link the detailed access leaf.
- [ ] Clarify that inferred transfer is optional and source has no move syntax or mandatory-consuming ordinary operation.
- [ ] Clarify that all parameter access modes may receive inferred responsibility.
- [ ] Add a clearly labelled `Accepted but deferred: declared memory groups` section.
- [ ] Include:
  - `group name:`;
  - `into group` declaration syntax;
  - destination-group visibility;
  - current/ancestor placement only;
  - no expression/reassignment/extraction/adoption in V1;
  - no group identity in types or signatures;
  - implementation deferred.
- [ ] Add source-visible RC, retain/release, weak ownership, finalizers, and unrestricted dynamic shared ownership to outside-scope wording where appropriate.
- [ ] Clarify that internal backend RC remains allowed.
- [ ] Update external package imports with:
  - future WIT value-only component profile;
  - current restricted host-binding profile;
  - opaque handle semantics;
  - explicit teardown;
  - deferred resources/callbacks/async/shared memory.
- [ ] Ensure current syntax examples do not imply `group` / `into` is implemented.

### Design-scope overview

- [ ] Add accepted declared lifetime groups as a narrow mechanism.
- [ ] Add source-level RC/dynamic shared ownership as outside scope.
- [ ] State that diagnostics plus groups/copy/common-owner patterns are the intended replacement.
- [ ] Keep this concise and link the canonical memory leaf.

### Educational compiler pages

- [ ] Update `memory-management-and-gc`:
  - reference semantics sentence;
  - GC representation versus topology legality;
  - two proof layers;
  - lifetime validation in pipeline;
  - avoid claiming GC accepts any graph.
- [ ] Update `borrow-validation-and-drops`:
  - optional transfer backs off;
  - no mandatory-consuming ordinary operation;
  - lifetime topology is a separate analysis;
  - ownership bit is responsibility only;
  - update exit-state handoff to lifetime validation before module artefacts/target planning.
- [ ] Update compiler-design overview pipeline and page links to include lifetime validation.
- [ ] Keep these pages educational and link the canonical memory leaves rather than restating the complete group design.

### Progress matrix

- [ ] Keep the current borrow validation row focused on actual current behaviour.
- [ ] Add a note that current move-sensitive and aggregate behaviour may not yet match the accepted final memory design and is tracked as compiler drift.
- [ ] Do not falsely mark accepted future semantics as supported.
- [ ] Add a separate row:

  ```text
  Surface: Lifetime-region and escape validation
  Status: Deferred
  Coverage: None
  Runtime target: Backend-neutral design
  Notes: Accepted architecture exists; no implementation owner has landed. GC does not bypass the future semantic validation.
  ```

- [ ] Add a separate row or a clearly distinct note for:

  ```text
  Surface: Declared memory groups (`group` / `into`)
  Status: Deferred
  Coverage: None
  Runtime target: Frontend / HIR / lifetime analysis / all backends
  Notes: Accepted end-state syntax; not current source support.
  ```

- [ ] Add future WIT value-only component integration as deferred if no existing row owns it.
- [ ] Do not change unrelated status rows.

### Navigation and contributor references

- [ ] Update `index.md` to link:

  ```text
  docs/src/docs/codebase/memory-management/overview.bd
  ```

  instead of the legacy monolith.
- [ ] Update the codebase overview to include the new lifetime leaf.
- [ ] Search `AGENTS.md`, `CONTRIBUTING.md`, and `README.md` for legacy paths or contradictory wording.
- [ ] Edit those files only when required to point to the canonical split docs.
- [ ] Keep website URLs that already target the memory landing page.

## Acceptance criteria

- [ ] The language authority cannot be read as “Beanstalk has no references.”
- [ ] Deferred syntax is clearly labelled and not mixed with current-valid examples.
- [ ] Educational pages teach the same stage boundaries as canonical architecture.
- [ ] Progress matrix separates current borrow support from deferred topology/groups.
- [ ] `index.md` no longer points at the legacy file.
- [ ] No unrelated roadmap or language churn is introduced.

## Validation

- [ ] Run docs-source checking.
- [ ] Search all changed files for the banned wording in Section 9.

---

# Phase 10 — Final migration audit, legacy retirement, and documentation release build

## Context and reasoning

The old monolith must remain until the split pages, cross-authorities, navigation, and status docs are complete. This final phase proves coverage, removes the duplicate authority, rebuilds generated documentation, and performs the required manual inspection.

## Files

Delete:

```text
docs/memory-management-design.md
```

Generated by the docs build only:

```text
docs/release/**
```

## Tasks

### Coverage audit

- [ ] Re-read the entire legacy monolith one final time.
- [ ] Compare every legacy heading against the Phase 0 migration matrix.
- [ ] Confirm every accepted technical detail is now present in a canonical destination or explicitly superseded by an approved decision.
- [ ] Confirm the following formerly legacy-only points are preserved:
  - memory safety independent of optimisation strength;
  - checked failure paths explicit in HIR before memory analysis;
  - map key/value rules;
  - reactivity precision boundaries;
  - ownership specialisation categories;
  - control-flow exit handling;
  - backend fallback requirements;
  - design tradeoffs and extension points.
- [ ] Confirm all newly approved design areas are present:
  - reference-semantic wording;
  - non-lexical activity;
  - mutable aliases;
  - aggregate shared storage;
  - graph copy;
  - one-owner topology;
  - nearest ancestor;
  - no lateral promotion;
  - cycles;
  - separate lifetime analysis;
  - diagnostics;
  - groups;
  - external profiles.

### Contradiction search

- [ ] Run:

  ```sh
  rg -n "Stage 7|decides: inferred move|path-dependent.*reject|temporar.*mutable receiver|all heap values are managed|unknown retention|aggregates own stored values|ownership bit.*unique|reference counting" \
      docs AGENTS.md CONTRIBUTING.md README.md index.md
  ```

- [ ] Review every result manually. Keep only intentionally qualified occurrences.
- [ ] Search for legacy paths:

  ```sh
  rg -n "memory-management-design|memory-management-overview|Beanstalk Memory Management Strategy|Legacy consolidated reference" \
      . --glob '!docs/release/**'
  ```

- [ ] Update every live source reference before deletion.

### Delete the monolith

- [ ] Delete `docs/memory-management-design.md`.
- [ ] Confirm no source link points to the deleted file.
- [ ] Confirm the new split overview is the only canonical memory entry in navigation and contributor references.

### Documentation build

- [ ] Run exactly one required documentation-only final gate:

  ```sh
  bean build docs --release
  ```

  or, when no suitable release binary is available:

  ```sh
  cargo run --quiet -- build docs --release
  ```

- [ ] Do not run `just validate`, Clippy, unit tests, integration tests, or benchmark checks for this documentation-only slice.
- [ ] Confirm the changed-file list contains documentation only.
- [ ] Confirm generated output resulted from source changes and was not edited directly.

### Manual route inspection

Inspect at minimum:

- [ ] `/docs/codebase/memory-management/`
- [ ] `/docs/codebase/memory-management/access-and-aliasing/`
- [ ] `/docs/codebase/memory-management/borrow-validation/`
- [ ] `/docs/codebase/memory-management/lifetime-regions-and-escape-validation/`
- [ ] `/docs/codebase/memory-management/ownership-and-drops/`
- [ ] `/docs/codebase/memory-management/runtime-and-backend-lowering/`
- [ ] `/docs/codebase/compiler-design/memory-management-and-gc/`
- [ ] `/docs/codebase/compiler-design/borrow-validation-and-drops/`
- [ ] `/docs/progress/`

For each route:

- [ ] Check title and description.
- [ ] Check breadcrumbs.
- [ ] Check internal and GitHub links.
- [ ] Check tables and code blocks.
- [ ] Check that Beandown renders special characters and arrows correctly.
- [ ] Check that deferred syntax is labelled.
- [ ] Check that current-support links point to the progress matrix.
- [ ] Check that no deleted legacy link remains.

### Final architecture audit

- [ ] One semantic owner exists for every topic.
- [ ] Borrow and lifetime analyses have distinct contracts.
- [ ] Compiler and build ownership remain clear.
- [ ] No backend is allowed to infer source legality.
- [ ] No duplicate canonical design text remains.
- [ ] No unresolved design question remains.
- [ ] No implementation status is presented as accepted architecture or vice versa.
- [ ] The progress matrix remains accurate.
- [ ] The roadmap remains accurate.
- [ ] The final report states exactly which command ran and which routes were inspected.

## Acceptance criteria

- [ ] Legacy monolith deleted.
- [ ] Repository-wide legacy-path search clean.
- [ ] Documentation release build passes.
- [ ] All changed routes inspected.
- [ ] Generated diff reviewed.
- [ ] Documentation-only changed-file requirement satisfied.
- [ ] No compiler behaviour was changed.

---

## 11. Required final report format

The implementing agent's final report must contain:

### Documentation changes

- Files created.
- Files materially revised.
- File deleted.
- New canonical topic ownership.

### Design decisions encoded

- Reference semantics.
- Optional inferred transfer.
- Mandatory lifetime topology.
- Grouped memory.
- Copy graph semantics.
- External boundary profiles.
- Diagnostic contract.

### Current implementation drift deliberately not fixed

List every drift item discovered or retained, with source paths when known.

### Validation

State the exact release-build command and result.

### Manual inspection

List every route inspected.

### Remaining work

State that compiler changes require a separate approved implementation plan. Do not describe the documentation migration as implementing memory groups, topology validation, WIT components, or ownership-aware Wasm.

---

## Appendix A — Compiler drift ledger for the later implementation plan

This appendix records known mismatches only. It is not permission to change code in this plan.

- [ ] Aggregate literal transfer currently forces non-scalar place children to move or diagnoses, rather than preserving shared storage by default.
- [ ] Scalar aggregate children currently receive an implicit-copy exception that conflicts with the uniform source model.
- [ ] Current user-function call semantics may restrict `MayConsume` to mutable parameters rather than all runtime values.
- [ ] Current path-dependent move handling may reject where accepted semantics require borrow fallback.
- [ ] Current use-after-move diagnostics may reflect optional compiler choices rather than mandatory source consumption.
- [ ] Current JavaScript deep-copy helper recursively clones but may not preserve repeated-reference topology or cycles without memoisation.
- [ ] Current mutable-alias declaration/write-through behaviour must be audited against the accepted design.
- [ ] Current aggregate runtime representations may assume exclusive child ownership rather than mixed shared/owned children.
- [ ] Current public function summaries do not carry full lifetime, projection, retention, result-to-result alias, or outlives constraints.
- [ ] No distinct lifetime-region and escape-validation analysis exists.
- [ ] No project/link topology instantiation exists for builder lifecycle roots.
- [ ] No accepted `group` / `into` implementation exists.
- [ ] No hidden destination-directed result allocation contract exists.
- [ ] Current ownership bit/runtime scaffolding does not encode complete region/allocation-family facts.
- [ ] Current external package metadata does not expose closed WIT-value versus host-binding semantic profiles.
- [ ] No WIT component import validator enforces the value-only V1 surface.
- [ ] Current progress/test coverage does not cover lifetime-topology diagnostics because the analysis is deferred.

The later compiler-drift plan must refresh and verify every item against the repository before proposing code changes.

---

## Appendix B — Minimum terminology consistency table

| Preferred term | Meaning | Do not substitute with |
|---|---|---|
| shared alias / shared reference | Read-only source-semantic access to existing storage | implicit copy, value duplication |
| mutable alias / exclusive alias | Write-through mutation-capable access to existing storage | owned mutable value, rebinding reference |
| lifetime owner | One semantic region responsible for allocation lifetime | last binding, ownership bit |
| destruction responsibility | Obligation to release or transfer when deterministic release is used | uniqueness, lifetime owner |
| lifetime region | Semantic owner domain with an outlives relation | physical arena, lexical scope |
| allocation family | Base allocation plus projections rooted in it | detached field allocation |
| retained edge | A stored reference that survives the immediate operation | ordinary transient read |
| explicit copy | Independent graph with preserved internal topology | shallow clone, backend hint |
| inferred transfer | Optional final-use responsibility transfer | source move operator |
| GC representation | One physical implementation of legal semantics | permission for invalid topology |
| declared memory group | Accepted deferred hard lifetime region | allocator value, lifetime type |
| builder lifecycle root | Page/request/frame/mount owner supplied by build/runtime architecture | builder-specific language law |
| WIT value boundary | Independent lower/lift conversion with no Beanstalk aliases crossing | borrowed component resource |
| opaque handle | Foreign identity exposed through curated operations | Beanstalk reference type |

---

## Appendix C — Final no-invention checklist

Before completing any phase, verify:

- [ ] No new source syntax beyond approved `group` / `into` was invented.
- [ ] No exact Rust type or module name was made architectural unless already canonical.
- [ ] No diagnostic code was reserved without the diagnostic registry owner.
- [ ] No mandatory-consuming source operation was introduced.
- [ ] No source-visible RC or retain/release model was introduced.
- [ ] No backend-specific legality rule was introduced.
- [ ] No build-profile-specific validity rule was introduced.
- [ ] No group identity entered `TypeId` or source signatures.
- [ ] No arbitrary external lifetime graph was introduced.
- [ ] No current implementation was presented as complete when it is deferred or drifting.
- [ ] No generated documentation was edited directly.
- [ ] No legacy authority was retained after final deletion.
