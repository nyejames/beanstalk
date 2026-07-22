# Memory-Management Documentation Follow-up Correction Plan

**Status:** complete  
**Repository:** `nyejames/beanstalk`  
**Baseline reviewed:** commit `ee6562e9cf1d42dbe15bbae9675288455bbbf371`  
**Change class:** documentation-only  
**Primary objective:** remove the remaining contradictions and ambiguities from the final memory-management documentation without restoring the deleted monolith or changing compiler code  
**Required final gate:** `bean build docs --release` or `cargo run --quiet -- build docs --release`  
**Out of scope:** Rust implementation, compiler behavior changes, tests, fixtures, manifests, build scripts, runtime code, backend code, or generated HTML edited by hand

## Current state

ACTIVE_PLAN: `docs/roadmap/plans/memory-management-documentation-follow-up-corrections-plan.md`
STATUS: complete
CURRENT_SLICE: none
LAST_ACCEPTED_COMMIT: pending parent commit
WORKTREE: main; unrelated parallel compiler/src changes preserved and not staged
REQUIRED_RELOADS: startup files, this plan, current source/diff
RELEVANT_CONTEXT_NOW:
- docs: declared-memory-groups leaf created; grouped-memory roadmap converted to implementation roadmap; lifetime/runtime/borrow/ownership/language/compiler/build/README/progress/roadmap corrected
- code: none (documentation-only)
ACCEPTANCE_CRITERIA:
- all phase acceptance criteria satisfied
VALIDATION_STATE:
- `bean check docs`: passed
- `bean build docs --release`: passed; 65 files
DOCS_IMPACT: new declared-memory-groups route; memory/compiler/build/language/progress/roadmap/index updated; `docs/release/**` regenerated
BLOCKERS_OR_OPEN_DECISIONS: none
DELEGATION_DECISION: parent-direct - documentation-only parent-owned work (workers cannot edit docs/**)
NEXT_WORKER_ORDER: none
STOP_REASON: active work source complete
NEXT_RESUME_ACTION: none; compiler behavior changes require a separate approved implementation plan

---

## 1. Purpose

The memory-management authority migration is complete in structure, and the legacy monolith must remain deleted. This follow-up corrects the remaining documentation defects found after review of the committed migration.

The completed documentation must:

- retain the split memory-management authority;
- preserve every accepted design decision from the memory-design interview;
- remove all remaining contradictory stage and pipeline descriptions;
- define the remaining ambiguous grouped-memory and external-boundary cases;
- move accepted grouped-memory semantics into the canonical memory authority;
- reduce duplicated prose without deleting unique design information;
- keep deferred implementation and current compiler drift clearly separated from accepted design;
- give an implementation agent exact replacement wording and no freedom to invent design choices.

This plan is intentionally prescriptive. Where exact wording is supplied, use that wording or a mechanically equivalent formulation that preserves every stated condition. Do not weaken rules through summaries such as “implementation-defined,” “backend-dependent,” “may be supported,” or “future work” unless this plan explicitly assigns that status.

---

## 2. Locked decisions

These decisions are final for this documentation correction. Do not reopen them while executing the plan.

### 2.1 Authority and scope

- The canonical memory authority remains:

  ```text
  docs/src/docs/codebase/memory-management/
  ```

- `docs/memory-management-design.md` remains deleted.
- The old monolith must not be restored as a redirect, summary, compatibility file, or historical authority.
- The progress matrix remains authoritative for current implementation status.
- The roadmap remains authoritative for implementation order, not accepted semantic rules.
- This correction is documentation-only. Current compiler drift is not fixed here.

### 2.2 Core source model

The following sentence remains prominent and intentionally repeated in the main memory landing page, the access page, the language overview, and relevant educational pages:

> **Beanstalk is reference-semantic by default, copy-explicit and move-inferred. It omits explicit reference types and lifetime syntax, not references themselves.**

Do not deduplicate this sentence away.

### 2.3 Final-use child extraction

Final-use extraction of one child or projection from an allocation family is **not accepted current design**.

Current canonical rule:

> Interior projections remain rooted in their containing allocation family. A proven final use may transfer the containing allocation family, but it does not detach one projected child into a new independent allocation.

A future optimisation investigation may consider child extraction or retroactive field detachment, but it belongs only in a roadmap note until a separate design is accepted.

The roadmap note must say that this future design requires explicit treatment of:

- partially moved aggregates;
- parent representation after extraction;
- remaining aliases and projection roots;
- control-flow joins;
- destruction of remaining fields;
- reactive observers;
- external observers;
- aggregate invariants;
- backend parity.

Do not retain “extract an independently owned child at a proven final use” as an accepted option in canonical design prose.

### 2.4 README wording

Replace the current README goal:

> A GC fallback with ownership analysis that can remove runtime collection in ideal cases.

with this exact accessible wording:

> **Safe automatic memory management, with compiler checks that prevent invalid memory use and optimisations that can avoid garbage collection when proven safe.**

Do not use the longer compiler-architecture wording in the README. The README is a project introduction, not the technical authority.

### 2.5 Freshness terminology

The documentation must distinguish three concepts:

| Term | Exact meaning |
|---|---|
| **Fresh result root** | The result’s root allocation is newly created. The result may still retain references to pre-existing allocations, subject to outlives constraints. |
| **Alias result** | The result root is an existing argument, projection, external allocation, or another result. |
| **Independent result graph** | The result graph has no mutable sharing or retained Beanstalk reference to pre-existing storage. Explicit `copy` and WIT value lifting produce this stronger form. |

Rules:

- Do not use bare “fresh result” where the distinction matters.
- A fresh result root may retain parameters or other pre-existing values only when every retained edge satisfies the destination lifetime’s outlives constraints.
- An alias result cannot be placed directly into a declared destination group as new group-owned storage.
- An independent result graph can enter an unrelated destination lifetime without retained-edge constraints to its source graph.
- A WIT value-only result is an independent result graph, not merely a fresh result root.
- An explicit `copy` result is an independent result graph while preserving internal alias topology inside the copied graph.

### 2.6 Ancestor-group visibility

A declaration placed into an ancestor group is a narrow V1 escape mechanism, not a general definite-assignment system.

Canonical V1 rule:

- A declaration targeting an ancestor group is legal only from a straight-line nested `block:` or nested `group:` that executes at most once.
- It is invalid when any conditional or repeatable construct lies between the declaration and the destination group’s body.
- It is therefore invalid directly inside:
  - `if` branches;
  - match arms;
  - `catch` branches;
  - loops;
  - repeated template/runtime control flow;
  - any construct that may execute zero or multiple times.
- Conditional production must use one declaration in the destination scope whose initializer is a value-producing `if`, match, or `catch`.
- Loop production must use a collection or aggregate already owned by the destination group and mutate it through ordinary exclusive access; it must not repeatedly declare the same ancestor-owned name.
- Name collisions and definite initialization are checked in the destination scope.
- Visibility begins at the declaration point and continues through the remainder of the destination group.

Do not design branch merging, same-name declarations in separate arms, or loop-carried destination declarations in this documentation change.

### 2.7 External boundary profiles

External bindings use closed semantic profiles.

#### WIT value-only V1

The complete fixed contract is:

- supported arguments are read through shared access;
- every supported argument is lowered into an independent component value;
- the component receives no alias into Beanstalk storage;
- the original Beanstalk argument remains usable after the call;
- every result is lifted into an independent Beanstalk result graph;
- result aliasing to arguments is impossible by contract;
- the source call is non-consuming;
- no ownership bit crosses the boundary;
- no destruction responsibility crosses the boundary;
- no component allocation becomes part of the Beanstalk lifetime graph;
- no cross-runtime allocation cycle can form;
- component memory and allocation strategy remain component-private.

Supported V1 WIT value families:

- primitives;
- strings;
- records whose members are supported values;
- variants whose payloads are supported values;
- options of supported values;
- recursively supported lists.

Rejected V1 WIT surfaces:

- WIT resources;
- borrowed or owned resource handles;
- resource constructors or methods;
- callbacks into Beanstalk;
- retained closures or executable state;
- async functions;
- futures;
- streams;
- shared-memory views;
- raw pointers or pointer/length escape hatches;
- returned aliases;
- retained Beanstalk references;
- project-authored Beanstalk functions imported by the component.

Required importer metadata:

- stable package identity;
- stable world identity;
- stable interface identity;
- stable function identity;
- Beanstalk-facing parameter and result types;
- corresponding WIT value types;
- component export identity;
- trap or failure behavior;
- required component imports;
- target and runtime capability requirements.

The importer derives the fixed memory facts from the boundary profile. Binding authors do not restate those facts for every function.

#### Restricted host-binding V1

- Ordinary Beanstalk values cross by value as non-retained inputs.
- Host code cannot retain references into ordinary Beanstalk storage.
- Mutable host access is supported only for opaque foreign handles.
- A mutable opaque-handle argument controls legal access to the foreign identity; it is not direct mutation of Beanstalk storage.
- Ordinary Beanstalk values are not passed by mutable reference to host code.
- Host operations that conceptually modify ordinary Beanstalk data return a fresh or independent result instead of mutating Beanstalk storage through copy-in/copy-out.
- Do not document copy-in/copy-out writeback semantics in V1.
- Opaque handles represent foreign identities, not Beanstalk reference types.
- Observable external resources require explicit close or teardown operations.
- Host finalization timing cannot define observable Beanstalk behavior.

#### Transfer qualification

The general rule:

> Any ordinary Beanstalk runtime parameter may receive inferred destruction responsibility at a proven final-use call site.

must always be followed by:

> Closed external boundary profiles override this general rule. WIT value-only calls are non-consuming. Restricted host-value crossings are non-consuming, and mutable opaque-handle access does not transfer Beanstalk storage through the ordinary ownership ABI.

### 2.8 Diagnostics

Use one deterministic diagnostic rule everywhere:

> **User-authored topology that is invalid or cannot be proven legal produces a structured `CompilerDiagnostic`. Missing or inconsistent compiler-owned summaries, boundary classifications, lifecycle roots, or metadata produce `CompilerError`.**

Do not write that mandatory semantic failure “may” produce a source diagnostic.

### 2.9 Initial implicit region

Add this first step before the widening rule:

> **An ordinary allocation begins in the narrowest inferred region capable of owning its initial uses.**

Then retain:

> The compiler may widen that allocation only to the nearest existing ancestor on the same ordered owner chain that outlives every retained observer.

### 2.10 Profile behavior

Replace any statement that profiles may vary “analysis depth” with:

> **Build profiles may vary optional optimisation-analysis effort and physical allocation strategy. They must run semantically equivalent mandatory borrow and lifetime-topology validation and must not change source legality.**

### 2.11 Reassignment wording

Do not say only:

> V1 has no reassignment placement.

Use:

> **V1 has no `into group` syntax on reassignment. A mutable binding already owned by a group may be reassigned only with a fresh result root valid for that same group, an independent copy allocated into that group, or a same-group value transferred at a proven final use.**

### 2.12 One canonical build pipeline

The build-system document must contain one canonical link and lowering sequence:

```text
entry or package roots
-> exact reachable function and effect union
-> instantiate local lifetime summaries with builder lifecycle roots
-> validate complete lifetime topology
-> target-affinity and capability analysis
-> deterministic target partition
-> validate assigned functions and permitted cross-target edges
-> lower selected functions
```

No competing shorter pipeline may remain elsewhere in the same document.

### 2.13 Canonical grouped-memory ownership

Accepted `group` / `into` semantics must live inside the memory authority.

Create:

```text
docs/src/docs/codebase/memory-management/declared-memory-groups/
├── #page.bst
├── overview.bd
└── declared-memory-groups.bd
```

The existing roadmap file remains at:

```text
docs/roadmap/plans/grouped-memory-design.md
```

but becomes implementation sequencing and deferred follow-up only. It must not remain a co-equal canonical semantic authority.

---

## 3. Definition of completion

This plan is complete. Every checkbox below is satisfied.

- [x] The old monolith remains deleted.
- [x] One canonical build/link/lifetime/target pipeline appears in the build-system authority.
- [x] All educational pipeline pages agree with that sequence.
- [x] The grouped-memory semantic contract lives under the memory authority.
- [x] The grouped-memory roadmap file contains sequencing and deferred work only.
- [x] Final-use child extraction is removed from accepted design and recorded only as an unaccepted future optimisation investigation.
- [x] Fresh result root, alias result, and independent result graph are defined and used consistently.
- [x] Ancestor-group placement under branches and loops has the exact narrow V1 rule.
- [x] WIT value-only fixed facts, supported values, rejected surfaces, and importer metadata are restored.
- [x] Restricted host-binding mutable access is limited to opaque handles.
- [x] External profiles explicitly override general inferred-transfer behavior.
- [x] Mandatory semantic failures deterministically use the correct diagnostic lane.
- [x] Initial narrowest implicit-region assignment is explicit.
- [x] Profile-dependent analysis wording applies only to optional optimisation effort.
- [x] Reassignment wording distinguishes missing `into` syntax from legal same-group reassignment.
- [x] Compiler and build-system summary bullets include lifetime facts and summaries.
- [x] README uses the approved accessible wording.
- [x] Remaining educational pages no longer teach stale stage ownership or status.
- [x] Duplicate prose is removed only after unique facts are relocated.
- [x] The progress matrix still separates accepted design from current implementation.
- [x] Repository-wide contradiction searches are clean.
- [x] The documentation release build passes.
- [x] Every affected route and generated diff is inspected.

---

## 4. Target documentation architecture

The final memory tree must be:

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
├── declared-memory-groups/
│   ├── #page.bst
│   ├── overview.bd
│   └── declared-memory-groups.bd
├── ownership-and-drops/
│   ├── #page.bst
│   ├── overview.bd
│   └── ownership-and-drops.bd
└── runtime-and-backend-lowering/
    ├── #page.bst
    ├── overview.bd
    └── runtime-and-backend-lowering.bd
```

Topic ownership:

| Topic | Canonical owner |
|---|---|
| Reference semantics, explicit copy, aliases, `~` | `access-and-aliasing` |
| Access conflicts and optional transfer eligibility | `borrow-validation` |
| One owner, retained edges, escapes, cycles, outlives | `lifetime-regions-and-escape-validation` |
| Accepted `group` / `into` source and semantic contract | `declared-memory-groups` |
| Destruction responsibility, ownership bit, drops | `ownership-and-drops` |
| GC/RC/arena/handle representations and external profiles | `runtime-and-backend-lowering` |
| Group implementation order and unaccepted future optimisations | `docs/roadmap/plans/grouped-memory-design.md` |
| Current support | progress matrix |
| Compiler stage ownership | compiler design overview |
| Project/link orchestration | build-system design |

---

## 5. File change matrix

| File or area | Required action |
|---|---|
| `README.md` | Replace the memory goal with the approved accessible sentence. |
| `docs/build-system-design.md` | Merge pipeline descriptions; include lifetime summaries/facts; qualify graph topology terminology. |
| `docs/compiler-design-overview.md` | Synchronize opening invariants, artefact lanes, generated sidecars, companion authority wording, and external profile qualification. |
| `docs/language-overview.md` | Link canonical group page; clarify fresh/independent result terminology and reassignment wording; retain deferred status. |
| `docs/src/docs/codebase/memory-management/overview.bd` | Add declared-groups route; make diagnostic lane deterministic; retain intentional core sentence. |
| `.../access-and-aliasing/access-and-aliasing.bd` | Add terminology cross-reference where needed; do not change core copy semantics. |
| `.../borrow-validation/borrow-validation.bd` | Qualify general parameter transfer with external-profile override. |
| `.../lifetime-regions-and-escape-validation/overview.bd` | Point to canonical declared-groups page, not roadmap as co-authority. |
| `.../lifetime-regions-and-escape-validation/lifetime-regions-and-escape-validation.bd` | Add initial region step; fresh terminology; remove child extraction; deterministic diagnostic lane. |
| `.../declared-memory-groups/**` | New canonical grouped-memory semantic leaf. |
| `.../ownership-and-drops/ownership-and-drops.bd` | Add external-profile override near parameter-transfer rule. |
| `.../runtime-and-backend-lowering/runtime-and-backend-lowering.bd` | Restore full external profile facts; clarify host mutability; reduce duplicate imported-component wording. |
| `docs/roadmap/plans/grouped-memory-design.md` | Convert from semantic co-authority to implementation roadmap; retain only sequencing, implementation notes, and deferred investigations. |
| `docs/roadmap/roadmap.md` | Link canonical group design and keep implementation status deferred. |
| `docs/src/docs/progress/#page.bst` | Preserve deferred statuses; link or name canonical new group leaf where useful. |
| Educational compiler pages | Synchronize lifetime facts, link validation, target input, lowering input, and deferred status. |
| `docs/roadmap/plans/memory-management-docs-migration-completion-plan.md` | Update completion metadata/checklist so it does not look unfinished. |
| `docs/release/**` | Regenerate only through the docs release build. Never edit manually. |

---

# Phase 0 — Establish the exact baseline

## Context

This phase prevents the agent from editing against stale assumptions or accidentally mixing code changes into a documentation-only correction.

## Tasks

- [ ] Confirm the working baseline includes commit:

  ```text
  ee6562e9cf1d42dbe15bbae9675288455bbbf371
  ```

- [ ] Read before editing:
  - `AGENTS.md`
  - `CONTRIBUTING.md`
  - `docs/src/docs/codebase/style-guide/style-guide.bd`
  - `docs/src/docs/codebase/style-guide/validation.bd`
  - `docs/src/docs/codebase/memory-management/overview.bd`
  - all five current detailed memory leaves
  - `docs/build-system-design.md`
  - `docs/compiler-design-overview.md`
  - `docs/language-overview.md`
  - `docs/src/docs/progress/#page.bst`
  - `docs/roadmap/plans/grouped-memory-design.md`

- [ ] Record the pre-edit changed-file list.
- [ ] Confirm no Rust, test, fixture, manifest, build-script, or runtime file is part of this slice.
- [ ] Run source-only searches and save the result for comparison:

  ```sh
  rg -n \
    "extracting an independently owned child|fresh result|analysis depth|reassignment placement|may produce a source diagnostic|possible ownership consumption|queued work.*lifetime|grouped-memory-design|GC fallback|Stage 7|memory-management-design" \
    README.md docs index.md
  ```

- [ ] Locate every educational page that discusses:
  - module artefacts;
  - linking/reachability;
  - lifetime validation;
  - target planning;
  - backend lowering;
  - borrow validation status.

## Acceptance criteria

- [ ] Baseline and scope are recorded.
- [ ] No non-documentation files are selected.
- [ ] Every stale phrase has a known owner before editing begins.

---

# Phase 1 — Make the memory directory the actual single authority

## Context

The memory overview currently claims the directory is the single authority, while accepted `group` / `into` semantics still live in a roadmap plan. This phase resolves that ownership contradiction.

## 1.1 Create the declared-groups leaf

Create:

```text
docs/src/docs/codebase/memory-management/declared-memory-groups/#page.bst
docs/src/docs/codebase/memory-management/declared-memory-groups/overview.bd
docs/src/docs/codebase/memory-management/declared-memory-groups/declared-memory-groups.bd
```

### `overview.bd` required contract

Use this contract:

```markdown
# Declared memory groups

Accepted deferred `group` / `into` semantics and hard destination lifetimes.

## Contract

- Input: accepted `group` blocks and declaration receiving-boundary placement
- Output: declared lifetime-owner, placement, destination-scope and group-exit semantics
- Decides: group identity, legal placement syntax, destination binding visibility, nested-group retained-edge rules and group escape rules
- Must not decide: current implementation support, borrow-checker algorithm, physical arena representation, backend allocation or implementation order
- Invariant: a declared group is a hard semantic lifetime boundary and cannot be silently widened
```

Then include links to:

- detailed group page;
- lifetime-region leaf;
- access leaf;
- roadmap implementation plan;
- language overview;
- progress matrix.

### Detailed page required headings

The detailed page must contain these headings in this order:

1. `## Design contract`
2. `## Status`
3. `## Relationship to the general memory model`
4. `## Terminology`
5. `## group block`
6. `## into declaration placement`
7. `## Destination scope and visibility`
8. `## Placement eligibility`
9. `## Nested groups and retained edges`
10. `## Escapes`
11. `## Reassignment inside a group`
12. `## Calls and hidden result destinations`
13. `## Reactive and builder-owned lifetimes`
14. `## Diagnostics`
15. `## Deferred extensions`
16. `## Related reading`

### Required design contract text

Include:

> A declared memory group is a hard semantic lifetime region. Values placed into a group belong to that group for their full lifetime. They may retain references to allocations owned by the same group or by a region statically known to outlive the group. No group-owned value, projection, or alias may outlive the group.

Also include:

> Group identity is semantic lifetime metadata. It is not a value, type, field, parameter, generic argument, trait, allocator object, or lifetime annotation, and it must not enter `TypeId` or a source signature.

### Required syntax

```beanstalk
group request:
    parsed ParsedPost into request = parse_post(post)
    html String into request = render_post(parsed)
;
```

```text
name [access/type] into group_name = expression
```

Rules:

- `group name:` is valid only in runtime executable bodies.
- `into group_name` appears only on declaration receiving boundaries in V1.
- Placement may target the current group or a lexically enclosing ancestor group.
- Placement may not target a sibling, child, unrelated group, or builder lifecycle root by spelling its name.
- Groups cannot be passed, returned, stored, imported, exported, compared, or used as values.
- Group identity is not source type identity.
- Group closure occurs on every exit: fallthrough, `return`, `return!`, `break`, recovery exit, and checked-operation failure path.

### Destination-scope rule

Insert the locked V1 rule from section 2.6 verbatim or equivalently.

Include valid example:

```beanstalk
group request:
    group scratch:
        parsed ParsedPost into scratch = parse_post(post)
        html String into request = render_post(parsed)
    ;

    use(html)
;
```

Include invalid branch example:

```beanstalk
group request:
    if condition:
        html String into request = render_post() -- invalid in V1
    ;

    use(html)
;
```

Include replacement:

```beanstalk
group request:
    html String into request = if condition:
        then render_post()
    else
        then render_fallback()
    ;
;
```

Include invalid loop example:

```beanstalk
group request:
    loop posts |post|:
        html String into request = render_post(post) -- invalid repeated declaration
    ;
;
```

Explain that loops must mutate a destination-owned aggregate instead.

### Placement eligibility

Define:

- **Fresh result root**
- **Alias result**
- **Independent result graph**

Use the exact definitions in section 2.5.

Then state:

> A declaration may be placed into a group when its result root is fresh and every retained edge is legal for the destination group, or when it is an independent result graph such as an explicit `copy`. An alias result cannot become new group-owned storage through `into`.

Do not say every fresh result is independent.

### Reassignment rule

Use the exact wording from section 2.11.

### Group crossing

State:

> A value crosses from a shorter-lived group into a longer-lived group only by producing independent storage in the destination lifetime or by invoking a fresh producer that allocates directly into the destination. V1 has no extraction, adoption, or group-to-group transfer operation.

### Diagnostics

Group-specific diagnostics must identify:

- group declaration;
- destination declaration;
- source value or result;
- invalid retained edge or escape;
- source and destination regions;
- failed rule;
- ranked remedy.

Remedy order remains:

1. allocate directly into the destination group;
2. place observers under one common group;
3. use `copy`;
4. shorten the retained edge;
5. repair package-owned metadata where applicable.

## 1.2 Update navigation

- [ ] Add `declared-memory-groups` to:
  - memory landing page detailed list;
  - memory task-reading guide;
  - codebase navigation page where memory leaves are enumerated;
  - lifetime leaf related reading;
  - access, ownership, and runtime related reading where relevant.

- [ ] Add a task-reading guide row:

  | Task | Read first | Detailed reference |
  |---|---|---|
  | `group` / `into`, destination scope, hard group exits, nested groups | declared-groups overview + lifetime overview | declared-groups detailed page |

## 1.3 Remove split authority wording

Replace wording equivalent to:

> canonical design lives in this leaf plus `docs/roadmap/plans/grouped-memory-design.md`

with:

> Canonical grouped-memory semantics live under `docs/src/docs/codebase/memory-management/declared-memory-groups/`. The roadmap file owns implementation sequencing and deferred follow-up only.

## Acceptance criteria

- [ ] The memory directory is literally the single semantic authority.
- [ ] The roadmap is no longer needed to understand accepted `group` / `into` semantics.
- [ ] The new route builds and is linked from all relevant index pages.
- [ ] No semantic rule was lost during the move.

---

# Phase 2 — Convert the grouped-memory roadmap into an implementation roadmap

## Context

The current grouped-memory file repeats the entire memory model and acts as a second design authority. Keep its unique implementation sequencing, but remove duplicated canonical semantics after they are moved to the new leaf.

## Required title and status

Change the file title and opening to:

```markdown
# Grouped Memory Implementation Roadmap

**Status:** accepted design; implementation deferred  
**Canonical semantics:** `docs/src/docs/codebase/memory-management/declared-memory-groups/`  
**Purpose:** implementation sequencing, prerequisites, current gaps, and deferred optimisation investigations
```

## Required sections

Keep only these sections:

1. `## Purpose and status`
2. `## Canonical design prerequisites`
3. `## Current implementation state`
4. `## Compiler and build-system prerequisites`
5. `## Proposed implementation phases`
6. `## Validation and diagnostics requirements`
7. `## Deferred optimisation investigations`
8. `## Relationship to ownership-aware Wasm completion`
9. `## Not active or queued`

## Content to retain

Retain unique implementation information about:

- parser and AST group identity;
- destination-scope bookkeeping;
- HIR group metadata and exits;
- local lifetime constraints and summaries;
- project/link summary instantiation;
- JS no-op physical lowering with full semantic enforcement;
- hidden result destinations;
- Wasm group release markers;
- builder-owned page and mount roots;
- collector-elision verification;
- required diagnostic coverage;
- likely prerequisite relationship to ownership-aware Wasm completion.

## Content to remove and replace with links

Remove repeated full explanations of:

- reference semantics;
- explicit copy graph semantics;
- one lifetime owner;
- stored-edge rule;
- cycles;
- ownership bit;
- WIT profile;
- host profile;
- backend representation;
- general diagnostic philosophy;
- source syntax already moved to the canonical group leaf.

Replace each removed area with one concise link sentence.

## Deferred child-extraction note

Add this exact roadmap-only note:

```markdown
### Final-use child extraction

Final-use extraction or detachment of one projected child from an allocation family is not accepted current design and is not part of the first grouped-memory implementation.

Before this can become accepted architecture, a separate design must define:

- partially moved aggregate semantics;
- the parent representation after extraction;
- invalidation of existing aliases and projections;
- control-flow joins;
- destruction of remaining fields;
- reactive and external observers;
- aggregate invariants;
- parity across GC, region, RC, and ownership-aware backends.

Until such a design is accepted, projections remain rooted in their containing allocation family. A proven final use may transfer the entire allocation family, not detach one child.
```

## Acceptance criteria

- [ ] The roadmap is materially shorter.
- [ ] No accepted semantic rule exists only in the roadmap.
- [ ] Unique implementation sequencing remains.
- [ ] Final-use child extraction is clearly unaccepted and deferred.

---

# Phase 3 — Correct lifetime terminology and projection rules

## Context

This phase removes the most important remaining semantic ambiguity: fresh root versus independent graph, and whole-family transfer versus child detachment.

## 3.1 Update lifetime terminology table

In:

```text
docs/src/docs/codebase/memory-management/lifetime-regions-and-escape-validation/lifetime-regions-and-escape-validation.bd
```

add rows for:

| Term | Definition |
|---|---|
| Fresh result root | Newly allocated result root that may retain legal references to older allocations. |
| Alias result | Result root that is an existing argument, projection, external allocation, or another result. |
| Independent result graph | New graph with no mutable sharing or retained Beanstalk reference to pre-existing storage. |

## 3.2 Initial-region rule

At the beginning of implicit inference, insert:

> An ordinary allocation begins in the narrowest inferred region capable of owning its initial uses.

Then present the sequence:

```text
choose the narrowest initial owner
-> collect retained observers
-> widen only when required
-> choose the nearest existing ancestor on the same owner chain
```

Keep all existing prohibitions against lateral sibling promotion and invented page/application/process owners.

## 3.3 Replace projection extraction wording

Delete every accepted-design statement equivalent to:

- extract an independently owned child at final use;
- detach a projection at final use;
- transfer a child out of its family without prior independent allocation.

Canonical replacement:

> Interior projections remain rooted in their containing allocation family. Returning, storing, or escaping a projection retains that family. At a proven final use, analysis may transfer the containing allocation family. A child is independently releasable only when it was already established as a separately owned allocation before the escape.

Field-sensitive splitting may remain as a future compatible optimisation only with this qualifier:

> Field-sensitive allocation splitting is legal only when the compiler establishes separate child ownership before the alias escapes. It is not retroactive child extraction.

## 3.4 Returns and destination placement

Replace ambiguous “fresh result” prose with:

> A caller may place a fresh result root directly into a destination region only when every retained edge in the result is legal for that destination. An alias result remains tied to its existing lifetime owner. An independent result graph may enter an unrelated destination lifetime without retained edges to its source graph.

## 3.5 Diagnostic lane

Replace:

> Failure to prove mandatory semantic legality may produce a source diagnostic.

with:

> User-authored topology that is invalid or cannot be proven legal produces a structured `CompilerDiagnostic`. Missing or inconsistent compiler-owned summaries, boundary classifications, lifecycle roots, or metadata produce `CompilerError`.

Apply the same replacement in the top-level memory overview and any other canonical leaf.

## Acceptance criteria

- [ ] Bare “fresh result” is absent where the stronger distinction matters.
- [ ] No accepted text permits retroactive child extraction.
- [ ] Initial region ownership is explicit.
- [ ] Diagnostic lanes are deterministic.

---

# Phase 4 — Restore the complete external-boundary contract

## Context

The committed migration preserved the profile names but compressed away several fixed facts and left ordinary mutable host values ambiguous.

## 4.1 WIT value-only canonical wording

In the runtime/backend detailed page, include this canonical block:

> **Beanstalk imports general external Wasm libraries as WIT-described Wasm Components. The V1 component boundary is value-only. Every supported argument is lowered from a shared read of a Beanstalk value into an independent component value, and every result is lifted into an independent Beanstalk result graph. No Beanstalk alias, lifetime owner, ownership state, or destruction responsibility crosses the component boundary.**

Then add:

> The component may use any private allocation or garbage-collection strategy. That strategy is not part of the Beanstalk binding contract and does not require a GC declaration.

## 4.2 Fixed facts table

Add:

| Fact | WIT value-only V1 |
|---|---|
| Parameter source access | Shared read |
| Boundary crossing | Independent semantic value |
| Retained Beanstalk reference | Never |
| Result provenance | Independent result graph |
| Result aliases an argument | Never |
| Source call consumes argument | Never |
| Destruction responsibility crosses | Never |
| External lifetime owner | Component-private |
| Cross-runtime allocation cycle | Impossible by contract |

## 4.3 Supported and rejected WIT values

Add the supported value list and rejected surface list from section 2.7.

Unsupported interfaces must receive structured import diagnostics identifying:

- unsupported WIT feature;
- function;
- parameter or result;
- why it cannot cross the value-only boundary;
- whether a wrapper component could expose a supported value-only interface.

## 4.4 Required importer metadata

Add the required metadata list from section 2.7.

State:

> The boundary classification generates the fixed memory facts. The importer must not infer them from a generic `Fresh` result marker, and binding authors must not restate them per function.

Do not require a `uses_gc` field.

## 4.5 Restricted host-binding profile

Insert the locked host rules from section 2.7.

Use this exact summary:

> Restricted host-binding V1 permits mutable access only to opaque foreign handles. Ordinary Beanstalk values cross by value as non-retained inputs. A host operation that conceptually changes ordinary Beanstalk data returns a fresh or independent result instead of mutating Beanstalk storage through the boundary.

Do not define copy-in/copy-out writeback.

## 4.6 External transfer override

In both:

```text
borrow-validation/borrow-validation.bd
ownership-and-drops/ownership-and-drops.bd
```

after the general parameter transfer rule, insert:

> Closed external boundary profiles override this general rule. WIT value-only calls are non-consuming. Restricted host-value crossings are non-consuming, and mutable opaque-handle access does not transfer Beanstalk storage through the ordinary Beanstalk ownership ABI.

## 4.7 Imported component memory wording

Collapse duplicate wording into one paragraph:

> Each page may own one runtime and memory shared by its linked Beanstalk Wasm variants. Imported WIT components are separate runtimes with component-private memory and communicate only through the closed value boundary.

## Acceptance criteria

- [ ] The full WIT fixed contract is present in one canonical place.
- [ ] Supported and rejected value families are explicit.
- [ ] Required importer metadata is explicit.
- [ ] No GC declaration is required.
- [ ] Ordinary mutable host values are no longer ambiguous.
- [ ] General transfer rules are qualified by external profiles.
- [ ] Page memory is limited to Beanstalk-linked variants.

---

# Phase 5 — Unify compiler and build-system architecture

## Context

The design authorities currently contain correct detailed sections but stale or competing summary pipelines.

## 5.1 Build-system companion authority wording

Change the memory companion description from wording limited to:

> access, borrow, GC, ownership and destruction

to:

> reference semantics, borrow validation, lifetime topology, declared groups, ownership, GC and backend memory lowering

## 5.2 Qualify topology terminology

In the Stage 0 invariant, replace unqualified:

> legal topology

with:

> legal project/module graph topology

This prevents confusion with lifetime topology.

## 5.3 Fixed bootstrap sequence

Use:

```text
select command, artefact builder, build profile and tooling overlays
-> construct compiler and builder bootstrap capability surface
-> compile and validate config
-> derive entry_root and @project
-> build the canonical source index and provider graphs
-> resolve build-input contracts
-> compile dependency-ordered waves
   -> bind provider interfaces
   -> order local declarations
   -> run AST semantics
   -> lower and validate HIR
   -> borrow-validate
   -> produce local lifetime constraints and exported summaries
-> complete the generated-function worklist
-> assemble a success-only ProjectCompilation
-> plan entry/package roots and exact reachable unions
-> instantiate and validate complete lifetime topology
-> plan target assignments and validate them
-> lower backend artefacts
```

The existing longer bootstrap detail may remain, but it must preserve this order.

## 5.4 Mixed-target sequence

Replace the current shorter mixed-target sequence with the one canonical sequence from section 2.12.

Delete or convert the later duplicate pipeline into prose describing inputs and ownership. Do not leave two code blocks that disagree.

## 5.5 Module jobs and generated sidecars

Ensure module-job output includes:

- local lifetime constraints;
- local lifetime facts where applicable;
- exported stable lifetime summaries.

Ensure generated sidecars carry:

- generated-local type context;
- HIR;
- borrow facts;
- lifetime facts and summaries;
- link facts;
- fingerprints.

## 5.6 Compiler overview opening invariants

Update opening bullets so they say:

- modules own local type, HIR, borrow, and lifetime-analysis identity/facts;
- normal modules produce local lifetime constraints and summaries before entry activation;
- generated functions are HIR-validated, borrow-validated, and lifetime-analysed before backend handoff;
- lowerers receive validated lifetime topology and do not reconsider source legality.

## 5.7 Compiler artefact summary

Ensure:

`ModuleExecutable` includes:

- local `TypeEnvironment`;
- validated HIR;
- borrow facts;
- local lifetime-region and escape facts.

`PublicSemanticInterface` conceptually includes stable summaries for:

- fresh result roots;
- alias and projection results;
- result-to-result alias relationships;
- retained-parameter relationships;
- outlives constraints;
- external boundary classification.

State:

> Donor-local region IDs do not cross module interfaces. Exported lifetime summaries use stable semantic relationships.

## 5.8 Target-validation order

Target validation must say it runs after:

- HIR validation;
- borrow validation;
- complete project/link lifetime-topology validation.

## Acceptance criteria

- [ ] One canonical pipeline remains.
- [ ] Opening summaries agree with detailed sections.
- [ ] Lifetime facts appear in ordinary and generated artefacts.
- [ ] Target planning receives validated topology.

---

# Phase 6 — Synchronize the language overview and README

## 6.1 README

Apply the exact replacement from section 2.4.

No other README memory architecture expansion is needed.

## 6.2 Language companion links

Update memory links so canonical grouped-memory semantics point to:

```text
docs/src/docs/codebase/memory-management/declared-memory-groups/
```

The roadmap link may remain only for implementation status and order.

## 6.3 Memory semantics section

Add or revise concise definitions:

```markdown
- A fresh result root has a new root allocation but may retain legal references.
- An alias result reuses an existing root or projection.
- An independent result graph has no retained Beanstalk reference to pre-existing storage.
```

## 6.4 Group wording

Replace:

> V1 has no expression-site placement, reassignment placement, extraction or unrestricted group-to-group adoption.

with:

> V1 has no expression-site placement, no `into group` syntax on reassignment, no extraction, and no unrestricted group-to-group adoption. A mutable binding already owned by a group may be reassigned only with a fresh result root valid for that group, an independent copy allocated into that group, or a same-group value transferred at a proven final use.

Add the straight-line ancestor-placement restriction in concise form.

## 6.5 External platform imports

Add one concise paragraph:

> Ordinary values in restricted host bindings cross by value and cannot be retained as references into Beanstalk storage. Mutable host access applies only to opaque foreign handles. General Wasm Components use the separate WIT value-only profile described by the memory authority.

Do not expand the language overview into the full WIT reference.

## Acceptance criteria

- [ ] README is simple and accurate.
- [ ] Language overview points to the canonical group page.
- [ ] Reassignment and ancestor-placement wording is exact.
- [ ] External host mutability is unambiguous.

---

# Phase 7 — Synchronize educational compiler pages

## Context

The educational series must teach the accepted pipeline without becoming a second architecture authority.

## 7.1 Module artefacts and reuse

Update:

```text
docs/src/docs/codebase/compiler-design/module-artefacts-and-reuse/
```

Required corrections:

- Executable state includes lifetime-region and escape facts.
- Public interface includes stable lifetime/effect summaries.
- Generated sidecars include lifetime facts and summaries.
- Project-level topology is not stored as donor-local region IDs.
- Fingerprints include topology-relevant public summaries and link facts.

Suggested concise replacement:

> **Executable state.** Module-local `TypeEnvironment`, validated HIR, borrow facts, and local lifetime-region/escape facts.
>
> **Public semantic interface.** Exported identities, types, values, access/effect facts, and stable lifetime summaries such as fresh roots, aliases, retained parameters, and outlives constraints.
>
> Generated sidecars carry the same categories for each concrete generated function.

## 7.2 Linking, entries and reachability

Add:

```markdown
## Complete lifetime topology

The exact reachable union is also the point where local lifetime summaries can
be instantiated against actual callers, entry/package roots, and
builder-supplied lifecycle roots.

Project/link validation completes the lifetime topology before target
assignment. Linking does not reopen source or mutate HIR.
```

Exit state:

> You leave with explicit roots, a reachable function union, and a validated project/link lifetime topology. Next: target assignment.

## 7.3 Target planning and validation

State at the start:

> Target planning receives an already-validated lifetime topology. It assigns targets and checks capabilities; it does not change region ownership or source legality.

Canonical loop:

1. take the reachable union and validated lifetime topology;
2. run affinity/capability analysis;
3. assign targets;
4. record reasons;
5. validate assigned functions;
6. validate cross-target edges.

## 7.4 Backend lowering

Add lowerer inputs:

- borrow facts;
- validated lifetime facts and summaries;
- external boundary classifications;
- builder lifecycle/runtime plan;
- selected functions/imports/capabilities/layout identities.

Add prohibition:

> A lowerer may not reconsider lifetime topology or source legality.

Qualify page memory:

> Page-local shared Wasm runtime/memory applies to Beanstalk-linked variants only. Imported WIT components use component-private memory and value conversion.

## 7.5 Borrow-validation educational article

Replace “possible ownership consumption” with:

> optional transfer eligibility and effect category

Correct status:

- lifetime-region and escape validation is **deferred implementation**, not queued;
- grouped memory is accepted design with deferred implementation;
- ownership-aware backend realisation follows separate plans.

Remove `unsafe return alias` from ordinary borrow failures if it is meant as topology failure. Use:

- access conflict;
- invalid mutation;
- invalid alias use;
- internal inconsistent borrow metadata.

Then state return-escape legality belongs to lifetime validation.

## 7.6 Memory/GC and design-choice educational pages

Replace loose statements such as:

> GC backs correctness on every backend.

with:

> GC can represent any lifetime topology the compiler has already accepted. Borrow and lifetime checks still define legality.

Preserve accessible teaching language.

## Acceptance criteria

- [ ] Every article’s incoming and outgoing state matches the canonical pipeline.
- [ ] Deferred status is accurate.
- [ ] Page memory excludes imported components.
- [ ] Educational pages do not invent a Stage 7.

---

# Phase 8 — Progress, roadmap, and plan-state cleanup

## 8.1 Progress matrix

Keep these rows deferred:

- lifetime-region and escape validation;
- declared memory groups;
- WIT value-only component integration.

Update notes to link or name the canonical group leaf.

Do not imply implementation has landed.

## 8.2 Main roadmap

In the region/group section:

- link canonical semantics to the declared-groups leaf;
- link implementation sequencing to the grouped-memory roadmap;
- state the work remains deferred and is not automatically active or queued;
- retain likely prerequisite relationship to ownership-aware Wasm completion.

## 8.3 Completed migration plan metadata

Update:

```text
docs/roadmap/plans/memory-management-docs-migration-completion-plan.md
```

Required cleanup:

- set `LAST_ACCEPTED_COMMIT` to the actual migration commit;
- tick completed definition-of-completion checkboxes or replace them with a completed-results summary;
- ensure no `pending parent commit` text remains;
- add a note linking this follow-up correction plan if it is committed into the roadmap;
- do not reopen the old migration as active work.

## Acceptance criteria

- [ ] Status authorities agree.
- [ ] No completed plan appears unfinished.
- [ ] Deferred work is not mislabeled queued.

---

# Phase 9 — Concision pass without information loss

## Context

Concision is permitted only after canonical ownership is correct.

## Rules

- Remove duplicated explanation only when the same fact exists in its canonical owner.
- Replace duplication with a direct link and a one-sentence boundary summary.
- Do not remove the intentional reference-semantics sentence.
- Do not compress rule tables into vague prose.
- Do not replace exact invalid/valid cases with “subject to analysis.”
- Do not move current-support status into design pages.

## Required reductions

### Group roadmap

Target the largest reduction here. Remove duplicated semantic chapters after they are moved to the declared-groups leaf.

### Runtime leaf

Collapse repeated imported-component isolation paragraphs into one section.

### Lifetime leaf

- Keep one declaration of the one-owner invariant.
- Keep one formal stored-edge rule.
- Keep one nearest-ancestor widening explanation.
- Avoid restating the same rule in consecutive paragraphs.

### Build system

Keep one pipeline code block. Use prose for ownership detail.

## Information that must remain explicit

- one lifetime owner;
- stored-edge formula;
- initial narrowest owner;
- nearest existing ancestor;
- no lateral sibling promotion;
- cross-region cycle prohibition;
- fresh root versus independent graph;
- group straight-line ancestor-placement restriction;
- WIT fixed facts;
- host opaque-handle mutability only;
- diagnostic remedy order;
- external-profile transfer override;
- final-use child extraction remains unaccepted.

## Acceptance criteria

- [ ] Documents are shorter where duplication existed.
- [ ] No unique rule is deleted.
- [ ] Every removed paragraph has an obvious canonical destination or was true duplication.

---

# Phase 10 — Contradiction and stale-reference audit

Run these searches against source documentation before generating output:

```sh
rg -n \
  "extracting an independently owned child|detach.*child|child extraction" \
  README.md docs index.md
```

Expected:

- only the roadmap deferred investigation note;
- no accepted-design occurrence.

```sh
rg -n \
  "\bfresh result\b|result is fresh|returns fresh storage" \
  README.md docs index.md
```

Review every hit and replace ambiguous uses with:

- fresh result root;
- alias result;
- independent result graph.

```sh
rg -n \
  "may produce a source diagnostic|may produce.*diagnostic" \
  docs/src/docs/codebase/memory-management docs/compiler-design-overview.md
```

Expected: no mandatory-topology ambiguity.

```sh
rg -n \
  "analysis depth" \
  docs/src/docs/codebase/memory-management docs/roadmap/plans/grouped-memory-design.md
```

Expected: no profile-dependent mandatory-analysis wording.

```sh
rg -n \
  "reassignment placement" \
  docs/language-overview.md docs/src/docs/codebase/memory-management
```

Expected: every hit explicitly says “no `into group` syntax on reassignment” and explains legal same-group reassignment.

```sh
rg -n \
  "possible ownership consumption|possible parameter consumption|MayConsume" \
  docs
```

Review for:
- optional transfer terminology;
- no mandatory-consuming source API;
- external-profile override.

```sh
rg -n \
  "queued work.*lifetime|lifetime.*queued work|queued.*group" \
  docs/src/docs/codebase docs/roadmap
```

Expected: deferred unless a later approved roadmap change says otherwise.

```sh
rg -n \
  "grouped-memory-design.md" \
  README.md docs index.md
```

Expected:
- roadmap references only for sequencing/deferred work;
- canonical semantic links point to declared-memory-groups.

```sh
rg -n \
  "memory-management-design.md|Legacy consolidated reference" \
  README.md docs index.md
```

Expected:
- no live authority reference;
- historical mention only if intentionally retained in a completed migration record.

```sh
rg -n \
  "Stage 7|stage 7" \
  docs
```

Expected:
- only explicit statements that no numbered Stage 7 exists, if retained.

```sh
rg -n \
  "one memory shared by.*Wasm|page.*shared.*memory" \
  docs
```

Expected:
- every relevant occurrence says Beanstalk-linked variants;
- imported WIT components are excluded.

## Acceptance criteria

- [ ] Every search has an expected, reviewed result.
- [ ] No unexplained contradictory hit remains.

---

# Phase 11 — Documentation validation and route inspection

## Required final gate

Because this is documentation-only, run exactly one release-build gate:

```sh
bean build docs --release
```

If the release binary is unavailable:

```sh
cargo run --quiet -- build docs --release
```

Do not run `just validate` solely for this documentation-only slice.

## Source inspection

Inspect every changed source file for:

- correct Beandown syntax;
- correct links;
- no broken tables;
- no invented source examples;
- no current/deferred status confusion;
- no generated HTML edited directly.

## Required route inspection

Inspect at minimum:

```text
/docs/codebase/memory-management/
/docs/codebase/memory-management/access-and-aliasing/
/docs/codebase/memory-management/borrow-validation/
/docs/codebase/memory-management/lifetime-regions-and-escape-validation/
/docs/codebase/memory-management/declared-memory-groups/
/docs/codebase/memory-management/ownership-and-drops/
/docs/codebase/memory-management/runtime-and-backend-lowering/
/docs/codebase/compiler-design/memory-management-and-gc/
/docs/codebase/compiler-design/borrow-validation-and-drops/
/docs/codebase/compiler-design/module-artefacts-and-reuse/
/docs/codebase/compiler-design/linking-entries-and-reachability/
/docs/codebase/compiler-design/target-planning-and-validation/
/docs/codebase/compiler-design/backend-lowering/
/docs/progress/
```

Also inspect:

- the generated codebase index;
- the generated memory landing page;
- README rendering on GitHub where practical.

## Generated diff inspection

Confirm:

- new declared-group route is generated;
- removed duplicate content disappears only because source moved or was deduplicated;
- no unrelated generated churn appears;
- no generated file was hand-edited;
- every generated change is attributable to a source change.

## Acceptance criteria

- [ ] Release build passes.
- [ ] Every required route is inspected.
- [ ] Generated output is source-derived.
- [ ] Changed-file list remains documentation-only.

---

## 12. Final report template

The implementing agent’s final report must use these headings.

### Documentation changes

- Files created.
- Files materially revised.
- Files reduced through deduplication.
- Canonical topic ownership changes.

### Design corrections

Explicitly confirm:

- final-use child extraction removed from accepted design;
- fresh result root / alias result / independent result graph terminology;
- narrow V1 ancestor-placement rule;
- complete WIT value-only contract;
- opaque-handle-only host mutability;
- external-profile transfer override;
- deterministic diagnostic lanes;
- initial narrowest-region rule;
- one canonical build pipeline;
- README wording.

### Information-preservation review

List each large deletion or rewrite and identify where its unique information now lives.

### Deferred implementation

State clearly that this change does not implement:

- lifetime-region analysis;
- declared groups;
- WIT component imports;
- host mutable-value writeback;
- final-use child extraction;
- ownership-aware Wasm completion;
- compiler drift fixes.

### Validation

State:

- exact release-build command;
- result;
- changed routes inspected;
- contradiction-search results.

### Remaining work

State that compiler behavior changes require a separate approved implementation plan.

---

## Appendix A — Exact replacement snippets

### A.1 README goal

```markdown
- Safe automatic memory management, with compiler checks that prevent invalid memory use and optimisations that can avoid garbage collection when proven safe.
```

### A.2 Diagnostic lane

```markdown
User-authored topology that is invalid or cannot be proven legal produces a structured `CompilerDiagnostic`. Missing or inconsistent compiler-owned summaries, boundary classifications, lifecycle roots, or metadata produce `CompilerError`.
```

### A.3 Initial implicit region

```markdown
An ordinary allocation begins in the narrowest inferred region capable of owning its initial uses. The compiler may widen it only to the nearest existing ancestor on the same ordered owner chain that outlives every retained observer.
```

### A.4 Projection rule

```markdown
Interior projections remain rooted in their containing allocation family. Returning, storing, or escaping a projection retains that family. At a proven final use, analysis may transfer the containing allocation family. A child is independently releasable only when it was already established as a separately owned allocation before the escape.
```

### A.5 Fresh terminology

```markdown
- A fresh result root has a newly created root allocation and may retain legal references to older allocations.
- An alias result reuses an existing argument, projection, external allocation, or another result as its root.
- An independent result graph has no mutable sharing or retained Beanstalk reference to pre-existing storage.
```

### A.6 External transfer override

```markdown
Any ordinary Beanstalk runtime parameter may receive inferred destruction responsibility at a proven final-use call site. Closed external boundary profiles override this general rule. WIT value-only calls are non-consuming. Restricted host-value crossings are non-consuming, and mutable opaque-handle access does not transfer Beanstalk storage through the ordinary Beanstalk ownership ABI.
```

### A.7 Host profile

```markdown
Restricted host-binding V1 permits mutable access only to opaque foreign handles. Ordinary Beanstalk values cross by value as non-retained inputs. A host operation that conceptually changes ordinary Beanstalk data returns a fresh or independent result instead of mutating Beanstalk storage through the boundary.
```

### A.8 Group reassignment

```markdown
V1 has no `into group` syntax on reassignment. A mutable binding already owned by a group may be reassigned only with a fresh result root valid for that same group, an independent copy allocated into that group, or a same-group value transferred at a proven final use.
```

### A.9 Profile behavior

```markdown
Build profiles may vary optional optimisation-analysis effort and physical allocation strategy. They must run semantically equivalent mandatory borrow and lifetime-topology validation and must not change source legality.
```

### A.10 Build/link pipeline

```text
entry or package roots
-> exact reachable function and effect union
-> instantiate local lifetime summaries with builder lifecycle roots
-> validate complete lifetime topology
-> target-affinity and capability analysis
-> deterministic target partition
-> validate assigned functions and permitted cross-target edges
-> lower selected functions
```

---

## Appendix B — No-invention checklist

Before marking the plan complete, verify:

- [ ] No source syntax beyond accepted `group` / `into` was added.
- [ ] No branch-merging or loop declaration system for ancestor placement was invented.
- [ ] No final-use child extraction semantics were accepted.
- [ ] No copy-in/copy-out host mutation model was invented.
- [ ] No source-visible RC, retain/release, weak ownership, or finalizer semantics were added.
- [ ] No exact Rust enum or struct name was made architectural.
- [ ] No diagnostic code was reserved without the diagnostic registry owner.
- [ ] No backend-specific source-legality rule was added.
- [ ] No build-profile-specific source validity was added.
- [ ] No group identity entered `TypeId` or source signatures.
- [ ] No external profile retained ordinary Beanstalk references.
- [ ] No current implementation was presented as complete when deferred.
- [ ] No generated documentation was edited manually.
- [ ] No legacy monolith or competing semantic authority was restored.
