# Declared Lifetime Regions and Grouped Memory Design

**Status:** accepted end-state design; implementation deferred  
**Scope:** canonical grouped-memory and declared lifetime-region design, not an implementation plan

## Purpose

Grouped memory gives the programmer a direct way to state that a set of fresh runtime values shares one lifetime owner and one destruction boundary.

A memory group is a **declared semantic lifetime region**. It is not an allocator object, value, type, field, parameter, generic argument, trait or lifetime annotation.

Canonical general topology rules live in:

```text
docs/src/docs/codebase/memory-management/lifetime-regions-and-escape-validation/
```

This file owns the accepted `group` / `into` surface and its interaction with that topology model.

```beanstalk
group request:
    parsed ParsedPost into request = parse_post(post)
    html String into request = render_post(parsed)
;
```

The source contract is:

> Values placed into a group belong to that group for their full lifetime. They may refer to values owned by the same group or an ancestor group. No group-owned value or alias may outlive the group.

A backend may realise the contract with:

- stack storage
- direct destination allocation
- a bump arena
- a segmented region
- a slab or pool
- a grouped drop list
- ordinary per-value ownership
- host or runtime GC
- internal reference counting for already-legal topology

The representation is not observable in Beanstalk source.

## Design goals

- Preserve one language semantics across JavaScript, Wasm and future targets.
- Preserve identical source validity across development and release profiles.
- Keep lifetime parameters, reference types and allocator values out of source signatures.
- Give optimising builders a static route to collector-free artefacts.
- Make explicit groups hard proof boundaries rather than weak allocation hints.
- Keep ordinary ungrouped code ergonomic through implicit region inference and inferred ownership transfer.
- Let `copy` express independent lifetime when sharing cannot use one common owner.
- Keep external resource cleanup explicit rather than dependent on object destruction timing.

## Non-goals

This design does not add:

- lifetime annotations or lifetime parameters
- source reference types
- explicit move syntax
- owned and borrowed signature variants
- first-class region or arena values
- allocator parameters
- manual `free`, `reset` or region destruction
- user-defined allocator traits
- group identity in semantic `TypeId`
- observable object addresses or allocation identity
- finalizers, weak references or resurrection
- reserved-byte or capacity syntax
- general object-graph relocation as a required runtime mechanism
- source-visible RC or dynamic shared-ownership surface

Internal RC remains a valid backend representation of already-legal topology.

Reserved capacity remains a separate deferred design.

## Relationship to the existing memory model

**Beanstalk is reference-semantic by default, copy-explicit and move-inferred. It omits explicit reference types and lifetime syntax, not references themselves.**

Grouped memory extends, rather than replaces, Beanstalk's existing memory model:

- Existing values use shared read-only access by default.
- Independent duplication uses explicit `copy`.
- Mutation requires explicit exclusive access.
- Ownership transfer is compiler-inferred and optional.
- Borrow validation is mandatory and backend-independent.
- Lifetime-region and escape validation is mandatory and backend-independent.
- HIR remains the backend-facing semantic IR.
- Borrow validation and lifetime analysis write side-table facts and do not rewrite HIR.
- Allocation and release strategy remain backend decisions.
- GC remains a legal physical representation for already-legal topology.

GC must not weaken group escape rules. A GC-only backend may ignore physical group release, but it must accept and reject the same grouped source as every other backend.

Build profile may change analysis depth and physical allocation strategy. It must not change:

- accepted source
- alias semantics
- mutation legality
- function contracts
- group escape legality
- observable cleanup behaviour

## Terminology

### Lexical scope

A source and HIR scope that controls name visibility and control-flow exits.

### Implicit lifetime region

A compiler-inferred lifetime owner for ordinary ungrouped allocations. The compiler may merge, widen, split or physically ignore implicit regions when behaviour remains unchanged, subject to the nearest-existing-ancestor rule.

### Declared lifetime region

A source `group name:` block. It is a hard lifetime boundary. The compiler must not silently widen it to make an invalid escape legal.

### Physical arena

One possible backend representation for one or more lifetime regions. An arena is not a source value and is not synonymous with a semantic region.

### Runtime reference

A target representation that locates a runtime value, such as a JavaScript object reference, Wasm offset, pointer or table index.

### Destruction responsibility

The obligation to release or safely transfer storage when deterministic release is enabled. The unified ownership bit represents this responsibility. It does not prove uniqueness.

### Lifetime owner

The one semantic region responsible for keeping an allocation alive and eventually ending its lifetime.

## Core lifetime invariants

### One lifetime owner

Every runtime allocation has exactly one lifetime owner.

Several bindings, fields or collection elements may alias the allocation. They do not become additional lifetime owners merely by storing a reference to it.

```text
Page region owns Theme

header.theme ─┐
footer.theme ──┼── Theme allocation
theme binding ─┘
```

### Stored-edge outlives rule

Let `A >= B` mean that lifetime region `A` lives at least as long as lifetime region `B`.

For an object in `R_container` to retain a reference to a value in `R_value`, the compiler must establish:

```text
R_value >= R_container
```

A shorter-lived object may retain a reference to longer-lived storage.

A longer-lived object must not retain a reference to shorter-lived storage.

### Ownership is not uniqueness

An owned runtime path carries destruction responsibility. Borrowed paths may still alias the same allocation.

```text
owned path ─────┐
borrowed path ──┼── allocation
borrowed path ──┘
```

Destruction is legal only when the compiler has also proved that no live borrowed path can still observe the allocation.

### Region membership is not type identity

`String into scratch` remains `String`.

Group identity belongs to HIR and lifetime-region facts. It must not affect:

- semantic type equality
- generic identity
- overload resolution
- public signatures
- package compatibility

### Copies separate lifetimes

`copy place` creates an independent value graph. The copy may be allocated into a different region and reclaimed independently from the source.

See `docs/src/docs/codebase/memory-management/access-and-aliasing/access-and-aliasing.bd` for the complete graph-level contract:

- deep copy of the complete copyable runtime value graph
- internal alias topology preserved
- same-region cycles preserved
- no mutable sharing with the source graph
- reactive sources copied as current values, not reactive identities
- external opaque handles non-copyable by default
- non-copyable graph members produce source diagnostics
- destination-region allocation permitted

## Two region layers

### Implicit regions

Ordinary Beanstalk code uses compiler-inferred regions.

```beanstalk
make_label |name String| -> Label:
    return Label(text = [: [name]])
;
```

The compiler may allocate the result directly into a hidden caller-selected result region.

Implicit region inference may widen an allocation only to the nearest existing ancestor on the same ordered owner chain that outlives every retained observer. It may not silently promote an allocation laterally across independently ending sibling lifetime domains.

The compiler must not invent a page-, application- or process-lifetime owner merely to avoid a diagnostic. Declared groups are never implicitly widened.

Sharing across sibling domains requires one of:

1. an already-existing common semantic owner
2. an enclosing declared memory group
3. a builder-declared common lifecycle
4. independent storage created by `copy`

### Declared groups

A declared group states a hard destruction family.

```beanstalk
group scratch:
    parsed ParsedPost into scratch = parse_post(post)
    html String into scratch = render_post(parsed)
;
```

The compiler may choose the physical representation, but it must preserve these source facts:

- the group has one lexical entry and explicit exits
- values placed into the group cannot escape it
- the group is not silently widened
- a child group ends before its parent
- a group cannot be passed, returned or stored

## Source syntax

### Group block

```beanstalk
group scratch:
    ...
;
```

Rules:

- The group name is local to the current executable body scope.
- It cannot collide with a visible value, type, import, constant, reactive source or active group.
- A group is not a value.
- Groups are valid only in runtime executable bodies.
- Groups are invalid in constants, config, signatures, fields, choices, traits and export surfaces.
- The group closes on every path leaving the block, including fallthrough, `return`, `return!`, `break`, recovery and checked-operation failure.

### Binding placement

Grouped placement belongs to a declaration receiving boundary.

```beanstalk
parsed into scratch = parse_post(post)
parsed ParsedPost into scratch = parse_post(post)
rows ~{Row} into scratch = {}
maybe_name String? into scratch = find_name(id)?
```

V1 placement syntax:

```text
name [access/type] into group_name = expression
```

`into group_name` appears after access or type syntax and before `=`.

Placement may target the current group or a lexically enclosing ancestor group, never a sibling or unrelated group.

### Binding visibility and destination scope

- A binding's lexical owner follows its destination group.
- A binding declared inside a nested block but placed into an ancestor group remains visible from its declaration point through the remainder of that destination group.
- Visibility begins at the declaration point, not at the group opening.
- Name collisions are checked in the destination group scope.
- Ordinary declarations without `into` retain normal lexical visibility.

### No expression-site placement in V1

Prefer:

```beanstalk
row Row into scratch = parse_row(raw)
~rows.push(row)!
```

Do not initially add:

```beanstalk
~rows.push(parse_row(raw) into scratch)!
```

Placement remains attached to closed receiving boundaries.

### No reassignment placement in V1

```beanstalk
value into scratch = make_value()
value = make_other_value()
```

A mutable binding declared into a group remains group-owned.

V1 reassignment accepts only:

- a fresh result allocated into the same group
- an explicit copy allocated into the same group
- a same-group value transferred at a proven final use

It does not let a group-owned binding switch into a borrowed alias of ancestor, sibling or external storage. Use a separate ordinary alias binding for that purpose.

V1 has no extraction or unrestricted group-to-group adoption.

## Freshness and direct destination allocation

A value may be placed `into group` only when it is fresh or proven to carry independent fresh storage.

Fresh sources include:

- string templates
- collection literals
- map literals
- struct and choice constructors
- compiler or builtin constructors with fresh-result semantics
- function results summarised as fresh
- explicit `copy` results

A result that may alias a parameter, aggregate field, external state or another result is not fresh.

### Hidden result destination

Fresh-result functions must support destination-directed lowering.

```beanstalk
html String into request = render_post(parsed)
```

Conceptually, an ownership-aware backend may lower this as:

```text
render_post(parsed, hidden_result_destination = request)
```

The hidden destination:

- is not part of the source signature
- is not a region parameter visible to generics or callers
- is not a source lifetime parameter
- may be ignored by a GC backend
- lets an optimising backend allocate the fresh result graph directly into the caller's region

A fresh-result summary must be backend-neutral and part of the function's semantic effect information.

## Access inside a group

A group-owned value behaves like an ordinary Beanstalk value while the group is live.

- Shared access follows normal shared-read rules.
- Mutation requires normal exclusive `~place` access.
- Alias liveness remains control-flow-sensitive.
- Calls use normal parameter access contracts.
- The group owns the value's lifetime even when individual runtime references are borrowed.
- GC does not relax exclusivity.

Group membership does not create implicit mutability or an alternative borrow discipline.

## Nested groups and retained edges

Nested groups are valid.

```beanstalk
group request:
    config Config into request = load_config()

    group scratch:
        parsed ParsedPost into scratch = parse_post(post)
        html String into request = render_post(parsed, config)
    ;

    use(html)
;
```

For a child group nested in a parent group:

| Retained edge | Rule |
|---|---|
| child value -> parent value | valid |
| child value -> same-child value | valid |
| parent value -> child value | invalid |
| sibling value -> sibling-group value | invalid |
| child value -> unrelated shorter-lived value | invalid |

A group-owned aggregate may retain:

- fresh values owned by the same group
- same-group aliases
- shared aliases to ancestor-group values
- ordinary longer-lived external values whose retention contract is known

It must not retain values owned by a child, sibling or otherwise shorter-lived region.

Parent, sibling or otherwise longer-lived regions must not retain child-group values.

### No group-to-group transfer in V1

A value cannot be extracted from one declared group and adopted by another.

Use one of:

- allocate the result directly into the destination group
- `copy` into the destination group
- place the whole graph in the correct common group from the start

This avoids graph splitting, alias rewriting and drop-list reparenting.

## Escapes

Lifetime-region and escape validation must reject every path where a group-owned value or alias can outlive the group.

Invalid escapes include:

- returning a group-owned value
- returning a projection or alias rooted in group-owned storage
- storing a group-owned value in a longer-lived local or aggregate
- storing a child-group value in a parent or sibling group
- assigning a group-owned value into longer-lived reactive storage
- passing a group-owned value to an external call that may retain it
- keeping a map lookup, collection element or field alias live after group exit
- creating a longer-lived retained edge into the group

Backend GC representation does not legalise these cases.

### Crossing a group boundary

A value crosses a group boundary only by creating independent storage in the destination lifetime.

```beanstalk
group request:
    group scratch:
        label String into scratch = [: temporary]
        saved String into request = copy label
    ;

    use(saved)
;
```

A fresh producer may avoid the copy by allocating directly into the destination group.

## Interior field and element aliases

### Ordinary ungrouped code

A field or element alias remains rooted in its containing allocation family.

```beanstalk
name_of |user User| -> String:
    return user.name
;
```

The return summary records that the result aliases parameter `user` through `.name`.

The compiler may preserve the result by:

- keeping the containing allocation region alive
- allocating the base graph in the caller's result region
- extracting an independently owned child at a proven final use
- applying field-sensitive allocation splitting
- using GC as the physical representation

The projection does not silently become an independent copy.

### Declared groups

An interior alias cannot escape its group by itself.

```beanstalk
load_name |id String| -> String, Error!:
    group scratch:
        user User into scratch = load_user(id)!
        return user.name -- invalid
    ;
;
```

Use `copy` or produce a fresh value directly in a longer-lived group.

## Persistent sharing and common lifetime ownership

Multiple bindings or aggregates may observe one allocation. The allocation still has one lifetime owner.

```beanstalk
group page:
    theme Theme into page = load_theme()
    header Header into page = Header(theme = theme)
    footer Footer into page = Footer(theme = theme)
;
```

Both fields borrow from the page-owned `Theme`.

### Same aggregate or one enclosing graph

Sharing is straightforward when every observer belongs to one enclosing lifetime graph.

Examples include:

- two fields of one aggregate
- several children of one page state
- aliases local to one call
- several results consumed under one caller region

### Sibling persistent lifetime domains

The difficult topology is one allocation retained by observers with independently ending runtime lifetimes.

```text
Header mount ─┐
              ├── Theme
Footer mount ─┘
```

Implicit region inference may widen an allocation only to the nearest existing ancestor on the same ordered owner chain that outlives every retained observer. It may not silently promote an allocation laterally across independently ending sibling lifetime domains.

The compiler cannot invent an overly broad owner merely to accept the program.

If no acceptable common owner exists, the programmer must create independent storage with `copy` or restructure the graph under one common group or builder lifecycle.

## Cycles

For a retained edge `A -> B`, `B`'s region must outlive `A`'s region.

For a cycle:

```text
A -> B
B -> A
```

both regions must outlive each other. They must therefore be the same lifetime region.

### Cross-region cycles

Cross-region cycles are invalid.

```beanstalk
group graph:
    outer ~Node into graph = Node(next = none)

    group scratch:
        inner ~Node into scratch = Node(next = outer)
        outer.next = inner -- invalid
    ;
;
```

The longer-lived `outer` cannot retain the shorter-lived `inner`.

### Same-region cycles

A complete strongly connected allocation graph may be reclaimed as one region.

The lifetime model therefore permits cycles only when all members belong to one lifetime region.

Direct alias-cycle construction remains deferred because ordinary exclusive access may prevent publishing back-edges safely. V1 should support cyclic domain graphs through scalar IDs, indexes or keys inside one group. A later construction-phase design may add partially initialised or unpublished group graphs if real programs justify it.

## Returns and function summaries

Groups never appear in source signatures.

Functions export backend-neutral facts sufficient for callers to perform lifetime planning:

- fresh result
- result aliasing one or more parameters
- projection path where useful
- unknown or external alias result
- result-to-result alias relationships for multi-return functions
- results or receiver storage that retain parameter aliases
- required outlives constraints such as `region(parameter) >= region(result)`
- possible parameter consumption
- external retention and capture effects under closed boundary profiles

Examples:

```beanstalk
make_user |name String| -> User
```

```text
result 0 is fresh
```

```beanstalk
name_of |user User| -> String
```

```text
result 0 aliases parameter 0 through `.name`
```

A caller may place only fresh results directly into a group. Alias results remain tied to their source lifetime owner. A fresh result that retains a parameter is valid only when the parameter's region outlives the selected result region.

## Ownership ABI integration

### Meaning of the ownership bit

The ownership bit means:

```text
borrowed
    This runtime path must not destroy the value.

owned
    This runtime path carries destruction responsibility.
```

It does not mean:

- this is the only alias
- this allocation is individually releasable
- this value is not region-owned
- this value is not GC-managed

Allocation class and group membership remain separate static or runtime facts.

### Group-owned values at calls

A memory group owns release responsibility for its storage family.

Group-owned values passed to ordinary functions are normally passed as borrowed. The callee may read or mutate according to the source access contract, but it must not individually destroy the group-owned allocation.

Fresh results requested into the group use the hidden result destination.

### What the bit can solve

The bit supports:

- inferred last-use transfer
- branch-dependent ownership
- one owned path plus several shorter-lived borrowed aliases
- conditional `drop_if_owned`
- unified borrowed and owned call ABI

It can solve sibling-looking cases when analysis proves they are mutually exclusive or ownership is handed off at a known point.

### What the bit cannot solve

The bit cannot by itself reclaim a value shared by independently ending observers.

If the owned observer ends while a borrowed observer remains, the value cannot be released. Transferring ownership to an unknown remaining observer requires dynamic observer discovery, a count or a central common owner.

The ownership bit therefore complements the common-owner region rule. It does not replace it.

## Reactive and builder-owned lifetimes

Reactive storage can outlive the lexical function that creates it. V1 must reject:

- reactive declarations placed directly into an ordinary lexical memory group
- assigning group-owned values into reactive storage that outlives the group
- subscriptions or mounted state retaining group-owned aliases past group exit

A GC backend may use ordinary reachability for already-legal topology.

A future collector-free HTML-Wasm path should use builder-owned lifecycle regions such as:

```text
Page region
├── page reactive state
├── Mount A region
│   ├── subscriptions
│   └── render-generation region
└── Mount B region
    ├── subscriptions
    └── render-generation region
```

Builder-owned regions obey the same retained-edge rule as source groups, but they are created and ended by the project runtime rather than ordinary source blocks. Builders cannot change source legality.

## External lifetime boundaries

External bindings use closed semantic boundary profiles. They do not expose arbitrary user-defined lifetime graphs.

### WIT value-only V1 profile

Beanstalk imports general external Wasm libraries through a value-only WIT component profile.

- Supported arguments are lowered from shared reads into independent component values.
- Results are lifted into fresh Beanstalk values.
- No Beanstalk alias, lifetime owner, ownership state or destruction responsibility crosses the component boundary.
- The source call is not consuming.
- Group-owned values cannot cross a value-only boundary as aliases; value conversion creates independent foreign values.
- WIT resources, handles, callbacks, async, futures, streams, shared memory, pointers, returned aliases and retained Beanstalk references are rejected by the V1 profile.

### Restricted host-binding profile

Existing Core, Builder, JavaScript-provider and curated platform bindings use a restricted host-binding profile.

- Ordinary Beanstalk values cross by value.
- Host code may not retain references into ordinary Beanstalk storage.
- Opaque handles represent foreign identities rather than Beanstalk reference types.
- Observable external resources require explicit close or teardown operations.
- Host or component finalization timing must not define observable language behaviour.

Richer resource-capable profiles are deferred and require a separate complete design.

## Backend and build-profile behaviour

### Semantic parity

Every backend must accept and reject the same grouped-memory source.

Development and release builds must also agree on source validity and observable behaviour. Profiles differ only in optimisation effort.

### JavaScript

JavaScript may lower:

- group allocation to ordinary host allocation
- group release to a no-op
- ownership drops to no-ops where host reachability owns storage

It must still enforce all frontend, borrow-validation and lifetime-topology group rules.

### Wasm and future native targets

An ownership-aware backend may choose:

- stack or inline storage
- direct caller-region allocation
- bump or segmented regions
- slabs or pools
- grouped drop lists
- ordinary per-value ownership
- internal RC for legal topology
- GC representation

The backend chooses from validated HIR, borrow facts, lifetime facts, group facts, type layout, selected functions, target capabilities and optimisation profile.

### Collector-free artefacts

A project builder may determine after reachability and memory planning that a physical artefact requires no collector.

This is an artefact property, not a source mode or project-wide language contract.

A profile may spend more analysis effort to eliminate GC. Failure to optimise one value must not change source validity or observable behaviour.

## Compiler ownership

### AST

AST owns, when implemented:

- parsing `group` and `into`
- group-name scope and collisions
- receiving-boundary placement syntax
- destination-group visibility and collision checks
- obvious position and freshness diagnostics
- stable group identity creation

Group identity must not enter `TypeId`. Implementation is deferred.

### HIR

HIR records explicit group metadata and group exits when implemented.

Conceptual structures:

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

HIR records:

- group declarations
- parent and child relationships
- placement sites
- group-owned allocation candidates
- all control-flow exits that close a group

HIR validation checks structural invariants only. HIR still does not decide exact lifetime topology.

### Borrow validation

Borrow validation owns access conflicts and optional transfer safety.

It does not own group escape, retained-edge, outlives, cycle or common-owner legality.

### Lifetime-region and escape validation

Lifetime-region and escape validation owns group escape, retained-edge, outlives, cycle and common-owner legality.

Local per-function/module analysis produces constraints and exported summaries. Project/link analysis instantiates those summaries over the reachable call graph and builder-supplied lifecycle roots.

Facts are immutable side tables. HIR is unchanged. There is no numbered compiler Stage 7.

### Optional region and ownership planning

A later optimisation stage may consume validated HIR, borrow facts and lifetime facts to choose:

- implicit region placement
- region widening or coalescing within legal bounds
- direct result allocation
- field-sensitive splitting
- last-use transfer
- group physical representation
- collector elimination

This stage must not invent source legality or mutate the meaning of borrow or lifetime facts.

### Backend lowering

Backends consume explicit group, borrow, lifetime, ownership and link-plan facts. They must not rediscover source group syntax or infer group identity from names.

Backend lowering follows the compiler handoff and target planning described by the compiler and build-system authorities.

## Diagnostics

Lifetime and group diagnostics are part of the memory model, not an afterthought. Diagnostic quality is part of the rationale for rejecting source-visible RC.

Diagnostics must distinguish:

- topology proven invalid
- topology not proven legal by conservative analysis
- invalid group syntax or placement
- non-copyable graph contents
- unsupported external boundary profile
- missing or inconsistent compiler-owned metadata

User-facing diagnostics use stable codes and structured reason payloads. Internal impossible or inconsistent metadata uses `CompilerError`.

Conceptual diagnostic families include invalid group name or position, non-fresh placement, alias result placement, return or projection escape, store escape, nested escape, cross-region cycle, live alias at exit, reactive escape, external retention and missing common owner.

Every diagnostic should identify, where applicable:

- the group declaration
- the grouped binding or allocation
- the retained edge or escaping use
- the shorter and longer lifetime owners
- the external metadata boundary where relevant
- the failed rule

Remedies must be ranked in this order:

1. allocate directly into the required destination region
2. place observers under one common group
3. create independent storage with `copy`
4. shorten the alias or retained edge
5. repair package-owned external lifetime metadata

There is no backend-specific escape from semantic lifetime diagnostics. A backend may ignore physical grouping through GC while preserving the source contract.

## Deferred extensions

Deferred work includes:

- reserved-byte or preallocation syntax
- safe adoption of an ungrouped uniquely owned value into a group
- safe extraction or movement between declared groups
- expression-site placement
- group-local graph construction and publication
- direct reference-cycle construction
- builder lifecycle region metadata for reactivity
- field-sensitive region splitting
- physical region layout and runtime ABI encoding

## Future implementation roadmap

A later implementation plan should separate:

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

Grouped-memory implementation should be evaluated as a likely prerequisite for the full ownership-aware Wasm completion plan. This document does not mark that work active or queued.

## Final design decisions

- The source feature is named grouped memory or memory groups.
- A memory group is semantically a declared lifetime region.
- `group name:` creates a hard local lifetime and destruction boundary.
- `into group` appears on declaration receiving boundaries only in V1.
- Destination-group lexical ownership and visibility follow the declaration point through the remainder of the destination group.
- Placement may target current or ancestor group only.
- Group membership is metadata, not type identity.
- Every allocation has one lifetime owner.
- Retained references may point only to storage in the same or a longer-lived region.
- Implicit widening uses the nearest existing ancestor on one ordered owner chain.
- No lateral promotion across independently ending sibling domains.
- Group-owned values may retain same-group and ancestor-group aliases.
- Parent and sibling regions must not retain child-group values.
- Group values and aliases must not escape.
- Explicit `copy` creates independent lifetime under the full graph-level copy contract.
- Fresh calls may allocate directly into a hidden caller-selected destination region.
- Interior aliases remain rooted in their containing allocation family.
- Cross-region cycles are invalid.
- Same-region cycles are lifetime-safe, while direct source construction remains deferred.
- The ownership bit records destruction responsibility, not uniqueness or observer count.
- Group-owned values are normally borrowed at call boundaries because the group owns release.
- Borrow validation owns access conflicts and optional transfer safety.
- Lifetime-region and escape validation owns topology legality.
- External bindings use closed WIT-value and host-binding profiles.
- JavaScript may implement groups as GC no-ops while enforcing identical source rules.
- Internal RC is a permitted backend representation of already-legal topology.
- Source-visible RC remains outside the language.
- Profiles may change optimisation effort but never language semantics.
- Project builders may report collector-free physical artefacts without introducing a collector-free language mode.
