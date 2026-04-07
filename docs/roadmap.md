# Beanstalk Pre-Alpha Checklist

This is a working execution plan for getting the compiler to a credible first alpha.

## Release gates

These are the non-negotiable conditions for starting Alpha.

- All claimed Alpha features compile, type check, and run through the full supported pipeline.
- Unsupported syntax or incomplete features fail with structured compiler diagnostics, not panics.
- The integration suite covers the supported language surface, not just recent feature areas.
- The JS backend and HTML builder are stable enough for real small projects and docs-style sites.
- Compiler diagnostics are useful, accurate, consistently formatted, and visually moving toward the Nushell-style goal.
- Cross-platform output is stable enough that Windows and macOS do not produce avoidable golden drift.

---

## Phase 0 - Correct Type behaviour

This phase is about cleaning up loose ends in the type system

### PR - Extend function call syntax with named args and explicit argument access

Unify the function-call argument model before more syntax lands on top of it.

This PR should establish one canonical `CallArgument` shape that can represent:
- a positional argument
- a named argument using `as`
- an explicit mutable-access call argument using `~`
- combinations of the above, where the name belongs to the argument slot and the `~` belongs to the value expression being passed


This PR is the semantic foundation for call syntax going into Alpha.
It should not try to also solve method-call receiver syntax in the same change.
Method-call explicit mutability remains a follow-up PR.

**Scope**
- Function calls only
- Host function calls must follow the same argument parsing model where practical
- Result-handling suffixes (`!`, fallback, named handler) must continue to work on top of the new call-argument representation
- Method-call receiver syntax is explicitly out of scope for this PR

**Syntax goals**
- Positional immutable/shared argument:
  - `sum(values)`
- Positional explicit mutable argument:
  - `sum(~values)`
- Named immutable/shared argument:
  - `sum(items as values)`
- Named explicit mutable argument:
  - `sum(items as ~values)`
- The `~` marker belongs to the passed expression, not the parameter name
- Named arguments must target declared parameter names exactly
- Calls must no longer rely on any implicit “mutable can satisfy immutable” collection shortcut
- Mutability at the call site must be explicit and type-checked for all argument kinds, including collections

**Checklist**
- Introduce a dedicated `CallArgument` AST type instead of passing around bare `Expression` values for function-call arguments.
- Make `CallArgument` carry at minimum:
  - the parsed value expression
  - an optional targeted parameter name
  - the explicit call-site access mode (shared/default vs explicit mutable)
- Refactor function-call parsing in the new `ast/expressions/function_calls.rs` area so argument parsing is no longer modeled as only `Vec<Expression>`.
- Replace the current “expected argument types only” parsing path with a richer per-parameter expectation shape that can validate:
  - parameter type
  - parameter mutability/access requirement
  - parameter name for named calls
- Parse `as` inside call arguments as the argument-name syntax for function calls.
- Decide and enforce the chosen grammar for named arguments consistently:
  - either `name as expr`
  - or another final chosen `as` direction
  - whichever shape is chosen, lock it in consistently in parser, diagnostics, docs, and tests
- Parse explicit mutable call arguments with `~` only on valid place expressions where required by the called parameter.
- Reject invalid `~` argument forms cleanly:
  - non-place expressions
  - immutable places
  - values that cannot be passed with mutable access
- Reject missing `~` where the parameter requires mutable/exclusive access.
- Remove any remaining compatibility behavior that treats mutable collections as implicitly acceptable for immutable parameters.
- Make collection arguments obey the same explicit call-site access rules as every other type instead of having special permissive behavior.
- Preserve the distinction between:
  - declaration-time mutability of a place
  - call-site explicit mutable access
  - later move/borrow lowering decisions
- Do not overload these into one flag if it makes the code harder to reason about.
- Thread the new `CallArgument` shape through:
  - user function call parsing
  - host function call parsing
  - result-handled call parsing
  - AST node construction for `FunctionCall`, `HostFunctionCall`, and `ResultHandledFunctionCall`
- Update call validation so it checks, in a clear order:
  - argument count / defaults
  - named argument resolution
  - duplicate names
  - positional vs named ordering rules
  - missing required parameters
  - unexpected parameter names
  - argument type compatibility
  - explicit mutable/shared access correctness
- Decide and enforce one consistent rule for mixed positional and named arguments.
  Recommended default:
  - positional arguments first
  - then named arguments
  - no positional arguments after the first named argument
- Ensure defaults still work correctly when named arguments skip earlier parameters.
- Make duplicate-parameter assignment in one call a hard error whether it happens through:
  - duplicate named args
  - positional + named targeting the same parameter
- Keep result-handling syntax (`call(...)!`, `call(...) ! fallback`, named handler blocks) working unchanged on top of the new call parser.
- Keep diagnostics specific and structured. Add dedicated errors for:
  - unknown named parameter
  - duplicate argument target
  - positional argument after named argument
  - mutable parameter requires explicit `~`
  - `~` used on non-place expression
  - `~` used on immutable place
  - wrong type even when name/access mode are otherwise valid
- Add WHAT/WHY comments around the new `CallArgument` representation and validation flow so the code reads like final design code, not transition code.
- Remove any old helper paths or compatibility logic that become redundant after the new call-argument model lands.
- Do not keep transitional wrappers just to preserve the older parser shape.

**Suggested implementation order**
1. Introduce `CallArgument` and thread it through AST node shapes first.
2. Refactor `create_function_call_arguments` into a dedicated argument parser that returns structured arguments rather than expressions.
3. Add per-parameter expectation metadata so parsing/validation can see both type and access requirements.
4. Add named-argument parsing with `as`.
5. Add explicit mutable-argument parsing and validation.
6. Rewrite validation around the new structured arguments.
7. Update host-call and result-handled call paths.
8. Prune old compatibility logic and stale helper code.
9. Add tests only after the new canonical parsing path is stable enough to avoid churn.

**Testing checklist**
- Parser/unit coverage for:
  - positional immutable arguments
  - positional mutable arguments
  - named immutable arguments
  - named mutable arguments
  - mixed positional + named valid cases
  - invalid ordering of positional/named args
  - duplicate parameter targeting
  - unknown parameter names
  - missing required arguments with defaults present on other parameters
  - `~` on temporary / literal / non-place expression
  - `~` on immutable variable
  - missing `~` for mutable parameter
- Integration coverage for:
  - normal user function calls
  - host calls that still use the unified argument path
  - result-returning calls with `!`
  - result-returning calls with fallback values
  - named handler blocks after calls parsed through the new argument layer
  - collection arguments passed explicitly as shared vs mutable
- Update any now-invalid fixtures that relied on old implicit mutable call behavior.
- Re-run:
  - `cargo check`
  - `cargo test`
  - `cargo run tests`

**Done when**
- Function-call syntax has one canonical argument representation instead of several partial conventions.
- Named arguments and explicit mutable call-site access both work through the same parsing and validation path.
- Collections no longer get a hidden call-compatibility exception.
- Invalid call-site mutability is a normal structured type/rule error rather than accidental behavior.
- The codebase is in a better shape for the later method-call explicit mutability PR instead of baking in more special cases.
- The compiler can explain bad named-argument usage cleanly.


### PR - Update function-call docs, diagnostics, and migrated tests for explicit call-site mutability

Once the new function-call argument model lands, clean up the user-facing surface immediately so the docs, diagnostics, and tests all describe the same language.

This PR is specifically about removing stale expectations from documentation and tests, and tightening the presentation of the new rules.
It should not introduce new call syntax semantics beyond what the implementation PR already decided.

**Scope**
- Language docs
- Compiler docs
- Roadmap references if needed
- Integration fixtures and parser/unit fixtures
- Diagnostic wording improvements directly related to the new function-call rules

**Checklist**
- Audit the language overview and any function/method documentation for outdated examples that omit required call-site mutability.
- Update docs so they clearly state:
  - function-call mutability is explicit at the call site
  - passing a mutable/exclusive argument without `~` is an error
  - using `~` on an invalid value is an error
  - collections follow the same explicit call-site mutability rules and do not get a permissive exception
- Add a concise explanation of the difference between:
  - a mutable variable declaration
  - an explicitly mutable function-call argument
- Update all examples that now require `~` at call sites.
- Update or remove old tests that were only passing because of the previous implicit behavior.
- Add explicit failure fixtures proving that the old behavior is now rejected.
- Add failure fixtures with message-fragment assertions for the most important mistakes:
  - missing `~` for mutable parameter
  - `~` on immutable place
  - `~` on non-place expression
  - wrong named parameter
  - duplicate named parameter
- Tighten diagnostic wording where needed so the compiler names:
  - the called function
  - the parameter name when available
  - the expected access mode
  - the actual argument form
- Ensure diagnostics suggest the direct fix where practical, for example:
  - “Call this with `~value`”
  - “Remove `~` because this parameter expects shared access”
  - “Use a declared parameter name”
- Audit any roadmap/doc references that still describe the older permissive collection-call behavior.
- Keep test names and fixture names aligned with the new rules instead of preserving old assumptions in file names.
- Re-run:
  - `cargo check`
  - `cargo test`
  - `cargo run tests`

**Suggested docs to audit**
- `README.md`
- language overview docs
- compiler design notes where function-call semantics are mentioned
- roadmap notes around named args / mutability
- any examples in tests or docs site content that demonstrate function calls

**Done when**
- The documented language matches the implemented language for function calls.
- There are no stale examples implying that mutable call-site access can be inferred.
- Tests now protect the explicit-call-site rule instead of preserving the old accidental behavior.
- Diagnostics make the new rule feel intentional rather than surprising.



## Phase 1 - Code review checkpoint

This phase is a deliberate cleanup and consolidation checkpoint before pushing further on language surface.
The goal is to reduce structural risk now, remove stale paths while the compiler is still prealpha,
and make later feature work land into a tighter codebase.

### PR - Consolidate shared type parsing, type resolution, and frontend utility helpers

Use this pass to pull duplicated or overly general logic out of feature-specific frontend files and move it into common, reusable homes.

Recent function / function-call refactoring exposed a broader pattern: some type-display, type-resolution, and parsing helpers have drifted into individual statement/expression modules even though they are general frontend concerns. This PR should clean that up before more language surface lands and before more small one-off helpers get copied into new files.

This is not meant to change language semantics. The goal is to keep the frontend architecture tight, reduce repeated work, and make shared behavior come from one deliberate place.

**Scope**
- Frontend only
- Refactor duplicated or overly general helpers into shared/common modules
- Keep current semantics and diagnostics behavior stable unless a clear inconsistency is discovered during cleanup
- Prefer deleting duplicated logic rather than wrapping old APIs around new helpers

**Primary goals**
- Stop feature files from owning general datatype or type-annotation logic
- Stop repeated “resolve named type / optional type / collection type” logic from drifting separately
- Stop repeated token-scanning / top-level nesting logic from being hand-rolled in multiple files where a shared helper would be clearer
- Make shared display / formatting / diagnostic helpers live in the most appropriate common location

**Checklist**
- Audit the frontend for duplicated or overly general helpers, especially in:
  - `ast/statements/*`
  - `ast/expressions/*`
  - `headers/*`
  - `compiler_messages/*`
- Consolidate named-type resolution logic that is currently split across multiple files into one shared helper path.
- Remove duplicated “unknown type” lookup / resolution logic and make declarations, multi-bind, signature resolution, and similar code go through the same core resolution path.
- Consolidate optional type wrapping and named-type resolution into reusable helpers instead of rebuilding the same logic per feature.
- Refactor explicit type-annotation parsing so declarations/binding targets and function signatures are not maintaining parallel parsers for:
  - primitive types
  - named types
  - collection types
  - optional `?` suffixes
- Decide whether the shared type-annotation parser should live:
  - in `datatypes.rs`
  - in a dedicated shared type parser module
  - or in another clearly common frontend location
  and move it there
- Keep context-specific validation and diagnostics (declaration vs parameter vs return) separate if needed, but make the core token-to-type parsing shared.
- Re-evaluate any display / formatting / diagnostic helper that is currently living in a feature file or in a location that does not match its responsibility.
- In particular, make sure type-related mismatch/explanation helpers live in a common type or diagnostics home rather than being owned by one feature parser.
- Re-check whether recently moved helpers now belong in an even more appropriate shared location than their current one.
- Look for repeated top-level token scanning / depth tracking code (for example parenthesis / curly / template depth tracking) and extract a small shared helper if doing so improves clarity and removes duplication.
- Do not over-abstract tiny one-off logic, but do consolidate repeated scanning machinery where the same nesting bookkeeping is being reimplemented in multiple places.
- Remove any now-redundant local helpers from feature modules after the shared replacements land.
- Tighten imports and module boundaries after the move so files only depend on the shared helpers they actually need.
- Add WHAT/WHY comments to any newly introduced shared helper module explaining:
  - what responsibility it owns
  - why that responsibility is shared
  - what should not be added there
- Make sure the final structure follows the codebase style guide and does not just move duplication into a larger miscellaneous bucket.

**Suggested refactor targets**
- Named-type resolution helpers currently split across declaration, multi-bind, and AST type-resolution code
- Explicit type-annotation parsing currently split across declaration syntax and signature parsing
- Optional-type suffix handling duplicated across type parsing paths
- Type mismatch / type conversion hint helpers that are general diagnostic policy rather than feature-specific logic
- Repeated token-depth / top-level scanning helpers used by declarations, multi-bind parsing, header parsing, or similar frontend passes

**Suggested implementation order**
1. Identify and list the duplicated helpers before moving anything.
2. Extract shared named-type resolution helpers first.
3. Extract shared type-annotation parsing helpers second.
4. Re-home general type/diagnostic helpers into a clear common location.
5. Extract any worthwhile shared token-scanning utilities.
6. Delete old duplicated helpers and thread the new shared calls through the frontend.
7. Run a readability pass so the final shape is cleaner, not just more centralized.

**Testing checklist**
- Add or update unit tests around the shared helper modules directly where practical.
- Re-run affected parser and AST tests to ensure behavior did not drift during refactor.
- Add regression coverage for any bug or inconsistency found while consolidating duplicated logic.
- Ensure diagnostics remain stable or improve only intentionally.
- Re-run:
  - `cargo check`
  - `cargo test`
  - `cargo run tests`

**Done when**
- General datatype/type-resolution/type-parsing behavior no longer lives in arbitrary feature files.
- The frontend has one clear shared path for named-type resolution and one clear shared path for type-annotation parsing.
- Repeated utility logic has been removed rather than merely moved around.
- The frontend reads more intentionally and is less likely to drift as more language features are added.

### PR - Run a style-guide and readability sweep across the touched areas

Finish the checkpoint by making the newly-refactored code read like deliberate final code rather than churn aftermath.

**Checklist**
- Add or tighten file-level docs and WHAT/WHY comments where the refactors introduced new seams.
- Normalize naming and function boundaries to match `docs/codebase-style-guide.md`.
- Remove any remaining low-value comments that only narrate syntax or restate code.
- Re-check that touched files are not carrying avoidable inline imports, broad dead-code allowances, or mixed responsibilities.
- Run the normal verification loop:
  - `cargo check`
  - `cargo test`
  - `cargo run tests`

**Done when**
- The refactor checkpoint leaves the codebase clearer, not just differently arranged.
- The touched subsystems read consistently with the style guide.
- This phase ends with the compiler in a tighter shape for the next language-feature work.

## Phase 2 - close the core language feature gaps

### PR - Consolidate Char across the frontend and backend surface

Stop Char being a neglected primitive with uneven support.

**Checklist**
- Audit tokenizer, parser, AST typing, HIR typing, evaluation, lowering, and backend handling for Char.
- Fill any missing type-checking or lowering gaps.
- Add parser, type, runtime/backend, and integration coverage.

**Done when**
- Char behaves like a deliberate core datatype rather than a half-kept edge type.

### PR - Harden structs, records, and methods together

Close the loop on struct/record/method behavior as one language slice.

**Checklist**
- Audit runtime structs and const records against current docs/scope.
- Confirm methods resolve cleanly, especially receiver methods and same-file/export visibility.
- Add missing integration tests for declaration, construction, defaults, methods, field access, mutation, and diagnostics.
- Tighten any remaining semantic rough edges.

**Done when**
- Structs and records feel Alpha-ready as a practical feature, not a partially assembled one.

### PR - Harden basic if expressions and logical expressions

Make these small core expression features boring and reliable.

**Checklist**
- Audit expression parsing, type checking, constant folding, and lowering.
- Add focused integration cases for boolean combinations, nesting, precedence, and invalid type combinations.
- Improve error messages for non-boolean logic misuse.

**Done when**
- These features no longer feel like edge behavior.

---

## Phase 3 - expand integration coverage across the full Alpha surface

### PR - Create a language-surface integration matrix

Track what supported language features have canonical end-to-end coverage.

**Checklist**
- Add a simple feature-to-case mapping section or helper doc.
- Enumerate the Alpha surface:
  - control flow
  - functions/calls
  - templates/style directives
  - structs/records/methods
  - choices
  - pattern matching
  - arrays
  - results/options/multiple returns/multiple assignment
  - type checking
  - paths/imports
  - html project builds
  - logical expressions
  - if expressions
  - char
  - named arguments
- Mark gaps explicitly.

**Done when**
- Missing integration coverage is visible immediately.

### PR - Add integration coverage for the neglected language areas

Broaden the suite away from being overly concentrated on current recent work.

**Checklist**
- Add success and failure cases for basic control flow.
- Add success and failure cases for function declarations/calls.
- Add templates/style directive stability cases.
- Add structs/records/methods cases.
- Add arrays and array diagnostics.
- Add logical and if-expression cases.
- Add Char cases.

**Done when**
- The canonical integration suite represents the supported language rather than mostly paths/results/assets.

### PR - Add backend-facing integration checks for runtime-heavy features

Make sure JS/backend semantics are being checked where language behavior depends on runtime lowering.

**Checklist**
- Add cases for alias-sensitive behavior where relevant.
- Add cases for template runtime fragment insertion behavior.
- Add cases for result propagation/fallback through generated outputs.
- Add cases for arrays and casts where backend behavior matters.
- Expand artifact assertions where goldens alone are too brittle or too vague.

**Done when**
- Runtime semantics are not being trusted blindly.

---

## Phase 4 - diagnostics and compiler UX hardening

### PR - Standardize unsupported/incomplete-feature diagnostics

All incomplete or intentionally deferred features fail the same way: clearly and helpfully.

**Checklist**
- Audit current “not implemented”, “reserved”, and fallback diagnostics.
- Normalize wording, stage metadata, source locations, and suggestion style.
- Prefer one clean pattern for deferred-feature errors.

**Done when**
- Unsupported features feel deliberately handled.

### PR - Improve type-checking diagnostics across common user mistakes

Push compiler errors toward useful Nushell-style presentation and clarity.

**Checklist**
- Audit the most common type mismatch surfaces.
- Make messages name exact types and exact offending value/name where practical.
- Improve suggestions for common mistakes in calls, assignments, expressions, and struct construction.
- Add targeted failure fixtures proving the wording is specific enough.

**Done when**
- Type errors are accurate, grounded, and visibly better than generic compiler output.

### PR - Improve formatting/rendering of compiler errors

Move the displayed output closer to the desired final feel.

**Checklist**
- Refine rendered formatting for file path, span, label ordering, suggestions, and grouped messages.
- Make CLI `check` and normal build output feel consistent.
- Keep the data model stable while improving presentation.
- Add snapshot/golden-style tests for formatter output if practical.

**Done when**
- Errors look intentional and readable, not merely structurally correct.

### PR - Add variable-name ban list / reserved near-builtins

Prevent obviously stupid or misleading variable names that collide with builtin semantics.

**Checklist**
- Define a ban/reservation policy for misleading names such as `_true`, `FALSE`, and too-close builtins.
- Enforce it in parsing/name-resolution/type stages as appropriate.
- Produce good diagnostics explaining why the name is reserved.
- Add integration tests.

**Done when**
- Users cannot create confusing pseudo-builtin identifiers.

---

## Phase 5 - cross-platform consistency and test stability

### PR - Finish CRLF normalization in strings and templates

Remove avoidable Windows/macOS golden drift from source normalization and emitted outputs.

**Checklist**
- Audit remaining CRLF behavior in strings, templates, and emitted output.
- Make sure normalized newline handling is consistent through the frontend and builder outputs.
- Add regression tests specifically for Windows-shaped input.

**Done when**
- Golden outputs are stable across normal Windows/macOS workflows.

### PR - Fix remaining Windows test-runner stability issues

Remove test-runner and lock-poisoning rough edges that still make Windows less reliable.

**Checklist**
- Audit known lock poisoning paths and test-runner failure behavior.
- Ensure failed tests/builds do not leave the runner in a poisoned or misleading state.
- Add targeted tests where possible.

**Done when**
- Windows failures look like normal compiler/test failures, not infrastructure weirdness.

---

## Phase 6 - JS backend and HTML builder hardening pass

### PR - JS backend semantic audit for Alpha surface

Verify that the JS backend behavior matches the intended Alpha language rules for the supported feature set.

**Checklist**
- Audit runtime helpers involved in aliasing, copying, arrays, result propagation, casts, and builtin helpers.
- Add or expand integration tests where behavior depends on emitted JS runtime logic.
- Fix any semantics that are currently “working by accident”.
- PullDeprecated enum variant (src/compiler_frontend/ast/field_access.rs:176, 189, 387): A deprecated CollectionBuiltinMethod variant guarded by three unreachable!() calls.

**Done when**
- The JS backend is trustworthy enough for real Alpha examples.

### PR - HTML builder final stabilization pass

Treat the HTML project builder as a real Alpha product surface.

**Checklist**
- Re-audit route derivation, homepage rules, duplicate path diagnostics, tracked assets, cleanup, and output layout.
- Add any remaining config and artifact assertions needed for confidence.
- Ensure docs site and small static-site projects remain a valid proving ground.

**Done when**
- The HTML project builder can be presented as a stable Alpha capability.

---

## Final pre-alpha sweep

### PR - Alpha checklist audit

Verify that the Alpha gates are genuinely met.

**Checklist**
- Re-run the feature matrix and mark all supported areas as covered.
- Re-check that unsupported/deferred features fail cleanly.
- Re-check that docs and examples match actual support.
- Re-check diagnostics quality on a representative set of failures.
- Re-check cross-platform golden stability.

**Done when**
- There is a credible yes/no answer to “is Alpha ready?”

### PR - Alpha cleanup PR

Land final small consistency and hygiene fixes before the release branch/tag.

**Checklist**
- Remove obsolete rejection fixtures for features that are now supported.
- Tighten comments, TODOs, and dead-code justifications.
- Prune stale scaffolding where the current design has clearly replaced it.
- Update release-facing docs and contribution notes if needed.

**Done when**
- The repo feels intentional at the point Alpha begins.

---

## Deferred until after Alpha
These are intentionally not Alpha blockers unless they become necessary for one of the supported slices.

This is a collection of notes and findings for future roadmaps once the roadmap above is complete.

- builtin `Error` enrichment beyond what is already required for the current compiler/runtime surface
- full tagged unions
- full pattern-matching design
- full interfaces implementation
- richer numeric redesign work not required by Alpha

**Wasm**

Broader Wasm maturity beyond the current experimental path.

Notes and limitations from previous investigations:
- The WASM backend can't handle Choice/Union types yet (maps to Handle but produces i32/i64 mismatches). 
- rt_string_from_i64 Wasm helper: Explicitly noted in the 1ac2613 commit message as an "incremental bridge implementation". It produces valid output but is not a complete runtime implementation. This is scoped for a dedicated follow-up and does not cause panics.