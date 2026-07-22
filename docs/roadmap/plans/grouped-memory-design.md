# Grouped Memory Implementation Roadmap

**Status:** accepted design; implementation deferred  
**Canonical semantics:** `docs/src/docs/codebase/memory-management/declared-memory-groups/`  
**Purpose:** implementation sequencing, prerequisites, current gaps, and deferred optimisation investigations

## 1. Purpose and status

Grouped memory gives the programmer a direct way to state that a set of fresh runtime values shares one lifetime owner and one destruction boundary. A memory group is a **declared semantic lifetime region**.

The accepted `group` / `into` surface and its full semantic contract are owned by `docs/src/docs/codebase/memory-management/declared-memory-groups/`. General topology rules, including the one-owner invariant, the stored-edge outlives rule, narrowing and widening, cycles and projection families, are owned by `docs/src/docs/codebase/memory-management/lifetime-regions-and-escape-validation/`. This file owns implementation sequencing, current gaps and deferred optimisation investigations only.

This design does not add lifetime annotations, source reference types, explicit move syntax, owned and borrowed signature variants, first-class region or arena values, allocator parameters, manual `free`, user-defined allocator traits, group identity in semantic `TypeId`, observable allocation identity, finalizers, weak references, reserved-byte syntax, source-visible RC or dynamic shared ownership. Internal RC remains a valid backend representation of already-legal topology. Reserved capacity remains a separate deferred design.

Implementation is deferred and not automatically active or queued.

## 2. Canonical design prerequisites

Before implementation can land, the canonical semantic contract must remain authoritative:

- one language semantics across JavaScript, Wasm and future targets
- identical source validity across development and release profiles
- lifetime parameters, reference types and allocator values kept out of source signatures
- explicit groups as hard proof boundaries rather than weak allocation hints
- ordinary ungrouped code ergonomic through implicit region inference and inferred ownership transfer
- `copy` to express independent lifetime when sharing cannot use one common owner
- external resource cleanup explicit rather than dependent on object destruction timing

Canonical design facts that must hold:

- `group name:` creates a hard local lifetime and destruction boundary
- `into group` appears on declaration receiving boundaries only in V1
- destination-group lexical ownership and visibility follow the declaration point through the remainder of the destination group
- placement may target current or ancestor group only
- group membership is metadata, not type identity
- every allocation has one lifetime owner
- retained references may point only to storage in the same or a longer-lived region
- implicit widening uses the nearest existing ancestor on one ordered owner chain
- no lateral promotion across independently ending sibling domains
- group-owned values may retain same-group and ancestor-group aliases
- parent and sibling regions must not retain child-group values
- group values and aliases must not escape
- explicit `copy` creates independent lifetime under the full graph-level copy contract
- fresh calls may allocate directly into a hidden caller-selected destination region
- interior aliases remain rooted in their containing allocation family
- cross-region cycles are invalid
- same-region cycles are lifetime-safe, while direct source construction remains deferred
- the ownership bit records destruction responsibility, not uniqueness or observer count
- group-owned values are normally borrowed at call boundaries because the group owns release
- borrow validation owns access conflicts and optional transfer safety
- lifetime-region and escape validation owns topology legality
- external bindings use closed WIT-value and host-binding profiles
- JavaScript may implement groups as GC no-ops while enforcing identical source rules
- internal RC is a permitted backend representation of already-legal topology
- source-visible RC remains outside the language
- profiles may change optimisation effort but never language semantics
- project builders may report collector-free physical artefacts without introducing a collector-free language mode

Full semantic detail and invalid/valid cases live in the canonical declared-groups leaf. This file does not repeat them.

## 3. Current implementation state

Implementation has not landed. The progress matrix records current support. Deferred work this roadmap sequences includes lifetime-region and escape validation, declared memory groups and WIT value-only component integration.

## 4. Compiler and build-system prerequisites

### Parser and AST group identity

AST owns, when implemented:

- parsing `group` and `into`
- group-name scope and collisions
- receiving-boundary placement syntax
- destination-group visibility and collision checks
- obvious position and freshness diagnostics
- stable group identity creation

Group identity must not enter `TypeId`. Implementation is deferred. See `docs/compiler-design-overview.md` for AST stage ownership and the deferred `group` / `into` paragraph under Stage 4.

### Destination-scope bookkeeping

Destination-scope and visibility bookkeeping must follow the narrow V1 ancestor-placement rule. Conditional production uses one declaration in the destination scope whose initializer is a value-producing `if`, match, or `catch`. Loop production mutates a destination-owned aggregate rather than repeatedly declaring an ancestor-owned name. See the declared-groups leaf for the exact placement-eligibility and destination-scope contract.

### HIR group metadata and exits

HIR records explicit group metadata and group exits when implemented. Conceptual structures:

```rust
pub struct HirMemoryGroup {
    pub id: MemoryGroupId,
    pub name: StringId,
    pub owner_region: RegionId,
    pub parent_group: Option<MemoryGroupId>,
    pub source_location: SourceLocation,
}

pub struct HirPlacement {
    pub group: MemoryGroupId,
    pub source_location: SourceLocation,
}
```

HIR records group declarations, parent and child relationships, placement sites, group-owned allocation candidates and all control-flow exits that close a group. HIR validation checks structural invariants only. HIR still does not decide exact lifetime topology.

### Local lifetime constraints and summaries

Local per-function and module analysis produces allocation, alias, retention, escape, result and outlives constraints plus exported summaries. It reads validated HIR and read-only borrow/effect facts, writes immutable side-table facts and summaries, and does not rewrite HIR or choose a physical allocation representation.

### Project/link summary instantiation

Project and link work instantiates local lifetime summaries over the reachable call graph and builder-supplied lifecycle roots. Local module compilation cannot validate every cross-module or builder-lifecycle relationship by itself. Backends receive a validated topology and may not reconsider source legality.

### JS no-op physical lowering with full semantic enforcement

JavaScript may lower group allocation to ordinary host allocation, group release to a no-op and ownership drops to no-ops where host reachability owns storage. It must still enforce all frontend, borrow-validation and lifetime-topology group rules.

### Hidden result destinations

Fresh-result functions must support destination-directed lowering. The hidden result destination is not part of the source signature, not a region parameter visible to generics or callers, not a source lifetime parameter, may be ignored by a GC backend and lets an optimising backend allocate the fresh result graph directly into the caller's region. A fresh-result summary must be backend-neutral and part of the function's semantic effect information.

### Wasm group release markers

An ownership-aware backend may choose stack or inline storage, direct caller-region allocation, bump or segmented regions, slabs or pools, grouped drop lists, ordinary per-value ownership, internal RC for legal topology or GC representation. It consumes validated HIR, borrow facts, lifetime facts, group facts, type layout, selected functions, target capabilities and optimisation profile.

### Builder-owned page and mount roots

Builder-supplied page, mount, request, frame and arena roots are lifecycle inputs. They obey the same retained-edge rule as source regions. Builders supply lifecycle roots but cannot change source legality. Reactive storage that outlives a lexical function must still satisfy one lifetime owner and the stored-edge outlives rule.

### Collector-elision verification

A project builder may determine after reachability and memory planning that a physical artefact requires no collector. This is an artefact property, not a source mode or project-wide language contract. A profile may spend more analysis effort to eliminate GC. Failure to optimise one value must not change source validity or observable behaviour.

## 5. Proposed implementation phases

A later concrete implementation plan should separate:

1. source syntax and AST group identity
2. HIR group and exit metadata
3. lifetime-region and escape validation for group and ungrouped topology
4. borrow-validation access interaction with group-owned places
5. JS no-op lowering with full semantic enforcement
6. hidden fresh-result destination support
7. Wasm group release markers
8. implicit region and ownership planning
9. builder-owned page and mount regions
10. collector-elision verification for physical artefacts

## 6. Validation and diagnostics requirements

Lifetime and group diagnostics are part of the memory model. They must distinguish:

- topology proven invalid
- topology not proven legal by conservative analysis
- invalid group syntax or placement
- non-copyable graph contents
- unsupported external boundary profile
- missing or inconsistent compiler-owned metadata

User-facing diagnostics use stable codes and structured reason payloads. Internal impossible or inconsistent metadata uses `CompilerError`.

Required diagnostic coverage:

- invalid group name or position
- non-fresh placement
- alias result placement
- return or projection escape
- store escape
- nested escape
- cross-region cycle
- live alias at exit
- reactive escape
- external retention
- missing common owner

Remedy order remains:

1. allocate directly into the required destination region
2. place observers under one common group
3. create independent storage with `copy`
4. shorten the alias or retained edge
5. repair package-owned external lifetime metadata

There is no backend-specific escape from semantic lifetime diagnostics. A backend may ignore physical grouping through GC while preserving the source contract.

## 7. Deferred optimisation investigations

Deferred extensions include reserved-byte or preallocation syntax, safe adoption of an ungrouped uniquely owned value into a group, safe extraction or movement between declared groups, expression-site placement, group-local graph construction and publication, direct reference-cycle construction, builder lifecycle region metadata for reactivity, field-sensitive region splitting, and physical region layout and runtime ABI encoding.

A later optimisation stage may consume validated HIR, borrow facts and lifetime facts to choose implicit region placement, region widening or coalescing within legal bounds, direct result allocation, field-sensitive splitting, last-use transfer, group physical representation and collector elimination. This stage must not invent source legality or mutate the meaning of borrow or lifetime facts.

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

## 8. Relationship to ownership-aware Wasm completion

Grouped-memory implementation should be evaluated as a likely prerequisite for the full ownership-aware Wasm completion plan. This document does not mark that work active or queued.

## 9. Not active or queued

The region/group work, lifetime-region and escape validation, declared memory groups and WIT value-only component integration remain deferred. They are not automatically active or queued. A separate approved implementation plan must activate a phase before it begins.