# Beanstalk agent guidance

Primary project references:
- docs/codebase-style-guide.md
- docs/compiler-design-overview.md
- docs/language-overview.md
- docs/memory-management-design.md

Instruction priority:
1. Explicit user request for the current task
2. The relevant project references
3. This AGENTS.md file
4. README.md for brief project overview, current status, and tooling
5. Existing code, unless the task is specifically to inspect current implementation behavior

The project references are the canonical source of truth for intended compiler and language behavior.
If code or README.md conflicts with them, call out the conflict explicitly.
If the project references conflict with each other, prefer the most specific and task-relevant document and state the conflict.

Always follow the style guide by default.

## Core working rules

- Prefer readability, modularity, correctness, and diagnostics over cleverness.
- Maintain clear boundaries between compiler stages. Do not mix frontend, analysis, IR, and backend concerns casually.
- Avoid user-input panics. Do not introduce `panic!`, `todo!`, or user-data-driven `.unwrap()` / `.expect()` in active compiler paths.
- Panic paths are only acceptable for proven internal invariants that indicate a compiler bug.
- Beanstalk is pre-alpha. Do not preserve old APIs through compatibility wrappers, forwarding shims, parallel structs, or duplicated legacy entry points.
- When an API shape changes, thread the new shape through the compiler and remove the old one.

## Duplication and abstraction policy

Be strict about avoiding duplicated logic.

Before adding a new helper, function, pass, or type:
- Check whether similar logic already exists nearby or elsewhere in the same stage.
- Check whether the new logic is actually a variant of an existing responsibility.
- Prefer extending or consolidating an existing implementation when that keeps ownership and responsibility clear.

When similar code exists in two or more places:
- Do not leave near-duplicate logic in place without a reason.
- Decide whether the shared logic should:
  1. stay local because the similarities are superficial,
  2. be extracted into a shared helper within the same subsystem,
  3. be moved to a more central module both callers depend on,
  4. or be removed by restructuring the flow so duplication disappears entirely.

Do not create “utility” abstractions prematurely.
Only extract shared code when:
- the behavior is genuinely the same,
- the abstraction has a clear owner,
- the name is more readable than the duplicated code,
- and the extraction does not blur compiler stage boundaries.

When reviewing or refactoring, actively look for:
- duplicated validation logic,
- duplicated lowering logic,
- duplicated diagnostic construction,
- duplicated type-checking/coercion paths,
- duplicated template handling paths,
- duplicated collection or control-flow lowering,
- duplicated test fixtures that no longer prove distinct behavior.

If duplication remains intentionally, state why it is staying local and why abstraction would be worse.

## Required workflow for non-trivial tasks

Before making non-trivial changes:
1. Identify which project references are relevant.
2. Read the relevant docs first.
3. Check whether the code already has an existing implementation path for the behavior.
4. Check whether similar logic already exists elsewhere that should be reused or consolidated.
5. If the task changes intended behavior, treat the user request as superseding current docs for that task and state that clearly.

For architecture, subsystem boundaries, lowering, IR design, and compiler structure,
consult `docs/compiler-design-overview.md`.

For language syntax, semantics, and user-facing behavior,
consult `docs/language-overview.md`.

For ownership, allocation, regions, GC assumptions, and memory model questions,
consult `docs/memory-management-design.md`.

For code organization, naming, diagnostics, testing expectations, comments, and refactor standards,
consult `docs/codebase-style-guide.md`.

## Implementation expectations

- Keep modules focused on one responsibility.
- Prefer context structs over threading large argument lists through many functions.
- Use descriptive names. Avoid unnecessary abbreviations.
- Add concise WHAT/WHY comments for non-obvious logic, invariants, control-flow joins, and subtle behavior.
- Do not add comments that merely restate syntax.
- Prefer one current implementation path, not parallel paths during refactors.
- Remove dead code, obsolete helpers, stale branches, and old scaffolding as part of the refactor when practical.
- Keep new abstractions small, stage-local, and easy to justify.
- Do not move shared logic upward into a broad common module unless both call sites genuinely depend on the same behavior and ownership is clear.

## Diagnostics and error handling

- Use structured diagnostics, not ad-hoc strings, where the compiler already has established helpers.
- Preserve source locations and stage-appropriate error categories.
- Error messages should be specific, actionable, and consistent with the style guide.
- If adding new validation behavior, ensure the failure mode is intentional and well explained.

## Testing and validation

Before finishing code changes, always run:
- `cargo clippy`
- `cargo test`
- `cargo run tests`

When adding or changing behavior:
- Prefer integration coverage for user-visible language behavior.
- Add or update unit tests only where subsystem-local behavior genuinely benefits from them.
- Avoid redundant fixtures that assert the same behavior in slightly different forms.
- Prefer meaningful end-to-end cases over brittle implementation-shaped assertions.
- When backend/runtime behavior matters, strengthen artifact assertions where goldens alone are too vague.

## Review and audit expectations

When reviewing, planning, or proposing implementation changes:
- Align with the relevant project references by default.
- Identify design drift, unnecessary indirection, duplicated logic, legacy codepaths, weak diagnostics, poor comments, and missing test coverage.
- Call out functions or helpers that appear semantically overlapping and decide whether they should be unified, relocated, or intentionally kept separate.
- Flag abstractions that are too broad, too early, or placed in the wrong layer.
- Flag stage-boundary leaks explicitly.

## Documentation update policy

Do not modify documentation files unless the user explicitly requests documentation changes, or explicitly approves them after you identify that documentation should be updated.

If implementation changes make the docs inaccurate:
- say so clearly,
- identify which document is now stale,
- and treat documentation follow-up as a separate explicit task unless the user asks for it.