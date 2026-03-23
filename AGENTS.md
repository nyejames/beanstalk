# Beanstalk agent guidance

Primary project references:
- docs/codebase-style-guide.md
- docs/compiler-design-overview.md
- docs/language-overview.md
- docs/memory-management-design.md

Instruction priority:
1. Explicit user request for the current task
2. The project references listed above
3. README.md for a brief high-level overview of the language, current development status, and available tooling
4. Existing code, unless the task is specifically to inspect current implementation behavior

Follow the project references by default.
If the task explicitly changes a feature or design decision, follow the task and state that it supersedes the current docs for that task.

If code or README.md conflicts with the project references, call out the conflict explicitly.
If the project references conflict with each other, prefer the most specific and task-relevant document and state the conflict.

Task routing:
- Style, code quality, reviews, and refactors: docs/codebase-style-guide.md
- Compiler architecture and subsystem boundaries: docs/compiler-design-overview.md
- Language syntax and semantics: docs/language-overview.md
- Ownership, allocation, regions, and memory model: docs/memory-management-design.md

## Design and documentation discipline

Before making non-trivial code changes, identify which project references are relevant to the task.

When reviewing, planning, or proposing implementation changes, align with the relevant project references by default.
Do not invent behavior, APIs, or design decisions that contradict them unless the task explicitly requests a new or revised design.

If the implementation conflicts with the relevant project references, call out the conflict explicitly.

Do not modify documentation files unless the user explicitly requests documentation changes, or explicitly approves them after you identify that documentation should be updated.

If a code or design change would require documentation updates, state that clearly and ask permission before editing the docs unless documentation changes were explicitly requested as part of the task.