# Grouped Memory Design

Status: roadmap design groundwork  
Scope: accepted language and compiler design direction, not an implementation plan

## Purpose

Grouped memory gives the programmer a direct way to state that a set of fresh runtime values share one destruction boundary.

The feature is not an arena API. It is a general language-level grouping mechanism that lets the compiler and backend choose the best allocation and release strategy for values that are proven to die together.

A memory group says:

> Fresh values placed into this group must not outlive the group. They may be allocated, tracked and released as one lifetime family when the backend can do so safely.

The feature should support arena-like allocation, pools, slab-like storage, stack-like frames, grouped drop lists or GC fallback without exposing those representations in the source language.

## Non-goals

This design does not add:

- lifetime parameters
- reference types
- explicit move syntax
- allocator objects
- first-class arena values
- manual free or reset
- user-defined allocator traits
- region types in signatures
- grouped memory as semantic `TypeId` identity
- capacity or `reserved_bytes` syntax

Preallocation with explicit reserved bytes remains deferred until the source syntax is designed separately.

## Design fit

Grouped memory extends the existing Beanstalk memory model:

- GC remains the semantic baseline.
- Borrow validation remains mandatory and backend independent.
- Ownership-aware lowering remains an optimisation.
- HIR remains the backend-facing semantic IR.
- Borrow validation writes side-table facts rather than rewriting HIR.
- Backend lowerers consume validated HIR, borrow facts and explicit link or target plans.

A group is a local destruction and allocation-planning fact. It is not a type-system feature and it must not leak into public interfaces.

## Why this is different from regular scope

A regular lexical scope controls name visibility and ordinary ownership cleanup. A memory group controls allocation-family membership.

Normal scope:

```beanstalk
render || -> String:
    temp = parse_input()
    page = render_page(temp)
    return page
;
```

The compiler may drop locals at scope exit or use GC fallback. Each heap object remains independently allocated and independently managed unless later optimisation proves otherwise.

Grouped memory:

```beanstalk
group scratch:
    parsed ParsedPost into scratch = parse_post(post)
    html String into scratch = render_post(parsed)
;
```

The programmer states that the fresh values placed `into scratch` belong to one lifetime family. A backend may lower that family to a bump region, slab pool, segmented arena, stack-like storage, drop-list-backed region or GC no-op.

The main win is not that names go out of scope. The main win is explicit, safe allocation grouping across nested calls, loops and aggregates where ordinary lexical scope does not express enough allocation intent.

## Source syntax

### Group block

```beanstalk
group scratch:
    ...
;
```

A group block creates a named scoped destruction boundary.

Rules:

- The group name is local to the current body scope.
- The group name cannot collide with any visible value, type, import, constant, reactive source or other active group name.
- The group is not a value and cannot be passed, stored, returned or imported.
- A group closes on every control-flow path that exits the block, including fallthrough, `return`, `return!`, `break`, `catch` recovery and checked-operation failure branches.
- Groups are valid only in runtime executable bodies.
- Groups are invalid in constants, top-level declarations, signatures, type aliases, struct fields, choice payloads, trait declarations and export surfaces.

### Binding placement

Grouped placement belongs to the receiving binding boundary.

```beanstalk
parsed into scratch = parse_post(post)
parsed ParsedPost into scratch = parse_post(post)
rows ~{Row} into scratch = {}
maybe_name String? into scratch = find_name(id)?
```

The grammar shape is:

```text
name [access/type] [into group_name] = expression
```

`into group_name` appears after any explicit access or type annotation and before `=`.

Examples:

```beanstalk
name into request = render_name(user)
name String into request = render_name(user)
items ~{Item} into request = {}
```

The type remains the ordinary semantic type. `String into scratch` does not create a new type. It means the fresh value received by this binding is group-owned.

### Reassignment

V1 should not add grouped placement to reassignment.

```beanstalk
value into scratch = make_value() -- declaration
value = make_other_value()        -- ordinary reassignment
```

A mutable binding that was declared into a group remains group-owned. Later assignments to that binding must preserve the binding's group ownership. If the new value is fresh and eligible, it is placed into the same group. If it aliases or comes from a longer-lived owner, borrow validation must reject or require `copy` according to normal ownership and alias rules.

### Expression-site placement

Expression-site placement should not be part of the initial source surface.

Prefer:

```beanstalk
row Row into scratch = parse_row(raw)
~rows.push(row)!
```

Avoid in V1:

```beanstalk
~rows.push(parse_row(raw) into scratch)!
```

This keeps `into` attached to closed receiving boundaries, matching Beanstalk's existing bias toward contextual typing at declarations, assignments, returns and parameters rather than free expression operators.

A later implementation may add expression-site placement only if real code needs it and the diagnostics remain clear.

## Core semantics

### Freshness

A binding placement is valid only when the received value is fresh or proven to carry fresh owned storage.

Fresh sources include:

- string templates
- collection literals
- map literals
- struct or choice constructors
- builtin constructors that produce owned runtime values
- function calls whose return metadata proves a fresh result
- explicit `copy` results

A call result may be placed into a group only when the callee summary proves that the result is fresh. If the result may alias a parameter, stored value or external state, grouped placement is rejected unless the source uses `copy` to create independence.

### Group ownership

A group-owned value behaves like an ordinary Beanstalk value until the group closes.

Within the group:

- shared access follows ordinary shared-read rules;
- mutation requires ordinary `~place` access;
- aliases are tracked through borrow validation;
- collections and maps own stored values according to existing aggregate rules;
- function calls use normal access contracts.

At group exit, no live access path may remain to any value owned by that group.

### Escapes

Borrow validation must reject any path where a group-owned value can outlive its group.

Invalid escapes include:

- returning a group-owned value;
- returning a value that aliases group-owned storage;
- storing a group-owned value in a longer-lived local, collection, map, struct field, choice payload or reactive source;
- passing a group-owned value to an external function that may retain it;
- storing a child-group value in a parent or sibling group;
- keeping a map `get` or projection alias live after the group closes.

Explicit `copy` creates an independent value and may cross the boundary when the receiving context allows it.

```beanstalk
group scratch:
    label String into scratch = [: temporary]
    saved String = copy label
;
```

### Nested groups

Nested groups are valid.

```beanstalk
group request:
    parts ~{String} into request = {}

    loop posts |post|:
        group scratch:
            parsed ParsedPost into scratch = parse_post(post)
            html String into request = render_post(parsed)
            ~parts.push(html)!
        ;
    ;
;
```

Rules:

- A child group closes before its parent.
- Child values may read or borrow parent-owned values while valid.
- Parent values must not store or retain child-owned values.
- Sibling groups must not exchange owned group values.
- Moving a value between groups is not part of V1. Use `copy` to create a value in the destination group when needed.

### Aggregates

A group-owned aggregate may store values owned by the same group.

```beanstalk
group scratch:
    rows ~{Row} into scratch = {}
    row Row into scratch = parse_row(raw)
    ~rows.push(row)!
;
```

A longer-lived aggregate must not store a shorter-lived group-owned value.

```beanstalk
rows ~{Row} = {}

group scratch:
    row Row into scratch = parse_row(raw)
    ~rows.push(row)! -- invalid
;
```

A parent group aggregate must not store a child-group value unless the value is copied into the parent group or produced fresh for the parent group.

### Reactive values

Reactive storage is long-lived relative to the subscriptions and mounted runtime sinks that can observe it. V1 must reject grouped placement for reactive declarations and reject assigning group-owned values into reactive sources that outlive the group.

```beanstalk
group scratch:
    label String into scratch = [: temporary]
    title = label -- invalid if `title` is a reactive source outside `scratch`
;
```

This preserves GC-equivalent behaviour for mounted fragments and subscriptions.

### External packages

External functions need metadata that declares whether arguments are borrowed-only, consumed, retained or returned by alias.

Grouped values may be passed to external functions only when the metadata proves the external call does not retain the value past the call.

Unknown external retention is a diagnostic, not a fallback.

Backend fallback can choose GC or ordinary ownership representation for implementation, but the frontend must still reject source programs that would let a group-owned value escape.

## Backend behaviour

### Required semantic parity

Every backend must accept and reject the same grouped-memory source programs.

GC-only backends may lower group allocation and group release to no-ops, but they still consume borrow-validation results. The group feature is a source-level safety and optimisation contract, not a target-specific extension.

### JavaScript

The JavaScript path may initially ignore group allocation and release at runtime.

Required behaviour:

- parse and type-check grouped syntax;
- enforce borrow-validation escape rules;
- preserve normal alias, copy, mutation and reactive behaviour;
- lower group releases as no-ops.

### Wasm and future native targets

Ownership-aware backends may use group facts to select an allocation plan.

Possible plans:

- GC no-op fallback;
- stack-like frame;
- bump region;
- segmented region;
- slab or pool allocation;
- grouped drop list followed by bulk memory release;
- ordinary per-value ownership when grouping is not profitable.

The backend chooses the plan from HIR, borrow facts, type layout, allocation sites, destructor or cleanup requirements, target capability and profile.

Grouped values passed to functions should normally be passed as borrowed under the unified ownership ABI. The group owns release responsibility. A callee must not individually destroy a group-owned value unless later design adds an explicit safe transfer out of the group.

## Compiler design shape

### Stage 1 and 2: tokenization, header syntax and interface binding

Add syntax for `group` and `into` without changing import, module or declaration ordering semantics.

`group` and `into` are runtime body syntax. They do not affect top-level declaration ordering, provider graph construction, public interfaces or config syntax.

Header syntax preparation should reject group syntax in impossible root-level positions only where structurally obvious. Full semantic diagnostics belong to AST and borrow validation.

### Stage 4: AST semantics

AST owns source parsing, name resolution and typed receiving-boundary validation.

AST should:

- register group names in body-local scope state;
- reject group-name collisions through the ordinary visible-name policy;
- resolve each `into group` to a stable group identity;
- attach placement metadata to the binding receiving boundary;
- validate that grouped placement appears only in runtime executable bodies;
- validate obvious fresh-value placement rules where the AST owns the evidence;
- leave control-flow-sensitive escape analysis to borrow validation.

AST must not encode group identity into `TypeId`. Group membership is access and ownership metadata, not semantic type identity.

### Stage 5: HIR and validation

HIR should receive explicit memory-group metadata.

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
    pub group: Option<MemoryGroupId>,
    pub source_location: SourceLocation,
}
```

HIR should record:

- group declarations;
- parent/child group relationships;
- binding placement metadata;
- allocation sites that may use a group;
- group exit sites for all control-flow exits.

HIR validation should check structural invariants only:

- every placement references an existing group;
- every group belongs to a valid lexical region;
- parent chains are acyclic;
- exit-site metadata references valid blocks;
- group IDs are module-local and stable.

User-facing grouped-memory diagnostics belong earlier in AST or later in borrow validation.

### Stage 6: borrow validation

Borrow validation owns group safety.

It should extend its side-table facts with:

- group-owned local or value facts;
- allocation-site facts;
- group exit or release-site facts;
- escape diagnostics;
- external retention diagnostics;
- aggregate storage diagnostics;
- reactive retention diagnostics.

Checks:

1. Reject returning a group-owned value or alias.
2. Reject storing group-owned values into longer-lived owners.
3. Reject storing child-group values into parent or sibling groups.
4. Reject passing grouped values to retaining external calls.
5. Reject grouped placement of non-fresh call results.
6. Reject live aliases across group exit.
7. Reject group-owned values in reactive storage that outlives the group.
8. Preserve ordinary shared and exclusive access rules.

Borrow validation should remain conservative. If it cannot prove that a grouped value is safe, it should reject grouped placement rather than silently degrade the source rule. Backend GC fallback is for representation, not for accepting invalid group escapes.

### Stage 7: backend lowering

Backends consume validated HIR and borrow facts.

Backend lowerers should not rediscover source group syntax or infer group membership from names. They receive explicit memory-group facts and choose a target-specific allocation plan.

The build system and target-validation layers may later use grouped-memory facts for target contracts, layout identity, link fingerprints and profile-specific lowering decisions. That is not required for the first design checkpoint.

## Diagnostics

Grouped-memory diagnostics should use stable codes and structured payloads.

Suggested families:

- `memory_group_unknown`
- `memory_group_name_collision`
- `memory_group_invalid_position`
- `memory_group_non_fresh_value`
- `memory_group_alias_result`
- `memory_group_return_escape`
- `memory_group_store_escape`
- `memory_group_nested_escape`
- `memory_group_live_alias_at_exit`
- `memory_group_reactive_escape`
- `memory_group_external_retention`
- `memory_group_backend_unsupported`

Diagnostics should point to:

- the `group` declaration;
- the grouped binding;
- the escaping use;
- the longer-lived receiving place where relevant;
- the call metadata boundary for external retention.

Suggested help text should prefer `copy` or allocating directly into a longer-lived group.

## Deferred reserved bytes design

Reserved capacity is intentionally out of scope for this document.

The future feature should be designed as an extension of group declarations, not as type identity and not as collection fixed-capacity syntax.

Current design constraints for the future extension:

- do not reuse fixed collection capacity semantics;
- do not make reserved bytes part of `TypeId`;
- do not require every backend to honour the reservation exactly;
- do not make reservation correctness-critical;
- prefer explicit named syntax over positional numbers;
- allow runtime values only if the future syntax and lowering can keep validation simple.

This document reserves the conceptual extension point but does not choose syntax.

## Roadmap placement

Grouped memory should be planned after the current ownership, borrow-validation and data-layout work is stable enough to expose reliable side-table facts.

The first future implementation plan should be phased around:

1. syntax and AST metadata;
2. HIR group metadata and validation;
3. borrow-validation escape checks;
4. JS no-op lowering with full frontend enforcement;
5. Wasm release markers;
6. target-specific group allocation planning.

This roadmap design deliberately stops before implementation steps, exact Rust APIs, runtime layout and reserved-capacity syntax.

## Final design decision summary

- The feature is named grouped memory or memory groups, not arenas.
- A `group name:` block creates a local destruction boundary.
- `into group` is written on binding receiving boundaries, after any explicit type/access annotation and before `=`.
- Group membership is metadata, not type identity.
- Grouped values are ordinary values while the group is live.
- A group-owned value or alias must not outlive the group.
- Explicit `copy` creates independent value semantics that can cross group boundaries.
- Nested groups are allowed, but child values cannot be retained by parent or sibling groups.
- Reactive storage and retaining external calls cannot hold group-owned values past the group.
- GC-only backends may lower groups as no-ops while preserving the same accepted source programs.
- Ownership-aware backends may lower groups to arenas, pools, stack frames, segmented regions, drop lists or ordinary per-value ownership.
- Reserved bytes and preallocation syntax are deferred.
