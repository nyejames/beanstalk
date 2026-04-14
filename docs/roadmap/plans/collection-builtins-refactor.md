# Refactor collection builtins into explicit compiler-owned operations and remove compatibility-shaped dispatch

Collection builtins should lower through an explicit compiler-owned representation instead of leaning on method-call-shaped compatibility scaffolding. This removes fake dispatch surface, simplifies backend contracts, and makes collection semantics easier to audit for Alpha.

**Why this PR exists**

The language rules are already clear: collection operations are compiler-owned builtins, not ordinary user-defined receiver methods. The current implementation still carries method-call-shaped indirection, including synthetic builtin paths and compatibility behavior that blurs the semantic boundary. That is workable in pre-alpha, but it is exactly the kind of representation drift that makes backend audits noisy and future maintenance harder.

**Goals**

* Represent collection builtin operations explicitly as compiler-owned operations.
* Remove synthetic “pretend method” compatibility paths where they no longer carry semantic value.
* Keep call-site mutability rules strict and explicit.
* Make collection lowering easier to audit in JS and HTML/Wasm runtime-heavy tests.

**Non-goals**

* No change to user-facing collection syntax in this PR.
* No redesign of collection semantics or error-return behavior.
* No broad container-type redesign.

**Implementation guidance**

#### 1. Replace method-shaped collection builtin representation

Audit how collection builtins currently move through AST/HIR/backend lowering.

The target shape should make it obvious that these are not normal receiver methods. Choose one current representation and thread it through:

**Preferred direction**

* add a dedicated compiler-owned builtin operation representation for collection operations

Possible shapes:

* dedicated AST node variants such as:

  * `CollectionGet`
  * `CollectionSet`
  * `CollectionPush`
  * `CollectionRemove`
  * `CollectionLength`
* or a smaller shared builtin-op enum if that keeps lowering cleaner

Avoid keeping synthetic method paths just to preserve the old AST shape.

#### 2. Remove compatibility-only dispatch artifacts

Clean up compatibility-shaped pieces such as:

* synthetic builtin method path for `set`
* collection-op lowering that depends on pretending there is a normal method symbol behind the syntax
* any compatibility branch retained only because older AST/HIR/backend shapes expected methods everywhere

Keep only what is still semantically justified.

#### 3. Re-audit mutability and place validation at the builtin boundary

Use this PR to make collection builtin validation visibly consistent with the language guide:

* mutating collection operations require explicit mutable/exclusive access at the receiver site
* non-mutating operations reject unnecessary `~`
* mutating operations require a mutable place receiver
* indexed-write / `get(index) = value` behavior remains explicit and compiler-owned

The parser/frontend diagnostics for these cases should stay clear and specific.

#### 4. Simplify HIR/backend lowering contracts

Once AST stops pretending these are methods, lower them through a smaller explicit contract.

Target result:

* HIR and JS lowering do not need to infer “is this really a collection builtin disguised as a method call?”
* lowering logic can switch on a dedicated builtin-op kind
* collection get/set/remove/push/length semantics become easier to test directly

#### 5. Re-check JS runtime helper usage against frontend semantics

Audit the emitted JS/runtime behavior for:

* `get`
* `set`
* `push`
* `remove`
* `length`

Specifically check for “working by accident” behavior and for any mismatch between current frontend validation and runtime helper semantics.

#### 6. Strengthen backend-facing coverage

Expand tests so collection behavior is not only parser/frontend-covered but also backend-contract-covered.

Add or improve cases for:

* successful `get/set/push/remove/length`
* out-of-bounds `get`
* explicit mutable receiver requirement for mutating ops
* indexed write forms
* result propagation/fallback after `get`
* HTML-Wasm runtime-sensitive collection paths where emitted runtime behavior matters

**Primary files to audit**

* `src/compiler_frontend/ast/field_access/collection_builtin.rs`
* `src/compiler_frontend/ast/field_access/mod.rs`
* relevant AST/HIR lowering files for method/builtin calls
* JS runtime helper emission and expression/statement lowering
* integration fixtures covering collection operations

**Checklist**

* Introduce one explicit representation for collection builtins.
* Remove synthetic method-path compatibility scaffolding where it is no longer needed.
* Keep parser/frontend mutability/place validation aligned with the language rules.
* Thread the new builtin-op shape through HIR/backend lowering.
* Re-audit JS runtime semantics for all collection builtins.
* Add backend-facing and HTML-Wasm-sensitive regression coverage.
* Remove stale compatibility branches and comments once the new shape lands.

**Done when**

* Collection builtins no longer depend on fake method-dispatch representation.
* AST/HIR/backend code treats collection ops as compiler-owned operations explicitly.
* Mutability/place diagnostics remain clear and correct.
* JS/backend tests prove collection behavior directly rather than indirectly through compatibility shape.

**Implementation notes for the later execution plan**

* Keep the representation change central and mechanical: choose one shape and thread it through.
* Avoid adding a second abstraction layer just to preserve old code.
* Land this before or alongside the JS backend semantic audit so the audit sees the final builtin representation.