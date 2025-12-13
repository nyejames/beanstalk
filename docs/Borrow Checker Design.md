# Beanstalk Memory Management and Borrow Checking Strategy

The goal of Beanstalk’s memory and borrow system is to **ensure safety and avoid common bugs** without requiring a garbage collector or significantly impacting performance. The design leverages **reference semantics, precise borrow tracking, and compiler-inferred lifetimes** to achieve this.

## 1. High-Level Goals

* No garbage collector. Memory is managed statically.
* Borrow checking ensures safety at compile time.
* The borrow checker aims to be **as precise as Polonius**, or at least stronger than Rust’s NLL.
* No explicit lifetime annotations; all lifetimes are inferred.
* No unsafe edge cases are considered, simplifying the checker.
* Reference semantics are default; explicit moves are unnecessary.
* Temporaries do not exist in the language semantics. Intermediate values are treated as locals.

---

## 2. Lifetimes and References

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

## 3. Place Model

A **Place** represents a logical storage location in memory.

```rust
struct Place {
    root: PlaceRoot,
    projections: Vec<Projection>,
}
```

### 3.1 PlaceRoot

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

### 3.2 Projection

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

### 3.3 Overlap Rules

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

## 4. Borrows

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

### 4.1 Borrow Checking

* **Mutable borrows** (`~`) are exclusive.
* **Shared borrows** are allowed concurrently if no mutable borrow exists.
* Borrow **lifetimes** are inferred by CFG analysis and **end at last use**.
* Borrow **conflicts** are detected per path. A mutation or borrow is only illegal if a conflicting borrow exists **on all paths** reaching the use (Polonius-style reasoning).

---

## 5. Additional Language Restrictions

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

8. **Optional additional restrictions for speed and clarity:**

   * Dynamic array indices cannot be borrowed mutably (or must be proven constant).
   * No reborrowing through references (`let s = &*r;` forbidden).
   * Struct fields cannot store references (simplifies lifetime reasoning).

---

## 6. Compiler IR and Temporaries

Even though temporaries do not exist at the language level:

* **Compiler IR may still use hidden locals** to store intermediate values for operations such as `x + y` or function calls.
* These are treated **identically to `Local` roots** in the borrow checker.
* No separate `Temporary` root is necessary; this keeps the **Place model simple**.

**Benefit:**

* Borrow checker remains simple, precise, and fast.
* CFG-based analysis of borrows is sufficient to enforce Polonius-style reasoning.

---

## 7. Borrow Checker Implementation Strategy

1. **Build CFG** from the lowered IR.
2. **Track borrows per basic block**:

   * Store `{borrow_id → Place, kind}` for each active borrow.
   * Record creation, last use, and active paths.
3. **At CFG joins**:

   * Merge borrow sets conservatively for shared borrows.
   * Only reject mutable access if a conflicting borrow is active on **all incoming paths**.
4. **Last-use analysis**:

   * Borrow ends immediately after its last use.
5. **Overlap checking**:

   * Compare `Place` roots and projections structurally.
6. **Violation detection**:

   * A conflict exists only if a mutable/immutable rule is violated on all paths reaching the use.

---

## 8. Summary

By leveraging these design choices:

* **Simpler Place model**: no temporaries, no unsafe, no interior mutability.
* **Path-sensitive borrow checking**: Polonius-style reasoning achievable without Datalog.
* **Memory safety guaranteed**: all aliasing and borrowing rules enforced at compile time.
* **IR simplicity**: hidden locals handle intermediate values; the language itself remains clean and predictable.

> The combination of language restrictions and CFG-based borrow tracking allows Beanstalk to have a **fast, precise, and simpler borrow checker** than Rust, while still enabling reference-return semantics and non-trivial memory safety guarantees.
