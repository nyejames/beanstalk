# Beanstalk language documentation migration and semantic realignment

## 1. Purpose

This project completes the migration from `docs/language-overview.md` into focused language references under `docs/src/docs/**`.

The migration has three outputs:

1. **Basic references** teach the language in a progressive order without front-loading edge cases, implementation detail or deferred syntax.
2. **Advanced references** define the complete source-facing language contract, including final syntax, exact semantics, rejected forms, edge cases, accepted deferred behaviour and outside-scope decisions.
3. **Public page entries** compose both levels into clear website routes with stable headings, examples, navigation and presentation.

This plan now also owns a bounded compiler realignment. The review found current source and compiler surfaces that conflict with the accepted simplified language design. Those surfaces must be removed or corrected together with their tests and documentation.

Beanstalk is early Alpha. Removed design does not receive compatibility syntax, deprecation periods, legacy diagnostics, dormant adapters or retained internal data shapes.

This plan does not replace the compiler, build-system or memory architecture references. Advanced language pages describe source syntax and observable behaviour. They link to those references when stage ownership, graph orchestration, lifetime analysis or backend lowering is directly relevant.

---

## 2. Current state

### 2.1 Migration state

Focused Basic and Advanced pairs already exist for most language routes:

- Language Basics
- Values and Bindings
- Numbers
- Casts
- Functions
- Branching
- Loops
- Structs
- Choices
- Errors, Options and Assertions
- Collections and Maps
- Templates
- Constants and Compile-Time Behaviour
- Aliases
- Generics
- Traits
- Reactivity
- Project Structure
- Packages and Imports
- Beandown
- Plain Markdown

These files are replacement candidates. Their existence does not prove final parity.

The remaining project is not a route-copying exercise. It is a whole-language reconciliation against:

- the maintained language monolith
- the final memory-management references
- the compiler and build-system architecture
- current compiler behaviour and tests
- the progress matrix
- accepted user decisions in this plan

### 2.2 Authority has become composite

The memory-management migration is complete. `docs/src/docs/codebase/memory-management/**` now owns accepted program memory semantics.

`docs/language-overview.md` remains the maintained source-facing parity baseline during this migration, but it is no longer the sole owner of every formal rule repeated inside it.

The current migration plan is replaced by this document. Historical batch notes and probe diaries remain available in Git history. They are not active implementation instructions.

### 2.3 Final authority has not switched

The focused language references remain under review until:

- every normative source-facing rule has one focused owner
- the compiler realignment in this plan is complete
- every route passes Basic and Advanced review
- the whole-language parity audit passes
- the user explicitly approves the authority switch

`AGENTS.md` must not be changed before that separate authority-switch patch.

---

## 3. Authority model

### 3.1 Evidence order during migration

Use this order when sources disagree:

1. Explicit user decisions recorded in this plan
2. The maintained source-facing contract in `docs/language-overview.md`
3. Canonical memory-management references for access, aliases, copy, lifetime topology, ownership and backend-neutral memory semantics
4. `docs/compiler-design-overview.md` for compiler stages, semantic owners, IR contracts and analysis boundaries
5. `docs/build-system-design.md` for project config, module and package graphs, builders, linking and output ownership
6. The progress matrix for current implementation and backend coverage
7. Accepted roadmap plans for explicitly deferred implementation
8. Compiler implementation and tests as evidence of current behaviour
9. Existing public pages as non-authoritative teaching material

Implementation is not automatically language design. A compiler behaviour that conflicts with an accepted decision must be fixed or explicitly classified. It must not be silently canonised.

### 3.2 Responsibilities during migration

| Source | Responsibility |
|---|---|
| `docs/language-overview.md` | Maintained source-facing parity baseline |
| Unsuffixed `<concept>.bd` | Complete Advanced replacement under review |
| `<concept>-basic.bd` | Beginner teaching consistent with the Advanced contract |
| `#page.bst` | Public composition, tone, ordering, navigation and presentation |
| `docs/src/docs/codebase/language/overview.bd` | Index of focused language owners |
| Memory-management references | Formal access, aliasing, lifetime, ownership and lowering semantics |
| Compiler-design references | Stage ownership, semantic identities, HIR and analysis contracts |
| Build-system design | Project, package, builder, link and output architecture |
| Progress matrix | Current support, partial support, target gates and coverage |
| Roadmap | Accepted deferred implementation and future sequencing |

### 3.3 Patch classes

There are three valid patch classes.

#### Semantic realignment patch

A semantic realignment patch may change:

- compiler source
- compiler tests and integration fixtures
- focused Basic and Advanced language references
- `docs/language-overview.md`
- the progress matrix
- directly affected compiler, build-system or memory wording when ownership facts changed

The compiler, focused references and monolith must agree at the end of the accepted slice.

#### Documentation parity patch

A documentation parity patch may change:

- focused language references
- Basic teaching files
- public page entries
- the focused-reference index
- the monolith when a verified accepted fact needs synchronisation
- generated documentation

It must not change compiler behaviour.

#### Final authority-switch patch

The final authority switch is separate and requires explicit user approval. It may update:

- `AGENTS.md`
- authority wording in indexes
- the status or disposition of `docs/language-overview.md`

No earlier patch may imply that the switch has happened.

### 3.4 Final authority model

After approval:

| Source | Final responsibility |
|---|---|
| Unsuffixed focused language references | Exact source syntax and observable language semantics |
| Basic focused references | Beginner teaching |
| Public `#page.bst` entries | Website composition and editorial context |
| Memory-management references | Formal memory semantics and backend-neutral memory architecture |
| Compiler-design references | Compiler stage and artefact contracts |
| Build-system design | Project and package orchestration |
| Progress matrix | Current implementation and backend support |
| Roadmap | Deferred implementation and future work |

A focused Advanced page may summarise an adjacent formal authority, but it must link instead of copying its complete architecture.

---

## 4. Documentation architecture

### 4.1 Existing page model remains fixed

The current documentation component foundation remains in place:

- one `#page.bst` route entry
- one unsuffixed Advanced file per independently useful concept
- one paired `-basic.bd` file
- Basic selected by default
- one independent selector per concept
- one page H1
- one stable H2 per concept outside both variants
- H3 and deeper inside imported fragments
- explicit Previous and Next navigation
- generated HTML produced only by the docs build

This plan does not redesign the selector, theme, pager or route component system.

### 4.2 `#page.bst`

The page entry owns:

- browser metadata
- navbar and breadcrumb
- the page H1
- friendly introduction
- concept ordering
- transitions
- optional editorial examples
- related links
- pager navigation
- calls to the existing Basic and Advanced component

It must not own the only copy of an exact language rule.

### 4.3 Basic references

A Basic file teaches the stable mental model.

It should normally contain:

1. A plain definition
2. Why the feature exists
3. The smallest useful valid example
4. A short explanation
5. One or two richer examples
6. Common mistakes a beginner is likely to make
7. One reliable rule to remember

Basic content must:

- introduce terms before using them
- stay accurate when Advanced edge cases are omitted
- prefer current usable syntax
- avoid compiler stage names and backend representation
- avoid exhaustive rejection lists
- avoid large status tables
- avoid accepted deferred syntax unless a short warning is necessary
- link to Advanced rather than compressing complex semantics into misleading prose

Basic may be longer than Advanced because teaching needs examples. It is semantically narrower, not mechanically shorter.

### 4.4 Advanced references

An Advanced file is the complete source-facing contract for its concept.

It should normally contain:

1. Concise definition
2. Canonical syntax
3. Exact semantic rules
4. Type and inference boundaries
5. Scope and receiving-context rules
6. Access, mutation, copy or lifetime effects where relevant
7. Evaluation-order guarantees where accepted
8. Edge cases
9. Invalid and rejected forms
10. Accepted deferred behaviour
11. Outside-scope behaviour
12. Links to adjacent source and architecture owners

Advanced content must:

- be directly readable without the page or Basic file
- use precise normative language
- distinguish source semantics from compiler implementation
- distinguish current support from accepted deferred design
- preserve important valid and invalid examples
- avoid repeating full compiler, build-system or memory architecture
- link to the progress matrix for current coverage
- contain every final semantic decision needed to replace the monolith

### 4.5 Status language

Use these categories consistently:

- **Current language:** accepted and implemented source behaviour
- **Accepted implementation gap:** final language behaviour that the current compiler does not yet implement correctly
- **Accepted deferred:** final design that belongs to a separate implementation plan
- **Rejected:** invalid current and final source form
- **Outside scope:** not part of the language design without a new design decision

Do not use "not supported" when one of these precise classifications applies.

### 4.6 Writing rules

Use the repository style guide.

In particular:

- use straight apostrophes
- avoid em dashes
- avoid prose semicolons
- keep headings concise
- use exact Beanstalk syntax
- label invalid examples clearly
- keep examples type coherent
- do not treat the reader as an agent or LLM
- retain legitimate LLM-aware language and tooling explanations

No automated prose rewriting is allowed. Search and read-only inventory tooling is allowed.

---

## 5. Accepted semantic decisions

These decisions are final for this plan. Route and compiler work must not reinterpret them.

### 5.1 Source-authored return-alias syntax is removed

The form:

```beanstalk
choose |first String, fallback String| -> first or fallback:
    return first
;
```

is not Beanstalk syntax.

Function signatures declare return value types and return channels only. They do not declare borrowed, owned or parameter-alias return categories.

Return-alias information remains a compiler semantic summary:

- source function summaries are inferred from validated function bodies and calls
- public interfaces may export inferred return-alias facts
- external binding metadata may state return aliasing because the compiler cannot inspect foreign bodies
- HIR and borrow validation may carry inferred side-table summaries
- the information is not a source type or signature form

The source syntax and all parser, AST and HIR plumbing that exists only to preserve it must be deleted.

There is no compatibility parser and no dedicated legacy diagnostic. The removed spelling fails through the normal current function-signature diagnostics.

### 5.2 `String` is one semantic surface

Quoted string slices and template-produced strings have one semantic `String` type at ordinary typed boundaries.

They share:

- parameter and return compatibility
- equality
- choice payload equality
- collection and map value use
- `String` map-key use
- casts
- IO and external `StringContent` boundaries
- template insertion
- aliases and generic use where `String` is concrete

Construction origin must not create hidden equality, hashing, map-key or call-compatibility rules.

#### Quoted string slices

A quoted form such as:

```beanstalk
name = "Priya"
```

creates a deliberately restricted string slice.

A string slice:

- is read-only content
- has no character or substring mutation surface
- is not concatenated with `+`
- may be read, compared, passed, returned, stored and inserted into templates
- may be held in a mutable binding, but binding mutability permits reassignment only

There is still no shadowing. A mutable binding that currently contains a slice may be reassigned. It does not make the slice contents mutable.

#### Template-produced strings

A template such as:

```beanstalk
message = [: Hello, [name]]
```

constructs a full owned string value.

Templates are the idiomatic and canonical way to:

- concatenate strings
- interpolate values
- build runtime or compile-time text
- produce strings that need full template composition

Concatenation is written:

```beanstalk
joined = [left, right]
```

not:

```beanstalk
joined = left + right
```

Templates are the only source form that constructs a full owned string. This plan does not add new in-place string mutation methods. Any future mutation-capable string API must preserve the slice versus owned-value distinction without creating two semantic `String` types.

Reactive template metadata remains orthogonal value metadata. It does not change `String` type identity or equality rules.

### 5.3 `+` is not string concatenation

`+` is a numeric operator only.

The compiler must reject `String + String` and every mixed string `+` form with the ordinary invalid-operator diagnostic.

Compile-time folding, HIR and backends must not retain a source-level string-add path.

Internal template assembly may use a backend or HIR string-append operation. That operation is an implementation detail and must not be represented as permission for source `+`.

### 5.4 `String` has equality but no ordering operators

`String` supports:

- `is`
- `is not`

`String` does not support:

- `<`
- `<=`
- `>`
- `>=`
- relational string match patterns

String ordering must not be backend-defined.

Future explicit text comparison helpers belong in a Core text package plan. They are not part of this migration.

### 5.5 `else =>` is the only full-match catch-all

A bare identifier is not a general capture pattern.

Valid binding patterns remain:

- option present capture with `|name|`
- declared choice payload captures
- renamed choice payload captures with `as`

The full-match catch-all is:

```beanstalk
else =>
```

or:

```beanstalk
else => body
```

A bare unknown name in choice pattern position is an unknown or invalid variant. A bare name in other full-match pattern positions is invalid syntax.

There is no compatibility capture diagnostic. Existing current-pattern diagnostics should report the invalid form.

### 5.6 Beandown implicit scope does not shadow

A `.bd` file may receive implicit compile-time constants from:

- the HTML builder package
- its same-directory module root public surface

If both surfaces expose the same visible name, compilation fails with a normal visible-name collision diagnostic.

Same-directory constants do not override `@html` constants.

The collision must identify both sources where location data is available. Authors resolve it by renaming one public constant.

---

## 6. Scope boundaries

### 6.1 Compiler changes owned by this plan

This plan owns compiler changes needed to:

- remove source-authored return-alias syntax
- make inferred return-alias summaries authoritative
- remove source string concatenation
- make runtime `String` equality and map-key behaviour uniform across slices and templates
- reject string ordering and relational string patterns
- remove general capture patterns
- reject Beandown implicit-scope collisions
- close the non-deferred implementation gaps listed in section 7.6 when they still reproduce

### 6.2 Documentation changes owned by this plan

This plan owns:

- every focused Basic and Advanced language page
- affected public page entries and navigation
- the focused language index
- source-facing synchronisation in `docs/language-overview.md`
- the complete focused design-scope owner
- a public source-facing Memory and Lifetimes route
- final Project Structure, Packages, Beandown and Markdown parity
- stable Core, Builder and external package language-facing contracts
- progress-matrix updates caused by implementation changes
- generated docs output from source changes

### 6.3 Work explicitly outside this plan

Do not absorb work already owned by a dedicated accepted plan, including:

- mandatory lifetime-region implementation
- `group` / `into` implementation
- optional ownership and path-dependent transfer drift
- project `#Import`, source `#Import`, `@project` and anonymous-record implementation
- entry-local `config:` implementation
- package manager, dependency resolution and lockfiles
- broad Core package expansion
- future string ordering helpers
- HTML-Wasm feature parity
- post-TIR performance work
- async or concurrency syntax
- glossary, sidebar or global Basic and Advanced persistence

Advanced language docs may document accepted end-state syntax from those authorities with explicit status and links.

---

## 7. Compiler realignment workstreams

Each workstream must leave source semantics, tests, focused docs, the monolith and the progress matrix mutually consistent.

### 7.1 Remove source-authored return aliases

#### Parser and declaration syntax

Current implementation anchors include:

- `src/compiler_frontend/declaration_syntax/signature_members.rs`
- `src/compiler_frontend/headers/tests/parse_file_headers_tests.rs`

Required work:

- delete `FunctionReturnSyntax::AliasCandidates`
- delete parameter-name detection in return-type parsing
- delete `parse_alias_return_item_syntax`
- delete alias-only signature validation and helper functions
- delete alias-only diagnostic reasons when no longer used
- parse return slots as type annotations plus optional error-channel markers only
- do not add a removed-syntax compatibility diagnostic

#### AST and type resolution

Current anchors include:

- `src/compiler_frontend/ast/statements/functions.rs`
- `src/compiler_frontend/ast/type_resolution/signatures.rs`
- `src/compiler_frontend/ast/type_resolution/resolve_type.rs`
- `src/compiler_frontend/ast/generic_functions/calls.rs`
- `src/compiler_frontend/ast/module_ast/environment/traits.rs`

Required work:

- remove `FunctionReturn::AliasCandidates`
- collapse `FunctionReturn` if the enum no longer represents more than a typed value
- remove alias-candidate type-resolution branches
- remove alias-only generic and trait handling
- remove alias-specific comparison and fixture helpers
- keep return value type and error-channel semantics unchanged

#### HIR and public interfaces

Required work:

- remove source-declared return-alias arrays from `HirFunction` and related validation, display and construction paths
- remove AST-to-HIR transfer of source alias candidates
- retain a separate inferred return-alias summary type
- export inferred summaries through public semantic interfaces where callers need them
- keep external binding return-alias metadata because foreign bodies are unavailable for analysis

#### Borrow analysis

Current analysis already classifies return expressions. That inferred classification becomes authoritative.

Required work:

- compute `Fresh`, `AliasParams` or `Unknown` from validated HIR
- remove validation against source-declared alias candidates
- make forwarded-call classification consume computed callee summaries
- compute same-module summaries in deterministic dependency order or by a monotone fixed point over the call graph
- handle recursive or unresolved cycles conservatively as `Unknown`
- consume completed provider summaries for cross-module calls
- keep lack of optional transfer proof non-diagnostic
- keep unknown alias topology conservative rather than inventing ownership

#### Cleanup gates

The completed slice must have no source-syntax support for:

```text
-> parameter
-> first or fallback
```

when those names are function parameters rather than types.

Search for and remove obsolete code and tests involving:

- `AliasCandidates`
- alias-return parser helpers
- alias-return type mismatch helpers
- HIR source-declared `return_aliases`
- valid source fixtures using parameter names as return slots

Do not remove general inferred or external return-alias metadata.

### 7.2 Simplify the `String` surface

#### AST operator policy

Current anchors include:

- `src/compiler_frontend/ast/expressions/eval_expression/operator_policy/arithmetic.rs`
- `src/compiler_frontend/ast/expressions/eval_expression/operator_policy/comparison.rs`
- `src/compiler_frontend/ast/expressions/eval_expression/operator_policy/shared.rs`
- `src/compiler_frontend/ast/const_eval/`

Required work:

- delete the plain-string `+` typing branch
- delete compile-time string-add folding
- make equality accept all runtime values with semantic type `String`
- do not gate equality on `PlainStringSlice`
- keep compile-time path values outside runtime string operators
- remove `both_plain_string_slices` if no remaining semantic owner needs it
- review `ExpressionValueShape` and retain only distinctions needed for parsing, ownership, reactive metadata or lowering
- ensure value shape does not silently change equality, hashing, map-key legality or call compatibility

#### Slice and owned-string semantics

The compiler must preserve:

- quoted literal construction as a restricted slice
- template construction as an owned string
- mutable binding reassignment without slice-content mutation
- one semantic `String` `TypeId`
- ordinary contextual compatibility between slices and template strings

Do not introduce `StringSlice` as a second public type in this plan.

#### Choice equality and map keys

Required work:

- make choice payload equality accept every `String` value
- make option and nested choice equality recurse through `String` consistently
- make `String` map keys use the same content equality and hashing regardless of source construction
- remove template-origin rejection paths that exist only because a value came from a template
- keep compile-time paths and non-runtime metadata out of map keys

#### HIR and backend boundary

Current anchors include:

- `src/compiler_frontend/hir/hir_expression/operators.rs`
- `src/compiler_frontend/hir/validation/expressions.rs`
- `src/compiler_frontend/hir/hir_expression/templates/`
- `src/backends/js/`
- `src/backends/wasm/`

Required work:

- ensure source string `+` cannot reach HIR
- separate internal template concatenation from source binary-operator permission
- rename or narrow plain HIR `Add` handling if it becomes template-internal only
- keep template append and accumulator lowering intact
- delete backend branches and comments that describe source string addition
- keep backend-native string assembly hidden behind template semantics

#### Test coverage

Add or update coverage for:

- quoted slice equality
- template string equality
- slice versus template equality
- `String?` equality where applicable
- choice payload equality with slice and template values
- `String` map keys produced by both source forms
- `String + String` rejection
- mutable binding reassignment of a quoted slice
- rejection of slice-content mutation forms
- template concatenation through `[left, right]`

### 7.3 Remove string ordering

Current anchors include:

- `src/compiler_frontend/ast/statements/match_patterns/relational.rs`
- relational pattern fixtures
- HIR and backend match tests

Required work:

- remove `String` from ordered relational pattern subjects
- retain `Int`, `Float` and `Char`
- keep ordinary `String` comparison limited to equality
- replace current string relational success fixtures with rejection coverage
- remove backend-specific ordering wording and dead branches
- update benchmark fixtures that use relational string patterns

The rejection should use the normal invalid relational-pattern or invalid comparison diagnostic.

### 7.4 Remove general capture patterns

Current anchors include:

- `src/compiler_frontend/ast/statements/match_headers.rs`
- `src/compiler_frontend/ast/statements/match_patterns/types.rs`
- `src/compiler_frontend/ast/statements/match_exhaustiveness.rs`
- `src/compiler_frontend/hir/hir_statement/match_captures.rs`
- `src/compiler_frontend/hir/hir_statement/control_flow.rs`
- HIR validation and display
- JS match lowering
- branching and match tests

Required work:

- delete `MatchPattern::Capture`
- delete bare-symbol capture construction
- delete arm-scope creation used only for whole-scrutinee capture
- delete `HirPattern::Capture` and its lowering, validation, display and backend paths
- remove capture-specific reachability and exhaustiveness handling
- make unknown choice names report invalid or unknown variants
- make bare identifiers invalid for non-choice full matches
- preserve option `|name|` and choice payload captures
- keep `else =>` as the only catch-all

Do not add a legacy general-capture diagnostic.

### 7.5 Reject Beandown implicit-scope collisions

Current anchors include:

- `src/compiler_frontend/headers/import_environment/builder.rs`
- `src/compiler_frontend/headers/tests/beandown_prepare_tests.rs`

Required work:

- route implicit `@html` and same-directory root constants through the ordinary visible-name registry
- stop collecting them into an overwriteable name map
- preserve enough source and export location data for a two-source collision diagnostic
- reject collisions before AST folding
- keep unique constants from both implicit surfaces visible
- retain filtering to exported compile-time constants and const records
- keep the generated `content` constant out of its own implicit scope
- replace the precedence test with collision coverage

The result must use the same no-shadowing model as ordinary source visibility.

### 7.6 Close accepted non-deferred compiler gaps

Reproduce these against current `main` before implementation. If a gap has already been fixed, record the evidence and remove it from the active ledger.

If still present, this plan owns the correction because the final semantics are accepted and the work is not explicitly deferred:

1. **Option payload equality inside choices**
   - `T?` supports equality when `T` supports equality
   - recursive equality queries must recognise option construction
   - nested choice and option checks must remain cycle safe

2. **Cross-choice inline predicate validation**
   - `if status is Ready then ...` must validate that `Ready` belongs to the scrutinee's nominal choice
   - a variant from another choice is rejected before HIR

3. **Nested-block `return!` in error-only functions**
   - `return!` is terminal from any legal nested control-flow block
   - error-only functions may otherwise fall through normally

4. **Block value-producing `if` with `then`**
   - the accepted block form must reach AST, HIR and backend lowering without an infrastructure failure
   - receiving arity and terminality remain the AST owner's responsibility

5. **Stored named template inserts**
   - a stored named insert accepted by the language must preserve its slot name when contributed later
   - slot routing must not depend on direct source placement only

Each correction requires focused success, rejection, HIR and integration coverage as appropriate.

### 7.7 Additional discrepancy rule

During parity work, classify every new mismatch as one of:

- documentation defect
- narrow implementation defect against accepted semantics
- explicitly deferred implementation
- outside-scope implementation accident
- unresolved design question

A narrow implementation defect may join this plan only when:

- the final design is already explicit
- no dedicated plan owns it
- the fix does not expand the language surface
- the patch remains reviewable

Otherwise record it and route it to the correct plan. Do not silently widen this project.

---

## 8. Documentation workstreams

### 8.1 Whole-language ownership ledger

Create and maintain a section-level ledger inside this plan or a deliberately named companion ledger approved by the user.

Each row must record:

| Field | Meaning |
|---|---|
| Source heading or delegated authority | Original rule source |
| Advanced owner | Final unsuffixed file |
| Basic owner | Teaching file |
| Public route | Importing page |
| Related formal owner | Memory, compiler or build-system link |
| Advanced complete | Yes or no |
| Basic complete | Yes or no |
| Important examples preserved | Yes or no |
| Implementation checked | Yes, no or not applicable |
| Current discrepancy | Description or none |
| Status | Current, gap, deferred, rejected or outside scope |

A section is not complete because text was copied. It is complete only when the final owner is direct-read complete and the Basic explanation remains true.

### 8.2 Strings and characters

Update:

- `docs/src/docs/language-overview/strings-and-characters.bd`
- its Basic partner
- the Language Basics page
- related Templates, Numbers, Functions, Collections, Choices, Casts, Traits and IO references

Advanced must define:

- quoted escape rules
- quoted slices as restricted read-only slices
- template strings as owned string construction
- one semantic `String` type
- binding reassignment versus content mutation
- equality across both source forms
- no string `+`
- no string ordering
- templates as canonical concatenation

Basic must show:

```beanstalk
first = "Hello, "
second = "world"
message = [first, second]
```

and explicitly warn against:

```beanstalk
message = first + second
```

Audit every source and documentation example that currently uses `+` for text, including function-default, trait-bound and package examples.

### 8.3 Functions and returned values

Update the Functions and Memory references to state:

- signatures return typed values only
- returning an existing value preserves ordinary shared-reference semantics
- the compiler infers alias and freshness effects
- authors do not write borrowed, owned, move or return-alias annotations
- external bindings may have compiler-owned alias metadata

Remove every valid example of parameter names in return slots.

Do not expose the internal summary lattice as source syntax.

### 8.4 Branching and pattern matching

Update Advanced Branching references to define only:

- literal patterns
- choice variants
- choice payload captures
- option patterns and `|name|`
- relational patterns for `Int`, `Float` and `Char`
- guards
- exact exhaustiveness rules
- `else =>` as the sole catch-all

Remove:

- general capture
- string relational patterns
- backend-defined ordering language
- examples that rely on misspelled variants becoming bindings

Basic should teach explicit `else =>` and avoid an exhaustive pattern taxonomy.

### 8.5 Beandown

Update Beandown Advanced references to state:

- the implicit body is a const-required `$md` template
- only allowed exported constants and const records enter the flat scope
- `@html` and same-directory root constants do not shadow
- duplicate visible names are errors
- authors resolve collisions by renaming the module-root export
- runtime values, functions, types and the generated self `content` constant remain unavailable

Basic may omit the collision edge until its common-mistakes section.

### 8.6 Public Memory and Lifetimes route

Add a public route under:

```text
docs/src/docs/memory/
```

Recommended concept pairs:

```text
reference-semantics.bd
reference-semantics-basic.bd

copy-and-exclusive-access.bd
copy-and-exclusive-access-basic.bd

lifetimes-and-result-shapes.bd
lifetimes-and-result-shapes-basic.bd

declared-memory-groups.bd
declared-memory-groups-basic.bd
```

The route owns source-facing behaviour:

- existing values use shared read-only access by default
- alias activity is non-lexical
- mutable aliases write through
- fresh mutable slots are independent
- `~place` requests exclusive access
- `copy` creates an independent graph
- aggregate storage retains ordinary reference semantics
- fresh, alias and independent result distinctions
- source-visible lifetime and escape consequences
- accepted deferred `group` / `into` syntax

The route must link to the formal memory references for:

- region topology
- retained-edge rules
- borrow-analysis algorithms
- ownership side tables
- backend allocation and release

Basic should teach shared access, `copy` and `~`. It should not teach `group` / `into` as current syntax. The paired Basic group file may state briefly that the feature is accepted but deferred and that current programs do not use it.

Update Bindings, Functions, Collections, Maps and Reactivity to link to this route instead of duplicating the complete memory model.

### 8.7 Collections and Maps memory correction

Rewrite Advanced collection and map wording to match the final memory model:

- containers own their structure
- stored existing values follow shared, copy and inferred-transfer semantics
- insertion is not an implicit deep copy
- `copy` is required for independent child graphs
- `get` returns shared access
- a live lookup alias blocks conflicting mutation
- `remove` returns the removed value under ordinary lifetime and ownership rules
- scalar representation does not create a source-level implicit-copy exception

Remove blanket wording that maps or collections automatically own independent duplicates of inserted keys or values.

### 8.8 Project Structure and Packages reopening

Re-audit the completed Project Structure and Packages routes against the final build-system design.

Advanced Project Structure must cover the source-facing projection of:

- self-contained `config.bst`
- the open `project` record
- builder and tooling sections
- direct project `#Import`
- source `#Import`
- explicit `@project` imports
- root-local `config:` blocks
- normal `#*.bst` roots
- API-only `+*.bst` support roots
- the project package facade
- active versus dormant normal-root work
- directory-based routes and builder-owned artifacts

Split large concepts when needed. Recommended additions:

```text
build-inputs.bd
build-inputs-basic.bd

entry-config.bd
entry-config-basic.bd

project-package-facade.bd
project-package-facade-basic.bd
```

Correct all stale claims that:

- config may import support files or packages
- config support types are accepted
- support-root runtime or fragments are merely inactive
- `#page.bst` and `#mod.bst` have different semantics
- `package_folders` or default `lib/` scanning exists
- root filenames choose HTML routes

Support roots and the project facade reject top-level runtime work and fragments. This is source legality, not inactive builder behaviour.

Basic should teach the current simple project shape. Accepted deferred config and package surfaces should be clearly labelled and kept out of the beginner path until implemented.

### 8.9 Core, Builder and external package surfaces

Add a focused public route or a clearly separated section under Packages. Do not leave the complete stable package surface only in the monolith or progress matrix.

Recommended route:

```text
docs/src/docs/core-packages/
```

Recommended concept pairs:

```text
core-io.bd
core-io-basic.bd

core-math.bd
core-math-basic.bd

core-text.bd
core-text-basic.bd

core-random-and-time.bd
core-random-and-time-basic.bd

builder-packages.bd
builder-packages-basic.bd

external-package-contracts.bd
external-package-contracts-basic.bd
```

Advanced owns language-facing contracts:

- import names
- stable public functions and opaque types
- parameter, return, access and error behaviour
- prelude policy
- source-backed versus binding-backed behaviour visible to authors
- explicit close or teardown requirements
- restricted host-value boundaries
- unsupported source forms

The progress matrix owns current target availability.

The build-system design owns provider registration, package graph construction, runtime assets and linking.

The memory references own retention and external-resource lifetime rules.

### 8.10 Complete focused design-scope owner

Add or expand a focused Advanced reference so the full exact outside-scope list no longer exists only in the monolith.

Recommended route:

```text
docs/src/docs/design-scope/
```

Recommended concept pairs:

```text
design-principles.bd
design-principles-basic.bd

deferred-and-outside-scope.bd
deferred-and-outside-scope-basic.bd

excluded-language-families.bd
excluded-language-families-basic.bd
```

Advanced must preserve:

- the exact deferred versus outside-scope distinction
- every excluded feature family
- the rationale
- the constrained Beanstalk mechanism used instead

Basic should explain the language's bias without presenting a long exclusion inventory.

### 8.11 Existing route final audit

Every existing route receives a final audit even when no known discrepancy is listed.

Review for:

- complete monolith parity
- final syntax
- source-form distinctions
- invalid examples
- deferred and outside-scope coverage
- direct-reading quality
- Basic truthfulness
- stale compiler-status wording
- stale links
- type coherence
- page heading and navigation structure
- generated output

The focused-reference index is updated only after the route's Advanced owners are complete.

---

## 9. Route status and required follow-up

| Route | Current state | Required follow-up |
|---|---|---|
| Getting Started | Existing public route | Recheck examples after config and string changes |
| Language Basics | Migrated | String slice and template model, final parity |
| Values and Bindings | Migrated | Link to final memory route, correct alias wording |
| Numbers | Migrated | Remove string `+` from operator surface |
| Casts | Migrated | Uniform `String` source compatibility |
| Functions | Migrated | Remove source return aliases, replace string `+` examples |
| Branching | Migrated | Remove general capture and string ordering |
| Loops | Prototype migrated | Final parity only unless review finds defects |
| Structs | Migrated | Final parity and memory links |
| Choices | Migrated | Uniform `String` equality and option equality fix |
| Errors, Options and Assertions | Migrated | Close accepted return and value-block gaps |
| Collections and Maps | Migrated | Final memory semantics and uniform `String` keys |
| Templates | Migrated | Canonical concatenation and stored named insert fix |
| Constants | Migrated | String construction and config/build-input links |
| Aliases | Migrated | Final parity |
| Generics | Migrated | Replace string `+` examples and final parity |
| Traits | Migrated | Replace string `+` examples and final parity |
| Reactivity | Migrated | Uniform `String` semantics and memory links |
| Memory and Lifetimes | Missing public route | Add Basic and Advanced pairs |
| Project Structure | Migrated but stale areas remain | Reopen against final build-system design |
| Packages and Imports | Migrated but incomplete | Reopen, add facade and package surface |
| Core and Builder Packages | Not fully owned | Add focused language-facing owners |
| Beandown | Migrated | Replace precedence with collision semantics |
| Plain Markdown | Migrated | Final parity and boundary links |
| Design Scope | Summary only | Add complete focused owner |

---

## 10. Delivery sequence

Semantic changes must be delivered in small slices. Do not combine unrelated removals into one unreviewable patch.

### Phase 0: Refresh and baseline

Before implementation:

1. Record `git rev-parse HEAD`, branch and `git status --short`
2. Read current `AGENTS.md`
3. Read this plan and all canonical authorities named by the active slice
4. Run baseline `just validate`
5. Record existing generated documentation changes separately
6. Reproduce the active discrepancy with one focused valid and invalid case
7. Identify exact source, test, docs and progress owners

Reading `AGENTS.md` does not authorise editing it.

### Phase 1: Remove source return-alias syntax

One semantic realignment patch should:

- remove parser, AST and HIR syntax plumbing
- make inferred summaries authoritative
- simplify data structures
- update tests
- update Functions and Memory docs
- synchronise the monolith and progress matrix
- rebuild affected routes

### Phase 2: Simplify `String`

One or more tightly related patches should:

- remove source string `+`
- separate internal template append from source operators
- make equality and map-key behaviour uniform
- preserve slice versus owned construction semantics
- update compiler tests
- update all affected focused docs and examples
- synchronise the monolith and progress matrix

Do not leave a state where docs recommend templates while compiler tests still bless source string addition.

### Phase 3: Simplify patterns

A focused patch should:

- reject string relational patterns
- remove general capture
- remove AST, HIR and backend variants
- update branching tests and benchmarks
- update Branching Basic and Advanced docs
- synchronise the monolith and progress matrix

### Phase 4: Align Beandown collisions

A focused patch should:

- use ordinary collision registration
- preserve two-source diagnostics
- update compiler tests
- update Beandown docs
- synchronise the monolith and progress matrix

### Phase 5: Close accepted non-deferred gaps

Fix each confirmed gap from section 7.6 in its own reviewable patch or a clearly coherent small batch.

Each patch updates its owning docs and status rows.

### Phase 6: Add missing focused owners

Recommended order:

1. Memory and Lifetimes
2. Project Structure and build inputs
3. Packages and project facade
4. Core, Builder and external package surfaces
5. Complete Design Scope

### Phase 7: Whole-language parity audit

After all route work:

- audit every monolith section and delegated formal authority
- audit every Advanced file as a direct reference
- audit every Basic file for truthfulness
- inspect every public route
- review deferred and outside-scope coverage
- review every compiler discrepancy
- verify the progress matrix
- verify the focused-reference index
- present any remaining mismatch to the user

### Phase 8: Authority switch

Only after explicit user approval:

- update `AGENTS.md`
- update focused language index authority wording
- decide whether the monolith is retained as a legacy consolidated reference, reduced to an index or removed
- make the selected disposition in a separate patch

---

## 11. Validation

### 11.1 Code-bearing or mixed semantic slice

When Rust, tests, fixtures or other implementation files change:

```sh
cargo fmt
just validate
bean build docs --release
```

Use the equivalent Cargo docs build when no suitable release `bean` is available.

Also perform a manual architecture audit:

- source syntax has one parser owner
- obsolete variants and adapters are gone
- inferred return aliases remain side-table or interface facts
- HIR is not mutated by borrow analysis
- template append is not confused with source string addition
- `TypeId` remains the semantic type authority
- value-shape metadata does not become a hidden second string type
- diagnostics use current structured families
- backends do not reinterpret removed source syntax
- no compatibility shim remains

### 11.2 Documentation-only slice

For a documentation-only patch:

```sh
bean build docs --release
```

or:

```sh
cargo run --quiet -- build docs --release
```

Do not run the full code-bearing gate for a strictly documentation-only slice.

### 11.3 Targeted iteration

Use focused commands during development:

- relevant Rust unit tests
- `bean tests` or selected integration cases
- `bean check docs`
- route-specific docs builds
- targeted compiler probes under `tmp/docs-language-probes/`

Delete temporary probes before completion.

Passing targeted checks does not replace the required final gate.

### 11.4 Generated documentation

- do not edit `docs/release/**` manually
- retain generated changes produced by source edits
- inspect changed routes
- verify one H1, stable concept headings, Basic default, selector independence, links and pagers
- inspect code blocks, tables, narrow layout and dark mode where affected
- report which routes were manually inspected

### 11.5 Search-zero checks

Each removal slice must run targeted searches for obsolete names, syntax and comments.

A zero count is required for obsolete source support, except where text intentionally explains that a form is invalid.

Examples:

```text
AliasCandidates
parse_alias_return_item_syntax
general capture
MatchPattern::Capture
HirPattern::Capture
plain string concatenation via +
local constants win on collisions
```

Search results must be reviewed semantically. Do not apply blind global replacement.

---

## 12. Completion criteria

The migration is ready for final authority review only when:

- every monolith section has one focused Advanced destination
- every delegated memory, compiler or build-system rule has a source-facing owner or link
- every Advanced concept has a Basic partner
- every public page imports both levels
- Basic remains the default
- Advanced files are direct-read complete
- Basic files remain accurate simplifications
- important valid and invalid examples are preserved
- source return-alias syntax is fully removed
- inferred and external return-alias summaries remain correct
- all runtime `String` values use one semantic surface
- string `+` is rejected
- templates are documented and implemented as canonical concatenation
- string ordering is rejected
- general capture is removed
- `else =>` is the only catch-all
- Beandown implicit names collide instead of shadowing
- accepted non-deferred compiler gaps are closed or explicitly reclassified by the user
- memory source semantics match the final memory references
- Project Structure and Packages match the final build-system design
- the complete design-scope list has a focused owner
- Core, Builder and external package language contracts have focused owners
- every route has correct headings, anchors and navigation
- generated HTML has been inspected
- the progress matrix reflects current support
- no generated HTML was edited manually
- no obsolete compatibility path remains
- `AGENTS.md` remains unchanged until the final switch
- all required validation gates pass

---

## 13. Required report for each slice

Every implementation report must include:

### Scope

- semantic decision or route covered
- source authorities read
- starting commit and branch

### Source changes

- compiler files changed
- test and fixture files changed
- documentation files changed
- generated files changed

### Deletions and simplification

- obsolete syntax, variants, helpers, diagnostics and tests removed
- remaining similarly named concepts that are intentionally retained

### Semantic result

- final accepted rule
- valid forms
- invalid forms
- implementation status
- any conservative behaviour

### Documentation parity

- monolith headings reviewed
- Advanced owner for each rule
- Basic owner for each concept
- examples replaced or removed
- routes inspected

### Validation

Report exact results of:

- targeted tests
- `cargo fmt` when Rust changed
- `just validate` for code-bearing work
- documentation release build
- generated route inspection

Do not claim a command or inspection that was not performed.

### Remaining uncertainty

Report any:

- unresolved design question
- implementation conflict
- deferred dependency
- incomplete parity row
- generated output not inspected in full

Do not hide uncertainty to declare a slice complete.

---

## 14. Resolved discrepancy ledger

| Surface | Decision | Required action |
|---|---|---|
| Parameter-alias return syntax | Removed | Delete source syntax and infer summaries |
| Quoted and template strings | One semantic `String` surface | Remove origin-based equality and map restrictions |
| String `+` | Rejected | Use templates for concatenation |
| String ordering | Rejected | Remove relational string comparisons and patterns |
| General full-match capture | Removed | Use `else =>` only |
| Beandown local-over-HTML precedence | Removed | Diagnose visible-name collision |

---

## 15. Explicitly deferred or separately owned work

These items remain visible but do not block this plan when their ownership and status are correct:

| Surface | Owner |
|---|---|
| Lifetime-region and escape validation implementation | Memory roadmap |
| `group` / `into` implementation | Grouped-memory plan |
| Ownership optimisation precision and path-dependent transfer drift | Memory/compiler follow-up plans |
| Self-contained config, build inputs, `@project` and anonymous records implementation | Config and build-input plan |
| Entry-local `config:` and runtime title implementation | Entry-config plan |
| Package manager and dependency solver | Package roadmap |
| HTML-Wasm feature parity | Backend roadmap and progress matrix |
| Future text ordering and locale APIs | Future Core text plan |
| Async and concurrency | Dedicated future design |
| Sidebar, glossary and global docs preference | Documentation roadmap |

Accepted deferred semantics still require complete Advanced source-facing documentation and accurate links.
