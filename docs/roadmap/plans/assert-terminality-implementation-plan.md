# Beanstalk implementation plan: `assert(false)` as terminal catch-handler control flow

## Goal

Teach the compiler that `assert(false)` and `assert(false, "message")` are statically terminal for AST value-production / catch-handler completeness analysis, so this pattern is accepted:

```beanstalk
ctx ~= get_canvas_context() catch |err|:
    io([:Failed to get canvas context: [err.message]])
    assert(false, "Canvas context retrieval failed")
;
```

The plan is optimized for a coding agent working in large but bounded chunks. Each phase is independently reviewable and ends with an explicit audit / style-guide / validation checkpoint.

## Current repo anchors

Primary code path:

- `src/compiler_frontend/ast/statements/fallible_handling/catch_handler.rs`
  - Parses catch handler body via `function_body_to_ast(...)`.
  - Calls `validate_catch_fallible_handler_value_requirement(...)` immediately after body parsing.
- `src/compiler_frontend/ast/statements/fallible_handling/validation.rs`
  - Calls `analyze_branch_flow(handler_body)`.
  - Rejects value-required catch handlers unless the body flow is `ProducesValue` or `Terminates`.
- `src/compiler_frontend/ast/statements/value_production/completeness.rs`
  - Owns `analyze_branch_flow(...)` and `statement_flow(...)`.
  - Currently treats `ThenValue`, `Return`, `ReturnError`, `If`, and `Match` specially.
  - Currently treats every other statement, including `NodeKind::Assert`, as `FallsThrough`.
- `src/compiler_frontend/ast/statements/value_production/types.rs`
  - Defines `BranchFlow` as the shared value-production completeness vocabulary.
- `src/compiler_frontend/ast/statements/asserts.rs`
  - Parses `assert(...)` into `NodeKind::Assert { condition, message }` after validating the condition is `Bool`.
- `src/compiler_frontend/ast/ast_nodes.rs`
  - Defines the `NodeKind::Assert` AST shape.
- `src/compiler_frontend/hir/hir_expression/fallible/catch.rs`
  - HIR catch lowering already accepts a handler body that has an explicit block terminator, and then resumes from the merge block for the success path.

Relevant design constraints:

- This fix belongs in AST, not HIR or borrow validation. The error is emitted during AST catch-handler validation before HIR lowering.
- Keep static terminality deliberately narrow: literal `false` only, plus any harmless wrapper that AST may introduce around that literal, such as `ExpressionKind::Coerced` if present.
- Do not try to prove arbitrary boolean expressions false in this feature. That would couple branch completeness to constant facts or expression evaluation and should be a separate design task.
- Do not change user-facing diagnostic construction for this feature. The existing `CatchHandlerCanFallThrough` diagnostic remains correct for dynamic assertions.

## Definition of done

- [ ] `assert(false)` in a value-required `catch` handler is classified as `BranchFlow::Terminates`.
- [ ] `assert(false, "message")` behaves the same way.
- [ ] Dynamic assertions, including `assert(condition)`, still fall through for static completeness purposes.
- [ ] Existing catch fallback behavior using `then` is unchanged.
- [ ] Existing `return` / `return!` terminal behavior is unchanged.
- [ ] HIR lowering remains consistent: a handler body accepted as terminal by AST must lower to an explicit HIR terminator.
- [ ] Unit tests cover branch-flow classification directly.
- [ ] AST/fallible-handling tests cover the original catch-handler failure mode.
- [ ] At least one end-to-end or integration fixture covers compilation of the source pattern through the normal compiler path.
- [ ] Documentation is verified. If docs are changed, the changed files are listed in the final implementation notes.
- [ ] `just validate` passes, or any failure is documented with the exact failing command and reason.

---

## Phase 0 — Baseline audit and reproduction

### Context / reasoning

Before changing code, confirm the observed failure is still caused by AST catch-handler completeness and not by a later HIR assertion-lowering issue. This keeps the implementation targeted and prevents a coding agent from moving logic into the wrong compiler stage.

### Steps

- [ ] Create a local branch named something like `fix/assert-false-catch-terminality`.
- [ ] Run the smallest local reproduction equivalent to the reported canvas pattern. Prefer a fixture that avoids browser/canvas dependencies:

  ```beanstalk
  can_error |ok Bool| -> String, Error!:
      if ok:
          return "ok"
      ;

      return! Error("boom")
  ;

  value = can_error(true) catch |err|:
      io(err.message)
      assert(false, "unreachable error path")
  ;
  ```

- [ ] Confirm the current diagnostic is `InvalidResultHandlingReason::CatchHandlerCanFallThrough` or its rendered equivalent:

  ```text
  Catch handler without fallback can fall through while success values are required.
  ```

- [ ] Inspect the exact current implementation of:
  - [ ] `src/compiler_frontend/ast/statements/value_production/completeness.rs`
  - [ ] `src/compiler_frontend/ast/statements/value_production/types.rs`
  - [ ] `src/compiler_frontend/ast/statements/fallible_handling/validation.rs`
  - [ ] `src/compiler_frontend/ast/statements/asserts.rs`
  - [ ] `src/compiler_frontend/hir/hir_statement.rs` and the module that defines `lower_assert_statement(...)`
- [ ] Confirm whether HIR already lowers `assert(false)` to an explicit terminator. Record the result in implementation notes.
- [ ] Confirm current docs already state or do not state that `assert(false)` is statically terminal:
  - [ ] `docs/language-overview.md`
  - [ ] `docs/src/docs/progress/#page.bst`, if the progress matrix tracks assertions or value-producing catch behavior.

### Audit / style-guide review / validation

- [ ] No code changes yet except optional local scratch fixtures.
- [ ] Record the failing command and failing diagnostic in the implementation notes.
- [ ] Verify the planned owner remains AST value-production completeness, not HIR or borrow validation.
- [ ] If baseline inspection shows the diagnostic now comes from a different stage, stop and update this plan before coding.

---

## Phase 1 — Implement narrow assert terminality in AST branch-flow analysis

### Context / reasoning

The minimal compiler fix is in `statement_flow(...)`: `NodeKind::Assert` must be classified as `Terminates` only when its condition is statically the literal `false`. A dynamic assertion may pass at runtime, so it must still be treated as `FallsThrough`.

This keeps AST validation aligned with language semantics without introducing a new dependency on constant folding, HIR CFG analysis, or borrow validation.

### Steps

- [ ] Open `src/compiler_frontend/ast/statements/value_production/completeness.rs`.
- [ ] Add the narrow imports needed for expression-kind inspection. Prefer top-level imports; avoid long inline paths.

  Likely shape:

  ```rust
  use crate::compiler_frontend::ast::expressions::expression::{Expression, ExpressionKind};
  ```

- [ ] Add a small helper near `statement_flow(...)`:

  ```rust
  fn assert_condition_is_statically_false(condition: &Expression) -> bool {
      match &condition.kind {
          ExpressionKind::Bool(false) => true,

          ExpressionKind::Coerced { value, .. } => {
              assert_condition_is_statically_false(value)
          }

          _ => false,
      }
  }
  ```

- [ ] Only keep the `Coerced` case if it exists and is a realistic wrapper around boolean literals in this path. If not needed, remove it and keep the helper even narrower.
- [ ] Add a short WHAT/WHY comment above the helper:
  - WHAT: detects only source-level/lowered literal false assertions.
  - WHY: static branch completeness must not prove arbitrary runtime conditions.
- [ ] Add the `NodeKind::Assert` branch in `statement_flow(...)` before the fallback arm:

  ```rust
  NodeKind::Assert { condition, .. } if assert_condition_is_statically_false(condition) => {
      BranchFlow::Terminates
  }
  ```

- [ ] Ensure all other `NodeKind::Assert` values fall through through the normal fallback.
- [ ] Do not change `validate_catch_fallible_handler_value_requirement(...)` in this phase.
- [ ] Do not change `BranchFlow` enum shape in this phase.
- [ ] Do not change `combine_branch_flows(...)` in this phase.

### Audit / style-guide review / validation

- [ ] Check that the helper is short, named clearly, and not clever.
- [ ] Check that no source-language diagnostic moved from `CompilerDiagnostic` into `CompilerError`.
- [ ] Check that no `DataType`-based semantic decisions were introduced.
- [ ] Check that no constant-facts or HIR dependencies were introduced into AST branch completeness.
- [ ] Run targeted formatting and tests after Phase 2 tests are added. If running now, use:

  ```bash
  cargo fmt
  cargo test value_production
  ```

---

## Phase 2 — Add focused unit coverage for value-production branch flow

### Context / reasoning

`value_production_tests.rs` already tests `analyze_branch_flow(...)` directly. This is the fastest regression layer for the bug because it isolates the exact analysis helper that catch validation consumes.

### Steps

- [ ] Open `src/compiler_frontend/ast/statements/tests/value_production_tests.rs`.
- [ ] Add a helper for `assert` nodes, following existing helper style:

  ```rust
  fn assert_statement(condition: Expression, line: i32) -> AstNode {
      node(
          NodeKind::Assert {
              condition,
              message: None,
          },
          test_location(line),
      )
  }
  ```

- [ ] Add a positive test:

  ```rust
  #[test]
  fn branch_flow_reports_assert_false_as_terminal() {
      let flow = analyze_branch_flow(&[
          rvalue(1),
          assert_statement(
              Expression::bool(false, test_location(2), ValueMode::ImmutableOwned),
              2,
          ),
          then_value(3),
      ]);

      assert_eq!(flow, BranchFlow::Terminates);
  }
  ```

- [ ] Add a negative/static-pass test:

  ```rust
  #[test]
  fn branch_flow_does_not_treat_passing_assert_as_terminal() {
      let flow = analyze_branch_flow(&[
          assert_statement(
              Expression::bool(true, test_location(1), ValueMode::ImmutableOwned),
              1,
          ),
      ]);

      assert_eq!(flow, BranchFlow::FallsThrough);
  }
  ```

- [ ] Add a branch test only if it remains readable:
  - [ ] `if condition: assert(false) else assert(false)` should classify as `Terminates`.
  - [ ] `if condition: assert(false)` without `else` should still classify as `FallsThrough`.
- [ ] Do not use production-code-only test utilities inside production files. Keep all new test helpers in the test file.
- [ ] Avoid adding broad expression-proving tests. The intended behavior is literal false only.

### Audit / style-guide review / validation

- [ ] Check test names describe behavior, not implementation details.
- [ ] Check tests are in the existing test module, not production code.
- [ ] Run:

  ```bash
  cargo fmt
  cargo test value_production
  ```

- [ ] Confirm the new positive test fails before Phase 1 and passes after Phase 1.
- [ ] Confirm the dynamic/static-pass test still fails closed as `FallsThrough`.

---

## Phase 3 — Add AST fallible-handling regression tests

### Context / reasoning

The user-facing bug is not just `analyze_branch_flow(...)`; it is catch-handler validation rejecting a value-required catch handler that ends with a statically terminal assertion. Add tests at the fallible-handling parser/AST layer so future refactors do not break the full path from `catch` parsing to value-requirement validation.

### Steps

- [ ] Open `src/compiler_frontend/ast/statements/tests/fallible_handling_tests.rs`.
- [ ] Add a positive test near the existing “Catch handler without fallback (terminating body)” section:

  ```rust
  #[test]
  fn parses_catch_handler_without_fallback_when_handler_ends_with_assert_false() {
      let (ast, string_table) = parse_single_file_ast(
          "can_error |value String| -> String, Error!:\n    return! Error(\"boom\")\n;\n\nrecover |value String| -> String:\n    output = can_error(value) catch |err|:\n        io(err.message)\n        assert(false, \"unreachable error path\")\n    ;\n    return output\n;\n",
      );

      let body = function_body_by_name(&ast, &string_table, "recover");
      // Assert the declaration was accepted and the catch handler body contains NodeKind::Assert.
  }
  ```

- [ ] In the positive test, inspect the parsed declaration similarly to existing catch tests:
  - [ ] Ensure the first statement is the `output` declaration.
  - [ ] Ensure the initializer is `ExpressionKind::ValueBlock { block }`.
  - [ ] Ensure the block is `ValueBlock::Catch`.
  - [ ] Ensure the `FallibleHandling::Handler` body contains a trailing `NodeKind::Assert`.
- [ ] Add a negative test proving dynamic assertions do not satisfy value-required catch completeness:

  ```rust
  #[test]
  fn rejects_catch_handler_without_fallback_when_handler_ends_with_dynamic_assert() {
      assert_invalid_fallible_handling(
          "can_error |value String| -> String, Error!:\n    return! Error(\"boom\")\n;\n\nrecover |value String, should_stop Bool| -> String:\n    return can_error(value) catch |err|:\n        io(err.message)\n        assert(should_stop, \"dynamic assertion can pass\")\n    ;\n;\n",
          InvalidResultHandlingReason::CatchHandlerCanFallThrough,
      );
  }
  ```

- [ ] Add a positive no-binding variant if coverage remains small:

  ```beanstalk
  output = can_error(value) catch:
      assert(false, "unreachable error path")
  ;
  ```

- [ ] Avoid using the canvas package in this AST-level test. The point is fallible catch semantics, not external package resolution.

### Audit / style-guide review / validation

- [ ] Check test snippets are readable and minimal.
- [ ] Check negative tests assert the structured enum reason rather than rendered diagnostic text.
- [ ] Run:

  ```bash
  cargo fmt
  cargo test fallible_handling
  ```

- [ ] Confirm existing fallible-handling tests still pass.
- [ ] Confirm no test uses `panic!` except the existing pattern in tests where destructuring expectations are internal test failures.

---

## Phase 4 — HIR assertion-lowering compatibility audit

### Context / reasoning

The AST validator will now accept handler bodies that end in `assert(false)`. HIR must actually lower that assertion to an explicit terminator, otherwise a later stage may see a fallthrough block that AST promised was terminal.

HIR catch lowering already checks whether the error handler tail block has an explicit terminator before joining the success path at the merge block. That means the assertion lowering path must emit a terminator for statically false assertions.

### Steps

- [ ] Locate `lower_assert_statement(...)`. It is invoked from `NodeKind::Assert { condition, message }` handling in `src/compiler_frontend/hir/hir_statement.rs`.
- [ ] Inspect current behavior for these cases:
  - [ ] `assert(false)`
  - [ ] `assert(false, "message")`
  - [ ] `assert(true)`
  - [ ] `assert(dynamic_bool)`
- [ ] Confirm `assert(false)` emits an explicit HIR terminator, ideally `HirTerminator::AssertFailure { ... }`.
- [ ] Confirm dynamic assertions still preserve a pass continuation path.
- [ ] If `assert(false)` does **not** emit a terminator, update HIR lowering in the existing assertion-lowering owner:
  - [ ] Add a narrow helper equivalent to the AST helper, or share only if the helper can remain stage-appropriate without adding cross-stage coupling.
  - [ ] Emit `HirTerminator::AssertFailure { message }` directly for literal false.
  - [ ] Do not emit a pass/merge block for literal false.
  - [ ] Keep dynamic assertion lowering unchanged.
- [ ] Add or update HIR tests only if HIR needed a code change:
  - [ ] `assert(false)` ends the current block with `AssertFailure`.
  - [ ] `assert(dynamic_bool)` creates/keeps a continuation path.
  - [ ] A catch handler ending in `assert(false)` lowers without the internal error `Catch handler reached HIR fallthrough while a value continuation is required`.

### Audit / style-guide review / validation

- [ ] Confirm HIR changes, if any, use `CompilerError` only for broken HIR invariants, not source-language diagnostics.
- [ ] Confirm assertion source message text handling remains consistent with the existing `AssertMessage` representation.
- [ ] Confirm no backend-specific logic is added to HIR.
- [ ] Run targeted HIR tests if they exist:

  ```bash
  cargo test hir assert
  cargo test catch
  ```

- [ ] If no HIR code changes are needed, record “HIR assertion lowering already emits a terminal block for `assert(false)`” in final notes.

---

## Phase 5 — End-to-end / integration coverage

### Context / reasoning

Unit tests prove the local parser and analysis behavior. Add at least one integration/compiler-test fixture to ensure the normal pipeline accepts the real user-facing pattern through AST, HIR, borrow validation, and backend lowering where the harness supports it.

### Steps

- [ ] Locate the current integration test fixture root. Follow the existing repository layout and manifest style; do not invent a parallel test runner.
- [ ] Add a success fixture with a minimal Beanstalk program equivalent to the original blocker:

  ```beanstalk
  can_error |ok Bool| -> String, Error!:
      if ok:
          return "ok"
      ;

      return! Error("boom")
  ;

  value = can_error(true) catch |err|:
      io(err.message)
      assert(false, "unreachable error path")
  ;

  [:
      [value]
  ]
  ```

- [ ] Prefer a compile/build success assertion over exact generated output unless the harness already has stable output goldens for similar cases.
- [ ] If the integration harness supports backend-specific matrix entries, enable at least the default HTML backend and any Wasm backend that currently handles assertions.
- [ ] Add a failure fixture only if the integration suite has clear conventions for diagnostic-code assertions:

  ```beanstalk
  can_error |ok Bool| -> String, Error!:
      if ok:
          return "ok"
      ;

      return! Error("boom")
  ;

  value = can_error(true) catch |err|:
      assert(err.code > 0, "dynamic assertion can pass")
  ;
  ```

  Expected reason: `CatchHandlerCanFallThrough` / the stable diagnostic code associated with invalid result handling.
- [ ] Do not use `@web/canvas` in the integration fixture unless the existing suite already has stable browser/canvas external-package fixtures. Keep this test focused on language semantics.
- [ ] Optionally add a separate external-package smoke fixture later for the original canvas program, but do not couple this compiler correctness regression to canvas availability.

### Audit / style-guide review / validation

- [ ] Check the fixture follows existing directory, manifest, warning, and golden conventions.
- [ ] Check failure expectations use stable diagnostic codes where available, not fragile rendered prose.
- [ ] Run the integration test runner:

  ```bash
  cargo run -- tests
  ```

- [ ] If the harness requires backend artifacts, verify generated outputs are stable and do not include incidental local paths.

---

## Phase 6 — Documentation and plan-change review

### Context / reasoning

Language documentation should already describe `assert(false)` as statically terminal. This phase ensures implementation and docs agree, and it prevents silent scope creep if the coding agent discovers adjacent branch-flow gaps.

### Steps

- [ ] Check `docs/language-overview.md` for assertion semantics.
  - [ ] If it already states that `assert(false)` is statically terminal and dynamic assertions are not, do not duplicate text.
  - [ ] If it does not, add or update the assertion section with:

    ```markdown
    - `assert(false)` and `assert(false, "message")` are statically terminal and may end a non-`Void` function or value-required catch handler.
    - Dynamic `assert(condition)` is not statically terminal because the pass path continues normally.
    ```

- [ ] Check whether `docs/compiler-design-overview.md` needs no change. Expected result: no change, because compiler stage ownership already places this in AST construction / HIR lowering.
- [ ] Check whether `docs/src/docs/progress/#page.bst` tracks assertions, value-producing blocks, or catch handling.
  - [ ] If yes, update the status/progress note to mention static terminal assert support in value-required catch handlers.
  - [ ] If no, record “no progress-doc row for this behavior” in final notes.
- [ ] If Phase 4 required HIR changes, add a short comment in the HIR assertion-lowering helper explaining why literal false is lowered as direct failure.
- [ ] If the implementation scope expanded beyond literal `assert(false)`, update this plan before final validation and list the new scope in a “Plan changes” section.

### Audit / style-guide review / validation

- [ ] Confirm docs describe user-facing behavior only and do not leak AST/HIR internals into language docs.
- [ ] Confirm compiler-design docs are changed only if stage ownership or pipeline contract changed.
- [ ] Confirm comments added to code explain behavior and rationale rather than restating syntax.
- [ ] Run markdown/style checks if the repository has them. Otherwise include docs in the final `just validate` run.

---

## Phase 7 — Full validation and final review

### Context / reasoning

This phase catches accidental regressions outside the narrow test set and validates the change against the project’s required workflow.

### Steps

- [ ] Run the full validation command:

  ```bash
  just validate
  ```

- [ ] If `just validate` is not available in the execution environment, run the component commands manually:

  ```bash
  cargo clippy
  cargo test
  cargo run -- tests
  ```

- [ ] Run the original local reproduction again.
- [ ] If practical, run the original canvas snippet once external package setup is available.
- [ ] Inspect `git diff` for accidental broad changes.
- [ ] Confirm no generated or local build artifacts are committed unless the integration harness requires updated goldens.
- [ ] Confirm all changed code follows the style guide:
  - [ ] clear names
  - [ ] no unnecessary inline imports
  - [ ] no user-input panics
  - [ ] readable match arms
  - [ ] no clever boolean-heavy API changes
  - [ ] no stale comments
  - [ ] tests live outside production code
- [ ] Confirm every changed compiler stage still respects diagnostic ownership:
  - [ ] AST/user-facing validation uses `CompilerDiagnostic`.
  - [ ] HIR invariant failures use `CompilerError`.
  - [ ] No source-language error was newly routed through an infrastructure error.

### Final implementation notes checklist

- [ ] List changed source files.
- [ ] List changed test files.
- [ ] List changed docs files, or state “no docs changes required; existing docs already specified this behavior.”
- [ ] Include validation commands and pass/fail result.
- [ ] Mention whether HIR assertion lowering required code changes.
- [ ] Mention any deferred follow-up discovered during implementation.

---

## Explicit non-goals for this implementation

- [ ] Do not implement general constant-condition proof for `assert(1 is 2)` or `assert(SOME_FALSE_CONST)`.
- [ ] Do not make `assert` an expression.
- [ ] Do not make assertion failure catchable.
- [ ] Do not alter `Error!` / `catch` syntax.
- [ ] Do not change canvas external package behavior.
- [ ] Do not move catch completeness validation from AST to HIR.
- [ ] Do not introduce backend-specific assertion behavior.

---

## Follow-up candidate: branch-flow lattice precision

This is not required to unblock the reported canvas pattern, but the current `BranchFlow` model is likely over-conservative for mixed complete branches.

Current behavior likely treats a branch/match where one path `then`s and another path `return`s as `FallsThrough`, even though no path actually falls through. Existing tests may currently assert this behavior. Before changing it, make an explicit design decision.

Possible follow-up shape:

- [ ] Add a fourth branch-flow state, such as `ProducesOrTerminates`, or replace `BranchFlow` with a clearer lattice that separately tracks:
  - [ ] `can_fall_through`
  - [ ] `can_produce_value`
  - [ ] `can_terminate`
- [ ] Update `combine_branch_flows(...)` so mixed `ProducesValue` / `Terminates` paths are accepted when no path can fall through.
- [ ] Update tests that currently expect mixed produce/terminate paths to be rejected.
- [ ] Audit HIR value-block lowering before enabling this broadly, because all-terminating and mixed-terminating value blocks can create unreachable merge blocks if lowering is not careful.
- [ ] Keep this as a separate change unless the current implementation requires it.

