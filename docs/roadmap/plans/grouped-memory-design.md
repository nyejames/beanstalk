# Declared Lifetime Regions and Grouped Memory Design

Status: roadmap design draft  
Scope: accepted region direction plus one isolated unresolved ownership question, not an implementation plan  
Replacement target: `docs/roadmap/plans/grouped-memory-design.md`

## Purpose

Grouped memory gives the programmer a direct way to state that a set of fresh runtime values shares one lifetime owner and one destruction boundary.

A memory group is a **declared lifetime region**. It is not an allocator object or source-visible ownership type.

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
- reference counting

Reserved capacity remains a separate deferred design.

## Relationship to the existing memory model

Grouped memory extends, rather than replaces, Beanstalk's existing memory model:

- Existing values use shared read-only access by default.
- Independent duplication uses explicit `copy`.
- Mutation requires explicit exclusive access.
- Ownership transfer is compiler-inferred.
- Borrow validation is mandatory and backend-independent.
- HIR remains the backend-facing semantic IR.
- Borrow validation writes side-table facts and does not rewrite HIR.
- Allocation and release strategy remain backend decisions.
- GC remains a legal physical representation.

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

A compiler-inferred lifetime owner for ordinary ungrouped allocations. The compiler may merge, widen, split or physically ignore implicit regions when behaviour remains unchanged.

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

Group identity belongs to HIR and borrow or region facts. It must not affect:

- semantic type equality
- generic identity
- overload resolution
- public signatures
- package compatibility

### Copies separate lifetimes

`copy place` creates an independent value graph. The copy may be allocated into a different region and reclaimed independently from the source.

## Two region layers

### Implicit regions

Ordinary Beanstalk code uses compiler-inferred regions.

```beanstalk
make_label |name String| -> Label:
    return Label(text = [: [name]])
;
```

The compiler may allocate the result directly into a hidden caller-selected result region.

Implicit regions may be:

- local to one call
- attached to a returned value
- owned by an aggregate
- widened to an enclosing lifetime
- merged with another inferred region
- implemented through GC

The compiler may refine implicit region analysis without changing source semantics.

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

Grammar shape:

```text
name [access/type] [into group_name] = expression
```

`into group_name` appears after access or type syntax and before `=`.

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

### No group-to-group transfer in V1

A value cannot be extracted from one declared group and adopted by another.

Use one of:

- allocate the result directly into the destination group
- `copy` into the destination group
- place the whole graph in the correct common group from the start

This avoids graph splitting, alias rewriting and drop-list reparenting.

## Escapes

Borrow validation must reject every path where a group-owned value or alias can outlive the group.

Invalid escapes include:

- returning a group-owned value
- returning a projection or alias rooted in group-owned storage
- storing a group-owned value in a longer-lived local or aggregate
- storing a child-group value in a parent or sibling group
- assigning a group-owned value into longer-lived reactive storage
- passing a group-owned value to an external call that may retain it
- keeping a map lookup, collection element or field alias live after group exit
- creating a longer-lived retained edge into the group

Backend GC fallback does not legalise these cases.

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

Precise reclamation when the second observer ends requires one of:

- a common semantic owner that outlives both
- independent copies
- dynamic reachability tracking such as GC or RC

A one-bit ownership flag cannot discover which observer is last.

The intended direction is:

> Persistent sharing across sibling lifetime domains requires a known common semantic owner. The compiler must not manufacture two destruction owners for one allocation.

The common owner may be:

- an enclosing aggregate region
- a declared memory group
- a builder-owned page, mount, request or frame lifetime
- another finite compiler-proven parent lifetime

If no acceptable common owner exists, the programmer must create independent storage with `copy` or restructure the graph under one common group.

The exact boundary between implicit common-owner inference and required explicit grouping is the remaining design question described at the end of this document.

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
- external retention and capture effects

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

A GC backend may use ordinary reachability.

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

Builder-owned regions obey the same retained-edge rule as source groups, but they are created and ended by the project runtime rather than ordinary source blocks.

## External packages and resources

External function metadata must declare:

- borrowed for call duration
- may consume destruction responsibility
- may retain past the call
- returns fresh storage
- returns an alias
- copies the argument
- transfers into a returned external owner
- explicit close or teardown requirement

A group-owned value may be passed only when metadata proves it will not be retained past the call.

Unknown retention is a source diagnostic for grouped values. It is not a backend fallback.

External resources must use explicit lifecycle operations such as `close` when cleanup timing is observable. Object finalization must not depend on whether the backend uses GC, ownership or a region.

## Backend and build-profile behaviour

### Semantic parity

Every backend must accept and reject the same grouped-memory source.

Development and release builds must also agree on source validity and observable behaviour.

### JavaScript

JavaScript may lower:

- group allocation to ordinary host allocation
- group release to a no-op
- ownership drops to no-ops where host reachability owns storage

It must still enforce all frontend and borrow-validation group rules.

### Wasm and future native targets

An ownership-aware backend may choose:

- stack or inline storage
- direct caller-region allocation
- bump or segmented regions
- slabs or pools
- grouped drop lists
- ordinary per-value ownership
- GC fallback

The backend chooses from validated HIR, borrow facts, group facts, type layout, selected functions, target capabilities and optimisation profile.

### Collector-free artefacts

A project builder may determine after reachability and memory planning that a physical artefact requires no collector.

This is an artefact property, not a source mode or project-wide language contract.

A profile may spend more analysis effort to eliminate GC. Failure to optimise one value must not change source validity or observable behaviour.

A builder must not claim useful collector-free planning merely by retaining arbitrary runtime allocations until process termination. Truly static or page-lifetime data may use a root region, but repeated temporary allocations need bounded teardown regions.

## Compiler ownership

### AST

AST owns:

- parsing `group` and `into`
- group-name scope and collisions
- receiving-boundary placement syntax
- obvious position and freshness diagnostics
- stable group identity creation

Group identity must not enter `TypeId`.

### HIR

HIR records explicit group metadata and group exits.

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

HIR validation checks structural invariants only.

### Borrow validation

Borrow validation owns group safety and mandatory lifetime topology checks.

It must reject:

1. grouped placement of non-fresh or alias results
2. group-owned return escapes
3. projection or collection aliases live across group exit
4. parent or sibling retention of child-group values
5. external retention beyond the group
6. reactive retention beyond the group
7. cross-group cycles
8. invalid stored edges into shorter-lived regions
9. ordinary shared or exclusive access conflicts

Borrow validation remains conservative. Failure to prove an explicit group safe is a source diagnostic.

### Optional region and ownership planning

A later optimisation stage may consume validated HIR and borrow facts to choose:

- implicit region placement
- region widening or coalescing
- direct result allocation
- field-sensitive splitting
- last-use transfer
- group physical representation
- collector elimination

This stage must not invent source legality or mutate the meaning of borrow facts.

### Backend lowering

Backends consume explicit group, borrow, ownership and link-plan facts. They must not rediscover source group syntax or infer group identity from names.

There is no numbered compiler `Stage 7`. Backend lowering follows the compiler handoff and target planning described by the compiler and build-system authorities.

## Diagnostics

Grouped-memory diagnostics need stable codes and structured payloads.

Suggested families:

- `memory_group_unknown`
- `memory_group_name_collision`
- `memory_group_invalid_position`
- `memory_group_non_fresh_value`
- `memory_group_alias_result`
- `memory_group_return_escape`
- `memory_group_projection_escape`
- `memory_group_store_escape`
- `memory_group_nested_escape`
- `memory_group_cross_region_cycle`
- `memory_group_live_alias_at_exit`
- `memory_group_reactive_escape`
- `memory_group_external_retention`
- `memory_region_missing_common_owner`

Diagnostics should identify:

- the group declaration
- the grouped binding or allocation
- the retained edge or escaping use
- the shorter and longer lifetime owners
- the external metadata boundary where relevant

Help should prefer:

- allocate directly into the correct longer-lived group
- move both observers under one common group
- use `copy` for independent lifetime
- shorten the retained alias

There is no `memory_group_backend_unsupported` diagnostic. A backend may ignore physical grouping through GC while preserving the source contract.

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

## Roadmap order

A later implementation plan should separate:

1. source syntax and AST group identity
2. HIR group and exit metadata
3. borrow-validation escape and stored-edge rules
4. JS no-op lowering with full semantic enforcement
5. hidden fresh-result destination support
6. Wasm group release markers
7. implicit region and ownership planning
8. builder-owned page and mount regions
9. collector-elision verification for physical artefacts

## Final design decisions

- The source feature is named grouped memory or memory groups.
- A memory group is semantically a declared lifetime region.
- `group name:` creates a hard local lifetime and destruction boundary.
- `into group` appears on declaration receiving boundaries.
- Group membership is metadata, not type identity.
- Every allocation has one lifetime owner.
- Retained references may point only to storage in the same or a longer-lived region.
- Group-owned values may retain same-group and ancestor-group aliases.
- Parent and sibling regions must not retain child-group values.
- Group values and aliases must not escape.
- Explicit `copy` creates independent lifetime.
- Fresh calls may allocate directly into a hidden caller-selected destination region.
- Interior aliases remain rooted in their containing allocation family.
- Cross-region cycles are invalid.
- Same-region cycles are lifetime-safe, while direct source construction remains deferred.
- The ownership bit records destruction responsibility, not uniqueness or observer count.
- Group-owned values are normally borrowed at call boundaries because the group owns release.
- JavaScript may implement groups as GC no-ops while enforcing identical source rules.
- Profiles may change optimisation effort but never language semantics.
- Project builders may report collector-free physical artefacts without introducing a collector-free language mode.

## Remaining design question: sibling persistent sharing

The final unresolved rule is how much common-owner inference ordinary ungrouped code should receive before explicit grouping or copying becomes mandatory.

The strongest proposed rule is:

> Implicit region inference may follow one nested ownership chain, but it must not silently hoist a runtime allocation across independently ending sibling lifetime domains. Such sharing requires an explicit common group, a builder-owned common lifecycle or independent copies.

This rule would:

- prevent hidden promotion into an overly broad page or process region
- give every shared allocation a statically visible destruction owner
- make collector-free lowering predictable
- reject some programs that a tracing GC could safely execute
- require programmers to group or copy persistent sibling state

A weaker rule would let the compiler choose any finite common ancestor region automatically. That accepts more source but may retain values much longer than expected and makes collector-free memory bounds harder to predict.

The ownership bit cannot remove this choice. It can represent the one common owner once selected, or transfer responsibility at a proven handoff. It cannot identify the last of several independently ending observers.
