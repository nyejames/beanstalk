---
inclusion: fileMatch
fileMatchPattern: ['src/compiler/WIR/**/*.rs', 'src/compiler/borrow_check/**/*.rs']
---

# Beanstalk WIR Implementation Guide

This guide defines the architecture and implementation patterns for Beanstalk's Mid-level Intermediate Representation (WIR). The WIR serves exactly two purposes: direct WASM lowering and precise borrow checking. It is NOT an optimization IR - folding and optimizations are handled at the AST stage or left to external WASM tools.

## Core Design Principles

**Dual Purpose**: WIR serves exactly two functions:
1. **Direct WASM Lowering**: Each WIR statement maps directly to ≤3 WASM instructions
2. **Precise Borrow Checking**: Polonius-style lifetime analysis with place-based tracking

**Key Design Constraints**:
- **Statement-level precision**: Each program point tracked for borrow checking
- **Field-sensitive places**: Struct fields and array indices tracked separately  
- **Three-address form**: Every operand read/write in separate statement for precise tracking
- **WASM-compatible types**: All operands use WASM value types (i32, i64, f32, f64)
- **No optimization passes**: Folding done at AST stage, complex optimizations left to external tools

## What WIR is NOT Used For

**WIR explicitly avoids optimization passes**:
- ❌ **Constant folding**: Handled at AST construction stage
- ❌ **Dead code elimination**: Left to external WASM optimization tools
- ❌ **Loop optimizations**: Not performed in WIR
- ❌ **Inlining**: Not a WIR responsibility
- ❌ **Complex transformations**: WIR maintains simple 1:1 mapping to WASM

**Why this design choice**:
- **Compilation speed**: Fewer passes means faster compilation
- **Simplicity**: Clear separation of concerns between stages
- **Tool ecosystem**: External WASM optimizers (like Binaryen) handle complex optimizations
- **Maintainability**: Simpler WIR is easier to debug and maintain

## Borrow Checking Rules

1. **Shared borrows**: Multiple `&` references allowed simultaneously
2. **Unique borrows**: At most one `&mut` reference, no overlap with shared
3. **Move semantics**: Owner cannot be moved while borrowed
4. **Use-after-move**: Illegal until reinitialized

## Module Structure

```
src/compiler/WIR/
├── mir_nodes.rs     // Core WIR types (Place, Stmt, Rvalue, Events, Loan)
├── build_mir.rs     // AST → WIR lowering with three-address form
├── cfg.rs           // Control Flow Graph construction
├── liveness.rs      // Backward liveness analysis for last-use refinement
├── extract.rs       // Extract borrow facts and build gen/kill sets
├── dataflow.rs      // Forward loan-liveness dataflow with bitsets
├── check.rs         // Borrow conflict detection and aliasing
└── diagnose.rs      // User-friendly error diagnostics
```

## Key Data Structures

### Place (Field-Sensitive Memory Locations)

```rust
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum Place {
    Var(VarId),                    // x
    Field(Box<Place>, FieldId),    // x.f
    Index(Box<Place>, ConstIndex), // arr[3] (constant index)
    Unknown(Box<Place>),           // arr[i] (dynamic index - conservative)
}
```

### WIR Statements (Three-Address Form)

```rust
#[derive(Clone, Debug)]
pub enum StmtKind {
    AssignTemp { dst: TempId, rv: Rvalue },     // t = rv
    AssignPlace { dst: Place, src: Operand },   // x = t
    Nop,
}

#[derive(Clone, Debug)]
pub enum Rvalue {
    Move(Place),          // consuming read (after refinement)
    Copy(Place),          // non-consuming read
    BorrowShared(Place),  // &place
    BorrowUnique(Place),  // &mut place
    Call(FuncId, Vec<Operand>),
    BinOp(BinOp, Operand, Operand),
    Const(ConstVal),
}
```

### Borrow Tracking

```rust
#[derive(Clone, Debug)]
pub struct Loan {
    pub id: LoanId,
    pub owner: Place,        // precise borrowed place
    pub kind: LoanKind,      // Shared or Unique
    pub origin_stmt: StmtId, // for diagnostics
}

#[derive(Default, Clone, Debug)]
pub struct Events {
    pub start_loans: Vec<LoanId>,         // loans starting at this stmt
    pub uses: Vec<Place>,                 // non-consuming reads
    pub moves: Vec<Place>,                // consuming moves
    pub reassigns: Vec<Place>,            // place reinitialized
    pub candidate_last_uses: Vec<Place>,  // from AST, refined by liveness
}
```

## Implementation Pipeline

### 1. AST → WIR Lowering (`build_mir.rs`)

**Key Pattern**: Break complex expressions into temporaries for precise borrow tracking and direct WASM lowering

```rust
// Input: x = foo(y + z*2)
// Output WIR (designed for both borrow checking and WASM lowering):
// t1 = z * 2        // Maps to: i32.const 2; local.get z; i32.mul
// t2 = y + t1       // Maps to: local.get y; local.get t1; i32.add  
// t3 = call foo(t2) // Maps to: local.get t2; call foo_idx
// x = t3            // Maps to: local.get t3; local.set x
```

**Lowering Rules for Dual Purpose**:
- **Borrow Checking**: Count AST uses per Place to hint last-uses
- **Borrow Checking**: Emit `Copy(place)` for reads, refine to `Move` later
- **Borrow Checking**: Create `Loan` for borrows, track in `events.start_loans`
- **Borrow Checking**: Mark `reassigns` for assignments
- **WASM Lowering**: Use WASM-compatible types (i32, i64, f32, f64) for all temporaries
- **WASM Lowering**: Structure statements for direct instruction mapping

### 2. CFG Construction (`cfg.rs`)

Build statement-level control flow graph:
- Link each statement to successors/predecessors
- Handle terminators (Goto, If, Switch, Return)

### 3. Liveness Analysis (`liveness.rs`)

**Purpose**: Refine AST last-use hints with CFG-aware backward dataflow

```rust
// Standard backward dataflow equations:
// LiveOut[s] = ⋃ LiveIn[succ(s)]
// LiveIn[s] = Uses[s] ∪ (LiveOut[s] - Defs[s])
```

**Refinement**: Convert `Copy(place)` → `Move(place)` at confirmed last uses

### 4. Extract Borrow Facts (`extract.rs`)

Build gen/kill bitsets for loan dataflow:
- `gen[s]`: loans starting at statement s
- `kill[s]`: loans ended by moves/reassigns that alias loan owner

### 5. Loan Liveness Dataflow (`dataflow.rs`)

**Forward dataflow** to compute live loans at each statement:

```rust
// LiveOutLoans[s] = ⋃ LiveInLoans[succ(s)]
// LiveInLoans[s] = Gen[s] ∪ (LiveOutLoans[s] - Kill[s])
```

Use efficient bitsets and worklist algorithm.

### 6. Conflict Detection (`check.rs`)

**Aliasing Rules** for `may_alias(a, b)`:
- Same place → alias
- `Var(x)` aliases `Field(Var(x), _)` and `Index(Var(x), _)`
- Distinct fields don't alias: `x.f1` vs `x.f2`
- Constant indices: `arr[0]` vs `arr[1]` don't alias
- Dynamic indices: `Unknown(_)` conservatively aliases everything

**Conflict Checks**:
1. **Unique/shared overlap**: Error if live loans have both Unique and Shared on aliasing places
2. **Move while borrowed**: Error if moving place that aliases live loan owner
3. **Use after move**: Track moved-out places, error on use before reinit

### 7. Diagnostics (`diagnose.rs`)

Generate helpful error messages with source spans:

```
error[E0001]: cannot move `x` because it is borrowed
  --> file.bs:42:5
42 |   move x
   |   ^^^^^^ move occurs here
note: borrow of `x.field1` starts here
  --> file.bs:37:9
37 |   a = &x.field1
   |        ^^^^^^^^
```

## Error Handling Patterns

Use appropriate error macros:
- `return_rule_error!(location, "message")` - borrow checking violations
- `return_compiler_error!("message")` - unimplemented WIR features
- Include precise source locations for user errors

## Testing Requirements

Essential test cases:
- Disjoint fields: `a = &x.f1; b = &x.f2` (should pass)
- Field conflicts: `a = &x; b = &x.f1` (should error)
- Last use precision: `a = &x.f; use(a); b = &mut x.f` (should pass)
- Move while borrowed: `a = &x.f; move x` (should error)
- Constant indices: `a = &arr[0]; b = &arr[1]` (should pass)
- Use after move: `move x; use x` (should error)

## Performance Guidelines

- Use `BitSet` for loan sets (width = number of loans in function)
- Cache `may_alias(a,b)` results with `HashMap<(Place,Place), bool>`
- Use worklist algorithm for dataflow convergence
- Consider sparse sets for small functions

## Integration Points

**Entry Point**: `pub fn wir_pipeline(fn_ast: &FnAst) -> Result<WIRBody, Vec<Diagnostic>>`

**Pipeline Order (Dual Purpose)**:
1. `lower_fn_to_WIR(ast)` - AST to WIR with WASM-compatible structure and borrow events
2. `build_cfg(&mut WIR)` - construct CFG for both borrow analysis and WASM control flow
3. `refine_last_uses(&mut WIR, &mut events)` - liveness analysis for precise move semantics
4. `build_gen_kill(&events, &loans)` - extract borrow facts for Polonius analysis
5. `compute_live_loans(&cfg, &gen, &kill)` - loan dataflow for borrow checking
6. `run_checks(&WIR, &events, &loans, &live_in)` - borrow conflict detection

**Output**: Validated WIR ready for direct WASM lowering with memory safety guarantees.