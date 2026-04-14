
# PR - Split the JS runtime prelude by concern and harden backend helper contracts

The JS backend runtime prelude currently centralizes too many unrelated helper groups in one file. Split it into focused modules, keep one small orchestration layer, and add stronger tests around the helper contracts that define Alpha runtime semantics.

**Why this PR exists**

The JS backend is the near-term stable backend and one of the main Alpha product surfaces. The runtime prelude is readable and well commented, but it is still too broad in one file: bindings, aliasing, computed places, cloning, errors, results, collections, strings, and casts all live together. That makes semantic auditing, targeted refactors, and regression testing harder than they need to be.

**Goals**

* Split the JS runtime helper emission into small focused modules.
* Preserve the current runtime semantics exactly unless a bug is being intentionally fixed.
* Make helper-group ownership obvious.
* Strengthen targeted tests for each helper surface.

**Non-goals**

* No wholesale JS backend redesign.
* No formatting/style churn unrelated to helper extraction.
* No user-facing language changes.

**Implementation guidance**

#### 1. Split `prelude.rs` into focused runtime helper modules

Refactor the current prelude into a small orchestration module plus focused helper emitters.

**Suggested structure**

* `src/backends/js/runtime/mod.rs`
* `src/backends/js/runtime/bindings.rs`
* `src/backends/js/runtime/aliasing.rs`
* `src/backends/js/runtime/places.rs`
* `src/backends/js/runtime/cloning.rs`
* `src/backends/js/runtime/errors.rs`
* `src/backends/js/runtime/results.rs`
* `src/backends/js/runtime/collections.rs`
* `src/backends/js/runtime/strings.rs`
* `src/backends/js/runtime/casts.rs`

The top-level emitter should only own:

* helper emission order
* high-level comments about why these groups exist
* any tiny shared glue that genuinely belongs at orchestration level

#### 2. Keep helper boundaries semantically intentional

Use the split to make helper responsibilities clearer:

* binding helpers: reference record construction, parameter normalization, read/write resolution
* alias helpers: borrow/value assignment semantics
* computed-place helpers: field/index place access
* clone helpers: explicit `copy` semantics
* error helpers: canonical runtime `Error` construction and context helpers
* result helpers: propagation and fallback behavior
* collection helpers: runtime contracts for ordered collections
* string helpers: string coercion and IO
* cast helpers: numeric/string cast behavior and result-carrier error paths

Avoid “misc” modules. Keep each file narrow.

#### 3. Re-check helper APIs for accidental overlap or leakage

During extraction, audit whether helper groups expose duplicated or cross-cutting behavior that should be simplified.

Examples to watch for:

* collection helpers depending on unrelated error-helper details without a clean boundary
* result helpers assuming too much about caller lowering shape
* alias/binding helpers carrying responsibilities that belong in computed-place helpers

Do not redesign aggressively; just remove obvious leakage.

#### 4. Strengthen JS backend tests around runtime contracts

Add targeted tests for helper-backed semantics, not just broad output snapshots.

Focus on:

* aliasing and assignment semantics
* explicit copy behavior
* result propagation/fallback helpers
* builtin error helper lowering
* collection runtime helpers
* cast success/failure behavior
* mutable receiver / place validation paths where JS runtime behavior depends on correct lowering

Prefer targeted artifact assertions or rendered-output assertions where full JS snapshots are noisy.

#### 5. Keep comments strong while reducing file breadth

The current prelude comments are useful. Preserve that quality after the split:

* each runtime helper file gets a short module doc comment
* each emitter function explains WHAT/WHY at the group level
* avoid repeating a giant duplicated overview in every file

**Primary files to touch**

* `src/backends/js/prelude.rs`
* `src/backends/js/mod.rs`
* JS backend tests and integration fixtures with runtime-heavy behavior

**Checklist**

* Split the JS runtime prelude into focused helper-group modules.
* Keep one small orchestration layer responsible for emission order.
* Preserve current helper semantics unless fixing an identified bug.
* Audit for duplicated or leaked helper responsibilities during extraction.
* Add or expand targeted tests for helper-backed runtime semantics.
* Prefer targeted assertions over brittle full-file snapshots where code shape is not the contract.

**Done when**

* No single JS runtime helper file owns most of the backend runtime surface.
* Helper-group ownership is obvious from file layout.
* Existing JS semantics remain stable.
* Runtime-heavy test coverage is stronger and lower-noise than before.

**Implementation notes for the later execution plan**

* Keep the first pass mostly structural.
* Only fix helper semantics in the same PR when the bug is obvious and covered.
* This PR should make the later “JS backend semantic audit for Alpha surface” materially easier.