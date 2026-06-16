# Beanstalk Compiler Design Drift Implementation Plan

## Goal

Bring the compiler and documentation back into alignment with the reviewed design direction:

- `#config.bst` remains a Beanstalk compile-time source file, but authored config entries must be known top-level `#` constants.
- Plain top-level `name = value` config entries are removed.
- Traits, trait conformances, and trait incompatibility declarations are rejected in authored config.
- Function fall-through diagnostics move from HIR lowering to AST-owned terminality validation.
- HIR lowering and HIR validation become infrastructure-only error boundaries.
- `docs/compiler-design-overview.md` becomes the central compiler architecture guide, with backend progress/status details moved out to the progress matrix.
- The progress matrix is refreshed after the compiler and design-document updates land.

This plan is optimized for coding agents. Each phase should fit in one focused implementation context and ends with an audit, style-guide review, and validation gate.

## Current repo anchor

Anchor this plan against commit `2e80c26c8d378db69820e8fe7294d7cee0e501ee`.

That commit landed the Core IO namespace work:

- callable `io(...)` was replaced by `io.line(...)` and related `io.*` namespace calls
- nested external namespace traversal was added for surfaces such as `io.input.*`
- public `IO` was removed
- language/docs-site/progress docs were updated for the Core IO change
- `docs/compiler-design-overview.md` was not updated for this review

Current design drift confirmed during review:

- `src/build_system/project_config/validation.rs` still extracts config entries from both `ast.module_constants` and the implicit config start body.
- `src/build_system/project_config/parsing.rs` still permits authored `HeaderKind::Trait`, `HeaderKind::TraitConformance`, and `HeaderKind::TraitIncompatibility` in config structural validation.
- `src/compiler_frontend/hir/hir_builder.rs` still has `HirLoweringError::Diagnostic`.
- `src/compiler_frontend/hir/hir_statement.rs` still emits `InvalidReturnShapeReason::FunctionMayFallThrough` from HIR lowering.
- `docs/language-overview.md`, `docs/src/docs/project-structure/#page.bst`, `docs/src/docs/libraries/#page.bst`, and `docs/src/docs/progress/#page.bst` still mention the old plain config key style and must be updated when the compiler config change lands.

If more commits land before implementation starts, rerun Phase 0 and update this anchor section before changing code.

## Implementation principles

- Prefer one current API shape. Do not add compatibility wrappers for old config syntax.
- Keep user-facing diagnostics on `CompilerDiagnostic`.
- Keep infrastructure and compiler-invariant failures on `CompilerError`.
- Keep config parsing frontend-backed. Remove the special plain-binding interpretation only.
- Keep AST/HIR stage boundaries explicit.
- Prefer focused helpers and named enums over broad booleans.
- Do not move backend progress/status language into the design overview. That belongs in `docs/src/docs/progress/#page.bst`.

---

## Phase 0 - Baseline audit and repo-shape confirmation

### Context

The Core IO commit landed during the documentation review. Before changing compiler behavior, confirm that the repo still matches the anchors above and that no later commit has already fixed part of this plan.

### Checklist

- [ ] Confirm the current `HEAD` and compare it to anchor commit `2e80c26c8d378db69820e8fe7294d7cee0e501ee`.
- [ ] Search for config plain-binding extraction:
  - [ ] `rg "implicit start function body|module_constants|validate_and_apply_config_ast|plain immutable" src/build_system/project_config`
  - [ ] Confirm `validate_and_apply_config_ast` still extracts from the implicit start body before editing.
- [ ] Search for config trait allowance:
  - [ ] `rg "TraitConformance|TraitIncompatibility|validate_authored_config_surface" src/build_system/project_config`
  - [ ] Confirm authored config still allows trait-related headers before editing.
- [ ] Search for HIR diagnostic exception:
  - [ ] `rg "HirLoweringError|FunctionMayFallThrough|invalid_return_shape" src/compiler_frontend/hir src/compiler_frontend/ast`
  - [ ] Confirm HIR still emits the fall-through diagnostic before editing.
- [ ] Search for old IO syntax after the landed Core IO patch:
  - [ ] `rg "\bio\s*\(" README.md docs tests src`
  - [ ] `rg "\bIO\b" README.md docs tests src`
  - [ ] Treat remaining hits as drift only if they are not compatibility diagnostics, migration tests, or historical plan text.
- [ ] Search for old config syntax in docs and fixtures:
  - [ ] `rg "project = \"html\"|entry_root =|library_folders =|plain immutable key" docs README.md tests src`
  - [ ] Save the relevant hit list for Phase 3.
- [ ] Open the current `docs/compiler-design-overview.md` and verify it has not already been rewritten.

### Phase 0 audit and validation gate

- [ ] Review findings against `docs/codebase-style-guide.md`.
- [ ] Update this plan's current repo anchor if any drift has already been corrected.
- [ ] Do not change production compiler behavior in this phase.
- [ ] If only notes changed, run the docs/static-site check if available.
- [ ] If any Rust or test file changed, run `cargo fmt` and the narrowest relevant tests.

---

## Phase 1 - Remove special config plain-binding parsing

### Context

`#config.bst` should remain a Beanstalk compile-time config source file, but authored config entries must be normal top-level `#` constants. Plain top-level bindings are runtime/start-body syntax and should be rejected in config instead of interpreted as config keys.

This phase removes the special config-only interpretation of start-body `VariableDeclaration` nodes while keeping the existing tokenizer, header parsing, dependency sorting, AST construction, const facts, imported support constants, and `ProjectConfigKeyRegistry`.

### Checklist

- [ ] Update `src/build_system/project_config/validation.rs`.
  - [ ] In `validate_and_apply_config_ast`, extract config entries only from `parsed_config.ast.module_constants` authored in `#config.bst`.
  - [ ] Remove extraction of `NodeKind::VariableDeclaration` from the implicit config start body.
  - [ ] Keep a start-body scan only for diagnostics on authored config runtime/start-body statements.
  - [ ] Report authored plain config bindings with a targeted config diagnostic such as “config keys must be compile-time constants”.
  - [ ] Prefer adding a dedicated `InvalidConfigReason` for plain config bindings if no good existing reason exists.
  - [ ] Keep `NodeKind::PushStartRuntimeFragment` mapped to the existing standalone-template config diagnostic.
  - [ ] Keep other start-body nodes rejected as unsupported config statements.
- [ ] Ensure duplicate-key detection applies only to authored `#` constants.
  - [ ] Imported module constants remain support surface and must not become entries.
  - [ ] Duplicate imported support constants should not collide with authored config keys unless normal import visibility already rejects the source.
- [ ] Preserve config const-fact behavior.
  - [ ] Config `#` keys may reference earlier config constants.
  - [ ] Config `#` keys may reference constants imported from core or builder source libraries.
  - [ ] Values must still resolve through shared AST const facts.
- [ ] Preserve `ProjectConfigKeyRegistry` behavior.
  - [ ] Known key enforcement remains before applying core config fields.
  - [ ] Registered value-shape validation remains unchanged except where tests need syntax updates.
- [ ] Update diagnostics and render tests as needed.
  - [ ] Prefer stable diagnostic-code assertions in integration fixtures.
  - [ ] Avoid rendered-text-only assertions unless the message text itself is the behavior under test.
- [ ] Update config test fixtures and examples under `tests/` from plain keys to `#` keys where they are success cases.
  - [ ] Change `project = "html"` to `project #= "html"`.
  - [ ] Change `entry_root = "src"` to `entry_root #= "src"`.
  - [ ] Change `dev_folder = "dev"` to `dev_folder #= "dev"`.
  - [ ] Change `output_folder = "release"` to `output_folder #= "release"`.
  - [ ] Change `library_folders = {...}` to `library_folders #= {...}`.
- [ ] Add or update failure fixtures.
  - [ ] Plain `project = "html"` in `#config.bst` is rejected.
  - [ ] Plain `library_folders = {"lib"}` in `#config.bst` is rejected.
  - [ ] A config `#` key referencing an imported support constant still succeeds.
  - [ ] A config `#` key using folded const-record field projection still succeeds.

### Phase 1 audit and validation gate

- [ ] Manual stage-boundary review:
  - [ ] Config parsing still stops at AST.
  - [ ] Config does not get HIR.
  - [ ] Imported support declarations are not config entries.
  - [ ] User-facing config mistakes use `CompilerDiagnostic`.
  - [ ] Infrastructure failures use `CompilerError`.
- [ ] Style-guide review:
  - [ ] No compatibility wrapper preserves plain config keys.
  - [ ] No old helper remains with a misleading name.
  - [ ] New diagnostics carry structured facts and source locations.
- [ ] Run focused tests for project config if available.
- [ ] Run `cargo fmt`.
- [ ] Run `cargo run --quiet -- tests`.
- [ ] Run `cargo run --quiet -- check docs` if docs or fixtures changed.
- [ ] Run `just validate` before closing the phase.

---

## Phase 2 - Reject trait surfaces in authored config

### Context

Traits should not be supported in config. Authored `#config.bst` is a compile-time key/value surface with limited support declarations. Trait declarations, conformance evidence, and trait incompatibility metadata belong to ordinary source semantics, not config.

This is a small compiler change, but keep it separate from Phase 1 so diagnostics and fixtures remain focused.

### Checklist

- [ ] Update `src/build_system/project_config/parsing.rs`.
  - [ ] In `validate_authored_config_surface`, reject `HeaderKind::Trait`.
  - [ ] Reject `HeaderKind::TraitConformance`.
  - [ ] Reject `HeaderKind::TraitIncompatibility`.
- [ ] Add targeted config diagnostics.
  - [ ] Prefer dedicated `InvalidConfigReason` variants for trait declaration, conformance, and incompatibility if existing reasons are too vague.
  - [ ] Reuse a generic unsupported config declaration reason only if it keeps diagnostics clear.
- [ ] Confirm imported support files are unaffected.
  - [ ] The authored config file rejects traits.
  - [ ] Core/builder support libraries keep their normal source-language behavior.
- [ ] Add or update failure fixtures.
  - [ ] Trait declaration in authored `#config.bst` is rejected.
  - [ ] Trait conformance in authored `#config.bst` is rejected.
  - [ ] Trait incompatibility metadata in authored `#config.bst` is rejected.
- [ ] Verify no config docs say traits are accepted.

### Phase 2 audit and validation gate

- [ ] Manual stage-boundary review:
  - [ ] Config structural validation remains before AST where possible.
  - [ ] Rejection does not affect normal `.bst` source files.
  - [ ] Imported config support files remain governed by normal frontend semantics.
- [ ] Style-guide review:
  - [ ] Diagnostics are structured.
  - [ ] New code is not boolean-heavy if named states would be clearer.
  - [ ] No duplicate diagnostic construction has been introduced.
- [ ] Run focused config tests.
- [ ] Run `cargo fmt`.
- [ ] Run `cargo run --quiet -- tests`.
- [ ] Run `just validate`.

---

## Phase 3 - Move function fall-through diagnostics from HIR to AST terminality validation

### Context

Function fall-through is a source-language terminality error. It should be diagnosed before HIR lowering. HIR lowering should not have a special user-facing diagnostic path.

The optimal implementation is an AST-owned per-body terminality helper invoked immediately after each function body is parsed. This avoids a module-wide finalization pass and avoids threading terminality state through every token parser helper.

### Checklist

- [ ] Add `src/compiler_frontend/ast/statements/terminality.rs`.
  - [ ] Add a file-level doc comment explaining that this validates function-body terminality before HIR.
  - [ ] Define `FunctionTerminalityPolicy` with explicit variants:
    - [ ] `AllowImplicitUnit`
    - [ ] `RequireExplicitReturn`
    - [ ] `EntryStartImplicitReturn`
  - [ ] Add a structural helper over `&[AstNode]`, for example `validate_function_body_terminality`.
  - [ ] Return `Option<CompilerDiagnostic>` for predicate-style validation unless a richer result is needed.
- [ ] Implement conservative terminality rules.
  - [ ] `Return(_)` terminates the current function.
  - [ ] `ReturnError(_)` terminates the current function.
  - [ ] `Assert { condition: false, .. }` terminates when the condition is structurally a folded `Bool(false)`.
  - [ ] `ScopedBlock { body }` terminates when its body terminates.
  - [ ] `If(_, then_body, Some(else_body))` terminates only when both bodies terminate.
  - [ ] `If(_, _, None)` does not terminate.
  - [ ] `Match { exhaustiveness: HasDefault, arms, default }` terminates only when all arm bodies terminate and the default body terminates.
  - [ ] `Match { exhaustiveness: ExhaustiveChoice, arms, .. }` terminates only when all arm bodies terminate.
  - [ ] Loops are not terminal in the initial implementation unless a specific already-proven terminal loop form exists.
  - [ ] `Break`, `Continue`, `ThenValue`, declarations, assignments, expression statements, runtime-fragment pushes, and ordinary loops do not terminate the function.
- [ ] Invoke terminality validation in normal function emission.
  - [ ] In `src/compiler_frontend/ast/module_ast/emission/emitter.rs`, call the helper after `function_body_to_ast(...)` succeeds and before the function node is appended.
  - [ ] Use `AllowImplicitUnit` for functions with no success returns or unit success.
  - [ ] Use `RequireExplicitReturn` for functions with non-unit success returns.
  - [ ] Preserve current error-only function semantics where an empty success channel can implicitly return unit success.
- [ ] Invoke terminality validation for generic templates.
  - [ ] In `validate_generic_function_body`, validate the parsed body before discarding validation nodes.
  - [ ] In concrete generic instance emission, use the same helper defensively after parsing the substituted body.
  - [ ] If template validation should guarantee the concrete instance, document why any later failure is an invariant error.
- [ ] Preserve entry start behavior.
  - [ ] Use `EntryStartImplicitReturn` or skip normal terminality validation for `emit_start`.
  - [ ] Do not require user-authored page entries to explicitly return the runtime fragment vector.
- [ ] Remove HIR's user-facing diagnostic exception.
  - [ ] Remove `HirLoweringError::Diagnostic` from `src/compiler_frontend/hir/hir_builder.rs`.
  - [ ] Collapse local HIR lowering error plumbing to `CompilerError` where practical.
  - [ ] Change HIR fall-through detection in `lower_function_body_inner` to `CompilerError` with `ErrorType::HirTransformation`.
  - [ ] Update HIR file docs to remove the exception.
- [ ] Add or update integration fixtures.
  - [ ] Non-unit function with no return is rejected before HIR.
  - [ ] Non-unit function with partial `if` return is rejected before HIR.
  - [ ] Non-unit function ending in `assert(false)` succeeds.
  - [ ] Generic function template missing a required return is rejected before instantiation reaches HIR.
  - [ ] Error-only function with empty success path preserves current implicit unit-success behavior.

### Phase 3 audit and validation gate

- [ ] Manual stage-boundary review:
  - [ ] User-facing terminality errors use `CompilerDiagnostic` in AST.
  - [ ] HIR lowering now uses `CompilerError` only.
  - [ ] HIR validation remains infrastructure-only.
  - [ ] No HIR path constructs `InvalidReturnShapeReason::FunctionMayFallThrough`.
- [ ] Style-guide review:
  - [ ] `terminality.rs` has one owner responsibility.
  - [ ] API uses named policy states rather than loose booleans.
  - [ ] Comments explain why terminality belongs in AST.
  - [ ] No compatibility path preserves the old HIR diagnostic exception.
- [ ] Run focused AST/HIR tests if available.
- [ ] Run `cargo fmt`.
- [ ] Run `cargo run --quiet -- tests`.
- [ ] Run `just validate`.

---

## Phase 4 - Apply compiler-design overview replacement and docs alignment

### Context

The design overview should be the central architecture guide. It should not track backend progress or partial support status. It should describe design contracts, stage ownership, data flow, and implementation owners.

The drop-in `docs/compiler-design-overview.md` replacement from this review should be applied after the compiler drift fixes are underway or landed, because it asserts the intended ownership model.

### Checklist

- [ ] Replace `docs/compiler-design-overview.md` with the updated drop-in document.
- [ ] Verify the document style:
  - [ ] Uses only `-` bullets.
  - [ ] Avoids semicolons in prose.
  - [ ] Does not include backend progress/status lists.
  - [ ] Links to implementation owners near the relevant sections.
  - [ ] Links to docs-site pages for user-facing examples rather than duplicating them.
- [ ] Update HIR docs/comments touched by Phase 3.
  - [ ] Remove wording that says HIR lowering has a user-facing diagnostic exception.
  - [ ] Keep HIR validation documented as infrastructure-only.
- [ ] Update config docs for mandatory `#` config keys.
  - [ ] `docs/language-overview.md`
  - [ ] `docs/src/docs/project-structure/#page.bst`
  - [ ] `docs/src/docs/libraries/#page.bst`
  - [ ] `docs/src/docs/progress/#page.bst`
  - [ ] any config examples found by `rg "project = \"html\"|entry_root =|library_folders =" docs README.md tests src`
- [ ] Update wording in config docs.
  - [ ] Say `#config.bst` accepts only known top-level `#` constants as config entries.
  - [ ] Say plain top-level bindings are runtime/start-body syntax and are rejected in config.
  - [ ] Remove references to plain immutable config keys.
  - [ ] Remove the old note that const-record field projection is deferred for plain `=` keys.
- [ ] Verify Core IO documentation remains current after commit `2e80c26c`.
  - [ ] `rg "\bio\s*\(" README.md docs tests src`
  - [ ] `rg "\bIO\b" README.md docs tests src`
  - [ ] Update any non-historical stale hits to `io.line(...)`, `io.print(...)`, or the correct `io.input.*` form.
  - [ ] Do not update tests or docs that intentionally assert old syntax rejection unless the test expectation is stale.

### Phase 4 audit and validation gate

- [ ] Manual docs-boundary review:
  - [ ] Design overview describes architecture, not progress.
  - [ ] Progress matrix describes implementation status.
  - [ ] Language overview describes compiler-facing language facts.
  - [ ] Docs-site pages contain user-facing examples.
- [ ] Style-guide review:
  - [ ] No stale comments from old config or HIR diagnostic behavior.
  - [ ] No obsolete API wording remains.
  - [ ] New docs are terse and not repetitive.
- [ ] Run `cargo run --quiet -- check docs`.
- [ ] Run `cargo run --quiet -- tests` if examples or fixtures changed.
- [ ] Run `just validate`.

---

## Phase 5 - Refresh the progress matrix

### Context

`docs/src/docs/progress/#page.bst` is the right place for backend support status, current partial coverage, deferred surfaces, and progress notes. After the design overview drops progress/status language, the progress matrix should absorb any still-current status details and remove stale items.

This phase should happen after Phases 1 through 4 so the matrix reflects the actual compiler and docs state.

### Checklist

- [ ] Review `docs/src/docs/progress/#page.bst` from top to bottom.
- [ ] Update config rows.
  - [ ] Mandatory `#` config keys are described as current implementation.
  - [ ] Plain immutable config keys are listed as rejected, not accepted.
  - [ ] Config trait declarations/conformances/incompatibility metadata are listed as rejected.
  - [ ] Config examples use `#=`.
- [ ] Update HIR/terminality rows.
  - [ ] Fall-through rejection is described as AST/pre-HIR terminality validation after Phase 3 lands.
  - [ ] HIR lowering and validation are described as infrastructure-only invariant boundaries.
  - [ ] Remove or correct any row that implies HIR still emits the user-facing fall-through diagnostic.
- [ ] Verify Core IO rows after commit `2e80c26c`.
  - [ ] Core IO console namespace shows `io.line`, `io.print`, `io.debug`, `io.warn`, and `io.error` as the current surface if they are present in the landed docs.
  - [ ] Input namespace rows show `io.input.*` current support and backend target status.
  - [ ] Old callable `io(...)` and public `IO` are described only as removed/rejected migration surfaces.
- [ ] Verify namespace rows.
  - [ ] External package namespace records can be recursive for package-local symbol paths.
  - [ ] Source/facade namespace records remain shallow.
  - [ ] Remove stale blanket wording that all namespace imports are shallow.
- [ ] Review backend-status items removed from the design overview.
  - [ ] Move still-current backend support status into the matrix if it is not already there.
  - [ ] Remove stale unsupported/deferred/partial notes that no longer match implementation.
  - [ ] Keep progress matrix language current and explicit about status.
- [ ] Review Beandown rows.
  - [ ] Confirm `.bd` source-kind status still matches the implementation.
  - [ ] Confirm any direct API claims still match current source and tests.
- [ ] Review numeric/cast rows.
  - [ ] Confirm checked numeric, Float formatting, Float validation, and runtime cast status is current.
  - [ ] Keep backend parity gaps in the matrix, not in the design overview.

### Phase 5 audit and validation gate

- [ ] Manual docs-boundary review:
  - [ ] The matrix contains current implementation status.
  - [ ] The matrix does not become a second language specification.
  - [ ] The design overview does not need status notes to be re-added.
- [ ] Style-guide review:
  - [ ] Matrix rows are concise but specific.
  - [ ] Stale progress notes are removed rather than qualified with hedging.
  - [ ] Rows do not duplicate long explanations already present in docs-site pages.
- [ ] Run `cargo run --quiet -- check docs`.
- [ ] Run `just validate` if the docs check is part of normal validation.

---

## Phase 6 - Final full validation and handoff audit

### Context

This phase validates the entire change set after compiler behavior, design docs, language/docs-site config references, and progress matrix updates are complete.

### Checklist

- [ ] Run a final drift search:
  - [ ] `rg "project = \"html\"|entry_root =|library_folders =|plain immutable key" docs README.md tests src`
  - [ ] `rg "HirLoweringError::Diagnostic|FunctionMayFallThrough" src docs tests`
  - [ ] `rg "\bio\s*\(" README.md docs tests src`
  - [ ] `rg "\bIO\b" README.md docs tests src`
  - [ ] `rg "shallow namespace|nested traversal" docs src tests`
- [ ] Confirm all remaining hits are intentional.
  - [ ] Migration diagnostics and negative tests may mention old syntax.
  - [ ] Historical roadmap text may mention old behavior only when clearly marked historical.
  - [ ] No current user-facing examples should use old config plain keys or old `io(...)` calls.
- [ ] Run formatting and tests:
  - [ ] `cargo fmt`
  - [ ] focused config tests
  - [ ] focused AST/HIR terminality tests
  - [ ] `cargo test -p beanstalk --quiet`
  - [ ] `cargo run --quiet -- tests`
  - [ ] `cargo run --quiet -- check docs`
  - [ ] `just validate`
- [ ] Manual code review checklist:
  - [ ] Stage boundaries match the updated design overview.
  - [ ] User-facing diagnostics use `CompilerDiagnostic`.
  - [ ] Infrastructure failures use `CompilerError`.
  - [ ] No obsolete wrappers or compatibility paths remain.
  - [ ] Config parsing still reuses frontend machinery up to AST.
  - [ ] HIR lowering is infrastructure-only.
  - [ ] Progress matrix is the only document that tracks backend status/progress.
- [ ] Update the PR description or implementation notes.
  - [ ] Include the final commit anchor.
  - [ ] Include tests run.
  - [ ] Include any intentional remaining limitations.
  - [ ] Include any follow-up work deferred to the progress matrix or roadmap.

## Expected final state

After this plan is complete:

- `#config.bst` config entries are known top-level `#` constants only.
- Plain top-level bindings in config are rejected with a structured diagnostic.
- Authored config traits and conformance metadata are rejected.
- Function fall-through is diagnosed during AST terminality validation.
- HIR lowering has no user-facing diagnostic exception.
- `docs/compiler-design-overview.md` describes compiler architecture and stage ownership without backend progress/status leakage.
- `docs/language-overview.md` and docs-site pages use the new config key style.
- `docs/src/docs/progress/#page.bst` is current after config, terminality, and Core IO namespace changes.
