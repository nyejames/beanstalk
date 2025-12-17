# Beanstalk Memory Management and Borrow Checking Strategy

The goal of Beanstalk’s memory and borrow system is to **ensure safety and avoid common bugs** without requiring a garbage collector or significantly impacting performance. The design leverages **reference semantics, precise borrow tracking, and compiler-inferred lifetimes** to achieve this.

## High-Level Goals

* No garbage collector. Memory is managed statically.
* Borrow checking ensures safety at compile time.
* The borrow checker aims to be **as precise as Polonius**, or at least stronger than Rust’s NLL.
* No explicit lifetime annotations; all lifetimes are inferred.
* No unsafe edge cases are considered, simplifying the checker.
* Reference semantics are default; explicit moves are unnecessary.
* Temporaries do not exist in the language semantics. Intermediate values are treated as locals.

---

## Lifetimes and References

* Functions can return references **only to parameters**.
* Return references using the parameter name in the function signature.

  ```beanstalk
  get_first |arr ~Array| -> arr:   -- returns a reference with the same lifetime as `arr`
  ```
* All other lifetimes are **elided** by the compiler.
* Only three kinds of value access exist:

  * **Immutable reference** (`&`-like, read-only)
  * **Mutable reference** (`~`-like, exclusive access)
  * **Value copy** (for copyable types)

---

## Place Model

A **Place** represents a logical storage location in memory.

```rust
struct Place {
    root: PlaceRoot,
    projections: Vec<Projection>,
}
```

### No Shadowing Simplification

**Critical Language Design**: Beanstalk disallows variable shadowing.

**Borrow Checker Benefits:**
- Each place name refers to exactly one memory location throughout its scope
- No need to track variable redefinitions or scope-based disambiguation
- Last-use analysis can safely ignore 'defines' since they don't change place identity
- Simplified place identity management without SSA-style renaming

### PlaceRoot

```rust
enum PlaceRoot {
    Local(LocalId),
    Param(ParamId),
    Global(GlobalId),
}
```

**Notes:**

* `Temporary` does **not exist** in language semantics but the compiler IR may still use hidden locals for intermediate computations.
* No heap object roots are exposed in the language.
* References always point to a `root + projections`.

---

### Projection

```rust
enum Projection {
    Field(FieldIndex),        // struct fields
    Index(IndexKind),         // arrays / slices
    Deref,                    // reference dereference
}

enum IndexKind {
    Constant(u32),            // arr[3]
    Dynamic,                  // arr[i]
}
```

**Restrictions:**

* `Deref` may only appear if the type is `&T` or `~T`.
* No chained pointer arithmetic.
* No reference-forging (`*&*x`) allowed.

---

### Overlap Rules

Overlap is a **structural check**, much simpler than Rust:

* Two places overlap if:

  1. They share the same root.
  2. One projection list is a **prefix** of the other.

**Examples:**

```text
x        overlaps x.a
x.a      overlaps x.a.b
x.a      does NOT overlap x.b
```

**Array indexing:**

| Case             | Overlap?           |
| ---------------- | ------------------ |
| arr[1] vs arr[1] | Yes                |
| arr[1] vs arr[2] | No                 |
| arr[i] vs arr[j] | Yes (conservative) |
| arr vs arr[i]    | Yes                |

---

## Borrows

```rust
struct Borrow {
    id: BorrowId,
    place: Place,
    kind: BorrowKind, // Shared | Mutable
}
```

For each borrow, the compiler tracks:

* Creation point
* Last use
* Control-flow paths where the borrow is active

###  Borrow Checking

* **Mutable borrows** (`~`) are exclusive.
* **Shared borrows** are allowed concurrently if no mutable borrow exists.
* Borrow **lifetimes** are inferred by CFG analysis and **end at last use**.
* Borrow **conflicts** are detected per path. A mutation or borrow is only illegal if a conflicting borrow exists **on all paths** reaching the use (Polonius-style reasoning).

---

## Additional Language Restrictions

1. **No interior mutability**

   * `&T` is always read-only; `~T` is always exclusive.
2. **No raw pointers or unsafe code**

   * Eliminates aliasing outside the type system.
3. **No closures or captured references**

   * Simplifies lifetime inference.
4. **No explicit moves**

   * Ownership is inferred via reference semantics and last-use analysis.
5. **No temporaries**

   * All values are named locals, parameters, or globals.
6. **No borrowing the whole object when a part is borrowed**

```beanstalk
let r = &x.field;
let s = &x;   -- error
```

7. **Function return references restricted**

   * Only references to parameters can be returned.
   * No references to locals, globals, or temporary values.

8. **Struct fields cannot store references** (simplifies lifetime reasoning)

---

## HIR and Last-Use Analysis Integration

The borrow checker operates exclusively on HIR, which provides the right level of abstraction:

### HIR Characteristics for Borrow Checking
* **No nested expressions**: All computation linearized into statements operating on places
* **Structured control flow**: If/match/loop preserved for CFG-based analysis  
* **Place-based memory model**: All memory access expressed through precise place representations
* **Borrow intent recording**: HIR records where access is requested, not ownership outcome

### Linearization for Precision
Even though HIR is structured, the borrow checker linearizes it for analysis:

* **Statement extraction**: Each HIR node becomes one or more linear statements
* **Single operation per statement**: Eliminates ambiguity about usage order within nodes
* **Precise CFG mapping**: Each CFG node represents exactly one statement
* **No temporaries needed**: Compiler IR uses hidden locals treated as `Local` roots

### Last-Use Analysis Requirements
* **Statement-level CFG**: Each CFG node corresponds to exactly one statement (1:1 mapping)
* **Statement ID = CFG node ID**: Eliminates complex mapping between statements and HIR nodes
* **Direct CFG connections**: Successors/predecessors between statements, not HIR nodes
* **Per-place propagation**: Efficient liveness computation without all_places initialization
* **Topological correctness**: Proper handling of complex control flow (early returns, break/continue)
* **Classic dataflow**: Backward analysis without visited sets or iteration caps

**Benefits:**

* Borrow checker remains simple, precise, and fast
* CFG-based analysis sufficient for Polonius-style reasoning
* Statement-level precision enables accurate Drop insertion
* No complex lifetime annotations required

---

## Borrow Checker Implementation Strategy

The borrow checker operates on HIR using a multi-phase approach:

### Phase 1: HIR Linearization
* **Flatten nested structures** into linear statements where each statement represents one operation
* **Statement-level granularity**: Each CFG node corresponds to one HIR statement, not complex nested nodes
* **Place extraction**: Identify all places used (read) and defined (written) by each statement

### Phase 2: Control Flow Graph Construction
* **Build CFG** from the linearized HIR statements
* **Structured control flow**: Preserve if/match/loop structure for analysis
* **Edge creation**: Connect statements based on execution flow, not traversal order

### Phase 3: Borrow Tracking
* **Track borrows per statement**: Store `{borrow_id → Place, kind}` for each active borrow
* **Record creation points**: Where each borrow is first created
* **Propagate through CFG**: Use dataflow analysis to track borrow state

### Phase 4: Last-Use Analysis (Classic Dataflow)
* **Statement-level CFG**: Build CFG where statement ID = CFG node ID (eliminate mapping)
* **Direct CFG edges**: Successors/predecessors defined between statements, not HIR nodes
* **Per-place liveness**: Compute live_after as HashMap<StmtId, HashSet<Place>>
* **Backward dataflow**: Worklist algorithm until fixed point without visited sets
* **Last-use detection**: Usage points where place ∉ live_after are last uses
* **Topological correctness**: Handle early returns, break/continue, fallthrough properly

### Phase 5: Conflict Detection
* **At CFG joins**: Merge borrow sets conservatively - conflicts only illegal on ALL incoming paths
* **Overlap checking**: Compare Place roots and projections structurally
* **Polonius-style reasoning**: Only reject access if conflicting borrow exists on all paths

### Phase 6: Move Refinement
* **Candidate move analysis**: Convert candidate moves to actual moves when they are last uses
* **Lifetime inference**: Borrows end immediately after their last use
* **Drop insertion**: Insert Drop nodes at precise last-use points

---

## Summary

* **Simpler Place model**: no temporaries, no unsafe, no interior mutability.
* **Path-sensitive borrow checking**: Polonius-style reasoning achievable without Datalog.
* **Memory safety guaranteed**: all aliasing and borrowing rules enforced at compile time.
* **IR simplicity**: hidden locals handle intermediate values; the language itself remains clean and predictable.

## Notes
- Automatic implicit boxing / unboxing may also be implemented to allow recursive structures