## Cross-platform consistency and test stability

### PR - Finish CRLF normalization in strings and templates

Remove avoidable Windows/macOS golden drift from source normalization and emitted outputs.

**Checklist**
- Audit remaining CRLF behavior in strings, templates, and emitted output.
- Make sure normalized newline handling is consistent through the frontend and builder outputs.
- Add regression tests specifically for Windows-shaped input.

**Done when**
- Golden outputs are stable across normal Windows/macOS workflows.

**Done when**
- Non-semantic generator-shape churn no longer causes broad golden failures.
- Semantic changes still fail with clear, targeted integration diffs.

### PR - Add rendered-output assertions for runtime-fragment semantics

Some integration behaviors are fundamentally about rendered output, not emitted JS text layout.
For runtime-fragment-heavy cases, asserting rendered slot output provides stronger semantic confidence
than snapshotting compiler-generated temporary symbols.

**Fits with other PRs**
- Builds on the normalized-assertion work above.
- Supports the Phase 6 JS backend semantic audit with behavior-first checks.

**Checklist**
- Add an optional integration assertion mode that executes generated HTML+JS in a deterministic test harness and compares rendered runtime-slot output.
- Keep this mode focused on semantic surfaces (runtime fragments, call/lowering paths, collection/read flows) where emitted-text snapshots are noisy.
- Ensure harness failures distinguish:
  - test harness limitations/infrastructure errors
  - actual rendered-output mismatches
- Add targeted cases that currently rely on brittle full-file goldens but are really asserting rendered text behavior.
- Document expectation-writing guidance so new cases choose rendered assertions when appropriate.

**Done when**
- Runtime-fragment semantics are asserted directly at rendered-output level where needed.
- Integration failures are lower-noise and more actionable during backend/lowering changes.

### PR - Fix remaining Windows test-runner stability issues

Remove test-runner and lock-poisoning rough edges that still make Windows less reliable.

**Checklist**
- Audit known lock poisoning paths and test-runner failure behavior.
- Ensure failed tests/builds do not leave the runner in a poisoned or misleading state.
- Add targeted tests where possible.

**Done when**
- Windows failures look like normal compiler/test failures, not infrastructure weirdness.
