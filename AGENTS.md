# Beanstalk agent guidance

Primary project references:
- docs/codebase-style-guide.md
- docs/compiler-design-overview.md
- docs/language-overview.md
- docs/memory-management-design.md

Instruction priority:
1. Explicit user request for the current task
2. The relevant project references
3. README.md for a brief high-level overview of the language, current development status, and available tooling
4. Existing code, unless the task is specifically to inspect current implementation behavior

Always follow the style guide by default.

Before making non-trivial code changes, identify which other project references are relevant to the task:

For architecture, subsystem boundaries, lowering, IR design, and compiler structure,
consult `docs/compiler-design-overview.md` when relevant.

For language syntax, semantics, and user-facing behavior,
consult `docs/language-overview.md` when relevant.

For ownership, allocation, regions, GC assumptions, and memory model questions,
consult `docs/memory-management-design.md` when relevant.

When reviewing, planning, or proposing implementation changes, align with the relevant project references by default.
Do not invent behavior, APIs, or design decisions that contradict them unless the task explicitly requests a new or revised design.

If the task explicitly changes a feature or design decision, follow the task and state that it supersedes the current docs for that task.
If code or README.md conflicts with the project references, call out the conflict explicitly.
If the project references conflict with each other, prefer the most specific and task-relevant document and state the conflict.

Do not modify documentation files unless the user explicitly requests documentation changes, or explicitly approves them after you identify that documentation should be updated.