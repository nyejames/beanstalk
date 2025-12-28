# Beanstalk IR Architecture: HIR and LIR

Beanstalk uses two intermediate representations — **HIR** and **LIR** — each with a sharply defined role in the compilation pipeline.

The full pipeline is:

**AST → HIR Generation → Borrow Checking → LIR Generation → Wasm Codegen**

The key idea is that **HIR exists to make ownership, borrowing, and lifetimes analyzable**, while **LIR exists to make codegen trivial**.

---

## High-Level IR (HIR)

### Purpose

HIR (High-Level IR) is Beanstalk’s *semantic* IR.

It represents programs in a fully structured, ownership-aware form suitable for **borrow checking, last-use analysis, and drop planning**, while remaining independent of Wasm’s execution model.

HIR preserves *meaning*, not syntax.

---

### Core Design Principles

* **Structured control flow**
  Control flow remains explicit (`if`, `loop`, `match`) to enable CFG-based analysis.

* **Place-based memory model**
  All memory access is expressed in terms of *places* (locals, parameters, globals, and projections).
  There are no raw addresses and no pointer arithmetic.

* **No expressions**
  HIR contains **no nested expressions or temporaries**.
  All computation is linearized into statements operating on named places.

* **Borrow intent, not ownership outcome**
  HIR records *where mutable or shared access is requested*, but does **not** decide whether that access becomes a move or a borrow.

* **Move decisions are deferred**
  Ownership transfer is determined *after* borrow checking via last-use analysis.
  HIR never commits to a move up front.

* **Language-shaped, not Wasm-shaped**
  HIR reflects Beanstalk semantics, not stack machines or linear memory.
  Wasm concerns are deferred to LIR.

* **Diagnostics-first**
  All HIR nodes retain source spans and structural context for high-quality error reporting.

---

### Key HIR Concepts

#### Places

A **Place** represents a precise logical memory location.

Examples:

* `local`
* `local.field`
* `param[index]`
* `global.value`
* `*ref.field`

Places are:

* Rooted in locals, parameters, or globals
* Extended via projections (field, index, deref)
* Compared structurally for overlap

There are **no temporaries** at the language level; compiler-introduced locals are treated exactly like user locals.

**No Shadowing**: Beanstalk disallows variable shadowing, meaning each place name refers to exactly one memory location throughout its scope. This simplifies borrow checking significantly.

---

#### Statements (Canonicalized Semantics)

HIR consists of a small, explicit set of statements:

* Assignment (`Assign`)
* Borrow creation (`Borrow { Shared | Mutable }`)
* Function calls (`Call`)
* Control-flow terminators (`If`, `Loop`, `Return`, etc.)
* Drops (`Drop`) — inserted *after* borrow checking

There is no syntactic sugar and no implicit behavior.

---

#### Borrows

* All reads create *shared borrow intent*
* `~` creates *mutable borrow intent*
* Borrows are explicit HIR nodes
* Borrows do **not** encode moves

HIR expresses *what access is requested*, not *what ownership result occurs*.

---

#### Templates

* Templates fully resolved at AST stage become string literals before HIR.
* Templates requiring runtime evaluation are lowered into **explicit template functions**.
* Calls to runtime templates appear as normal HIR call nodes.

HIR never performs template parsing or folding.

---

#### Host Calls

* Builtins such as `io` are preserved as explicit call nodes.
* HIR assumes required host imports exist.
* No abstraction layer exists between HIR and host calls.

---

### HIR Responsibilities

HIR generation is responsible for:

1. **Desugaring**

   * Control flow
   * Error propagation (`?`, `!`)
   * Multi-return syntax
   * Assignment forms

2. **Linearization**

   * Eliminate nested expressions
   * Introduce locals for all intermediate values
   * Ensure one operation per statement for borrow checker precision

3. **Place construction**

   * Represent all reads and writes as place operations
   * Enable precise overlap analysis

4. **Borrow intent emission**

   * Insert explicit shared/mutable borrow nodes
   * Record access intent, not ownership outcome

5. **CFG readiness**

   * Produce structured control flow suitable for CFG construction
   * Enable statement-level analysis granularity

6. **Analysis preparation**

   * Create IR optimized for borrow checking and last-use analysis
   * Maintain source location information for error reporting

---

### HIR and Borrow Checker Integration

The borrow checker requires additional linearization of HIR for precise analysis:

**Why Linearization is Required:**
* HIR nodes can contain multiple place usages (e.g., `x = y + z`)
* Statement-level granularity needed for accurate last-use detection
* CFG analysis requires one operation per node for correctness

**Statement-Level CFG Construction:**
* Extract linear statements from structured HIR nodes
* Each statement becomes exactly one CFG node (1:1 correspondence)
* Statement ID = CFG node ID (eliminates mapping complexity)
* Direct successor/predecessor relationships between statements
* Preserve source location mapping for error reporting

**Architectural Benefits:**
* Eliminates complex mapping between statements and HIR nodes
* Enables topologically correct analysis of complex control flow
* Supports efficient per-place liveness propagation
* Accurate Drop insertion at precise statement boundaries
* Maintains HIR's diagnostic capabilities while enabling precise analysis

**Topological Correctness:**
* Proper handling of early returns inside blocks
* Correct CFG edges for loops with break/continue statements
* Accurate modeling of fallthrough after if statements
* Essential for correct last-use analysis in complex control flow

---

## Borrow Checking and Move Determination

Borrow checking operates **exclusively on HIR**.

At this stage, HIR represents a *sound but conservative* view of access intent.

---

### Borrow Checking Stages

1. **HIR Linearization**

   * Flatten nested HIR structures into linear statements
   * Each statement represents one operation (use/define places)
   * Statement-level granularity for precise analysis

2. **CFG Construction**

   * Build control-flow graph from linearized statements
   * Each CFG node represents exactly one statement
   * Preserve structured control flow for analysis

3. **Borrow Tracking**

   * Track active borrows per statement
   * Record creation points and propagate through CFG
   * Use dataflow analysis for borrow state management

4. **Last-Use Analysis (Classic Dataflow)**

   * Build statement-level CFG where statement ID = CFG node ID
   * Direct CFG edges between statements (not HIR nodes)
   * Per-place liveness: HashMap<StmtId, HashSet<Place>>
   * Backward dataflow with worklist algorithm until fixed point
   * Last-use detection: place ∉ live_after at usage points
   * Topological correctness for complex control flow

5. **Conflict Detection**

   * Shared vs mutable conflicts using place overlap analysis
   * Path-sensitive (Polonius-style): conflicts illegal only on ALL incoming paths
   * Conservative merging at CFG join points

6. **Return Validation**

   * Ensure reference returns originate only from parameters
   * Enforce declared return origins (`-> param_name`)

---

### Move Determination and Drop Planning

Using precise last-use analysis results:

**Move Refinement:**
* Candidate moves examined against last-use information
* If candidate move is the last use → becomes actual move
* Otherwise → remains mutable borrow
* Decision entirely compiler-internal, never visible to user

**Drop Insertion:**
* Exact drop points computed from last-use analysis
* `Drop` nodes inserted at precise statement locations
* Statement-level granularity ensures accurate placement
* Ownership outcomes now fully determined

**HIR Annotation Complete:**
At this point, HIR is fully annotated with:
* Explicit borrows with inferred lifetimes
* Refined moves (from candidate moves)
* Precise Drop node placement
* Verified memory safety

This annotated HIR is then lowered into LIR with all ownership decisions resolved.

---

## Low-Level IR (LIR)

### Purpose

LIR (Low-Level IR) is a **Wasm-shaped** IR.

It represents the program in terms of explicit locals, memory operations, control blocks, and calls that map directly to WebAssembly.

LIR contains **no ownership logic** — all ownership decisions have already been made.

---

### Design Priorities

* Deterministic, mechanical lowering to Wasm
* Explicit memory addressing and layout
* Explicit stack discipline
* No semantic ambiguity
* No hidden control flow

---

### Key LIR Concepts

* **Instructions**

  * `LocalGet`, `LocalSet`
  * `Load`, `Store`
  * `Call`
  * `Return`
  * `Block`, `Loop`, `Branch`
  * `Drop`, `Free`

* **Explicit memory layout**

  * Field offsets computed
  * Struct and array layouts finalized

* **Stack normalization**

  * All branches balance the operand stack
  * Locals used to materialize values when needed

* **Template runtime calls**

  * Lowered into plain function calls

* **Host functions**

  * Preserved as explicit imports
  * No abstraction or indirection

---

### LIR Responsibilities

1. Lower HIR statements into explicit instruction sequences
2. Emit drop and free operations at exact points
3. Compute memory offsets and layouts
4. Normalize control flow for Wasm’s structured CFG
5. Preserve host calls and imports
6. Produce a stack-consistent, Wasm-ready IR

---

## Division of Responsibility

**AST → HIR**
Preserve semantic meaning, eliminate syntax, linearize computation, express borrow intent.

**HIR → Borrow Checker**
* Linearize HIR into statements for precise analysis
* Build CFG from linearized statements  
* Perform classic dataflow analysis for last-use detection
* Prove memory safety using Polonius-style conflict detection
* Infer lifetimes and determine ownership outcomes
* Insert Drop nodes at precise statement locations

**HIR → LIR**
Lower fully-annotated HIR (with resolved ownership) into explicit Wasm-shaped operations.

**LIR → Wasm**
Perform direct, mechanical bytecode emission with no remaining semantic decisions.