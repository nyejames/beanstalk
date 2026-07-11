# Beanstalk agent rules

Resolve every relative path in this file from the current worktree root. Do not read project references from another worktree unless the user explicitly asks you to.

Before any Beanstalk task, read:
- this file
- `docs/src/docs/codebase/overview.bd`
- `docs/src/docs/codebase/style-guide/style-guide.bd`
- `docs/src/docs/codebase/memory-management/overview.bd`

Before making or reviewing a non-trivial change, read:
- `docs/src/docs/codebase/style-guide/validation.bd`

Read `docs/src/docs/codebase/style-guide/testing.bd` when the task changes or reviews behavior, diagnostics, compiler stages, backend artifacts, tests, fixtures, or test infrastructure.

For compiler, build-system, analysis, HIR, or backend work:
1. Read `docs/src/docs/codebase/compiler-design/overview.bd`
2. Use its task-reading matrix
3. Read the selected leaf documents before changing implementation

For memory, ownership, borrow checking, allocation, GC, drops, or runtime-handle work:
1. Read `docs/src/docs/codebase/memory-management/overview.bd`
2. Use its task-reading guide
3. Read the selected memory leaf documents

For language syntax, semantics, and user-visible behavior, read:
- `docs/language-overview.md`
- the relevant user-facing pages under `docs/src/docs/**`

`docs/language-overview.md` is the remaining monolithic source of truth. Compiler architecture, memory design, code style, testing, and validation are owned by the Beandown documents under `docs/src/docs/codebase/**`.

Use:
- `docs/src/docs/progress/#page.bst` for current implementation status and coverage
- `docs/roadmap/roadmap.md` for sequencing, active plans, and unaccepted proposals
- `index.md` only as a file and module locator

## Instruction priority

1. The explicit user request for the current task
2. The most specific relevant design or standards document
3. This file
4. Existing implementation behavior

A narrow leaf document takes precedence over a broad overview within the same documentation area.

Code may lag accepted design. When implementation conflicts with the relevant design document, call out the conflict rather than silently treating the code as authoritative.

The progress matrix answers what works today. It does not override accepted architecture or language semantics.

## Core working rules

- Prefer readability, modularity, correctness, and structured diagnostics over cleverness. Avoid complexity.
- Maintain strict boundaries between build-system, frontend, AST, HIR, analysis, project-builder, and backend responsibilities.
- Avoid user-input panics. User failures use structured diagnostics; panic paths are only for proven internal compiler invariants.
- Beanstalk is pre-release. Do not preserve old APIs through compatibility wrappers, forwarding shims, parallel structs, or legacy entry points.
- Prefer one current implementation path. Extend, consolidate, replace, or delete existing paths instead of adding parallel systems. 
- When an API shape changes, thread the new shape through the compiler and remove the old one. 
- Be strict about making root-cause fixes over patches. Never leave code that will need refactoring or cleaning up later.
- Write beautiful code that uses descriptive names, explicit control flow, narrow helpers, context structs, and concise WHAT/WHY comments.
- Remove dead code, obsolete helpers, stale comments, duplicate paths, and superseded fixtures as part of the owning change.
- Be strict about design drift, duplicated implementation paths, weak diagnostics, oversized modules, stale helpers, and stage-boundary leaks.
- Do not move shared logic into a broad utility module unless the behavior is genuinely shared and the owner remains clear.
- Do not claim work was validated by commands that were not run.

## Required workflow

Every non-trivial implementation plan must end with the Final audit below.

For multi-phase work, briefly re-check ownership, duplication, stale paths and
test gaps after each completed phase.

1. Identify and read the relevant documentation.
2. Inspect the current implementation and its existing owner.
3. Search for overlapping helpers, validators, lowering paths, diagnostics, tests, and legacy implementations.
4. Decide whether the task extends, consolidates, replaces, or removes an existing path.
5. Implement the smallest coherent slice without leaving transitional duplication.
6. Add or update tests according to `style-guide/testing.bd` when behavior or internal invariants changed.
7. Review the progress matrix when support, rejection, backend coverage, or test coverage changed.
8. Apply the correct final gate from `style-guide/validation.bd`.
9. Perform the final audit below.

If a user request changes accepted behavior, treat the request as authoritative for that task and update the relevant design/status documentation when documentation changes are in scope. Call out any implementation conflict explicitly.

## Duplication and abstraction policy

Be strict about avoiding duplicated logic. Prefer extending, consolidating, or replacing the existing owner of the behavior over adding a new module, system, or parallel path. Only add a new subsystem when the existing ownership is clearly wrong or the new behavior is genuinely separate.

Before adding a helper, pass, type, registry, validator, or module:
- check for an existing owner
- check adjacent stages and backend paths for near-duplicate logic
- prefer extending or restructuring the current owner
- extract shared code only when the behavior is genuinely identical and the abstraction has a clear home

When similar logic remains separate, state why the similarity is superficial or why sharing would blur ownership.

Actively look for duplicated:
- validation
- diagnostic construction
- type and coercion logic
- template handling
- control-flow lowering
- backend lowering
- test fixtures and assertions

## Testing

Follow `docs/src/docs/codebase/style-guide/testing.bd`.

Key routing:
- prefer integration cases under `tests/cases/` for user-visible language behavior
- use focused unit tests only for subsystem-local invariants or side-table facts
  that integration output can't expose
- use backend-specific artifact assertions or contractual goldens for backend structure
- use one input with backend-specific expectations for cross-backend parity
- don't use benchmark fixtures as correctness coverage

## Validation

Always follow `docs/src/docs/codebase/style-guide/validation.bd`

## Documentation policy

Do not modify documentation unless the user explicitly requests documentation
changes or explicitly approves them after they are identified. 

The progress matrix is the standing exception. Update it when implementation
status, rejection behavior, backend coverage, or test coverage changes. Do not
edit it for a pure refactor or prose-only correction.

If implementation work makes documentation inaccurate, report the affected files and required corrections as a separate follow-up. Do not edit generated files under `docs/release/**` directly, rebuild it through the compiler.

- Codebase design documents may describe accepted end-state architecture that has not fully landed.
- The progress matrix records current support, partial support, clean rejection, experimental paths, and coverage.
- The roadmap records sequencing, active plans, and proposals not yet accepted as design.
- Update the progress matrix when current status changed. Do not make a meaningless matrix edit for a pure refactor or prose-only correction.
- Put architecture in the narrowest relevant codebase page.

## Benchmarking

- Use `just bench-check` for non-recording performance evidence
- Use `just bench` only when intentionally recording benchmark history
- Keep raw profiling and benchmark data local
- Treat profiling as attribution evidence, not proof of correctness or improvement

## Context recovery

If context was compacted, reset or may be incomplete, always re-read:

1. This file
2. The codebase overview
3. The style guide and validation gates
4. The testing standards when behavior or tests are in scope
5. The compiler or memory overview when relevant
6. The task-selected leaf documents
7. `docs/language-overview.md` when language semantics are involved
8. The progress matrix when implementation status or coverage is in scope
9. The current plan
10. The current implementation and diff

Do not continue implementation from compressed memory alone.

## Final audit

Before reporting a non-trivial slice complete or reviewing changes, verify:
- the relevant style, compiler, memory, and language contracts are respected.
- stage and subsystem ownership remain clear.
- no duplicated, legacy or obsolete implementation path remains.
- there is no unnecessary indirection, weak diagnostics, poor comments, or missing test coverage.
- there are no abstractions that are too broad, too early, or placed in the wrong layer
- diagnostics use the correct lane and preserve useful source context.
- tests protect behavior or real internal invariants rather than implementation accidents.
- the progress matrix accurately reflects changed support.
- documentation and comments name the current owner and behavior.
- the correct validation path was run.
- the final report states exactly what was and was not validated.
