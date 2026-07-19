# Beanstalk educational compiler-design docs outline production plan

## Purpose

Create the first structural outline for the educational compiler-design documentation under:

```text
docs/src/docs/codebase/compiler-design/
```

The finished outline must guide a later writing pass. It must not contain full article prose yet.

The documentation will teach readers who know Rust but may know little about compilers. It will follow one Beanstalk project from source files to a generated HTML page. Along the way it will explain common compiler terms, Beanstalk's compiler architecture and the language choices that make Beanstalk different from mainstream Rust, JavaScript, TypeScript or C-family designs.

The educational pages do not define compiler behaviour. They explain accepted architecture from the canonical design documents and show how that architecture fits together.

## Required result

The agent following this plan must produce:

1. A revised master outline for `docs/src/docs/codebase/compiler-design/overview.bd`.
2. A chronological page tree for the full educational series.
3. One outline-only `.bd` file for every article in the page tree.
4. A matching `#page.bst` route wrapper plan for every article.
5. A migration map from the current area-based pages to the new chronological pages.
6. A visual plan for every article.
7. A roadmap and implementation-status audit for every article.
8. A final cross-page review showing that terminology, example state and stage ownership remain consistent.

Do not draft complete paragraphs, final analogies or polished article text during this task. Headings, teaching notes, diagram briefs, code checkpoints and transition notes count as outline material.

## Authority and required reading

### Read before changing any outline

Read these files in this order:

1. `AGENTS.md`
2. `docs/compiler-design-overview.md`
3. `docs/build-system-design.md`
4. `docs/src/docs/codebase/style-guide/style-guide.bd`
5. `docs/src/docs/codebase/style-guide/validation.bd`
6. `docs/src/docs/codebase/memory-management/overview.bd`
7. The memory leaf documents selected by that overview:
   - `access-and-aliasing/access-and-aliasing.bd`
   - `borrow-validation/borrow-validation.bd`
   - `ownership-and-drops/ownership-and-drops.bd`
   - `runtime-and-backend-lowering/runtime-and-backend-lowering.bd`
8. `docs/src/docs/codebase/language/overview.bd`
9. `docs/language-overview.md` for language areas not yet migrated
10. `docs/src/docs/progress/#page.bst`
11. `docs/roadmap/roadmap.md`
12. Every active or queued roadmap plan that affects a page in this series, including:
    - `compiler-diagnostics-improvement-plan.md`
    - `canonical-module-compilation-and-scoped-packages-plan.md`
    - `import_values_anonymous_records_plan.md`
    - `entry-config-blocks-runtime-title-plan.md`
    - `number_type_numeric_plan.md`
    - `html_project_backend_wasm_final_implementation_plan.md`
    - `post-tir-template-parser-optimization-plan.md` when discussing future template performance
13. The current pages under `docs/src/docs/codebase/compiler-design/**`
14. The repository `README.md`
15. The supplied `informative-writer` skill
16. The supplied `tech-article.md` writing references

Read current implementation files only after the design and status documents. Use them to link readers to real owners and to explain current examples. Do not let transitional code override accepted architecture.

### Authority order

Use this order whenever sources disagree:

1. The user's explicit direction for this documentation project
2. The narrow canonical design document for the topic
3. `docs/compiler-design-overview.md` or `docs/build-system-design.md`
4. The language and memory authorities
5. `AGENTS.md` and the style guides
6. The progress matrix for current support
7. The roadmap for implementation order and deferred work
8. Current implementation code
9. Existing educational pages

Roadmap plans cannot override canonical architecture. Current code may lag accepted design. Call out that gap instead of teaching the code as the final model.

## Roadmap-aware writing rules

The series must teach accepted end-state architecture as its main path. It must also tell the reader when that architecture has not landed yet.

Use four clear status classes:

- **Accepted design:** The canonical architecture or language contract, including work that has not landed yet.
- **Current implementation:** Behaviour supported by the current progress matrix and current code.
- **Queued or active implementation:** Work covered by a named roadmap plan.
- **Deferred or outside scope:** Work that the roadmap defers or the language design intentionally rejects.

Apply these rules:

- Do not present a queued plan as implemented.
- Do not present an experimental implementation as the accepted final shape.
- Do not teach rejected transitional paths as stable concepts.
- Do not copy the progress matrix into the articles. Link to it and add a short status note only where the difference affects understanding.
- Recheck the progress matrix and roadmap immediately before drafting each final article. Plans may land between the outline and writing passes.
- Prefer accepted terms such as canonical module graph, immutable module artefact, `ProjectCompilation`, per-function link facts and entry-specific target assignment.
- Avoid anchoring the tutorial to transitional paths such as flat `Vec<Module>` backend handoff, whole-module JavaScript or Wasm selection, `bst_start`, per-module Wasm memories or the old flat config model.
- When accepted syntax has not landed, keep runnable examples current-valid and show accepted future syntax in a separately labelled callout. Never mix the two silently.

## Audience and teaching stance

Assume the reader:

- writes Rust comfortably
- understands functions, enums, structs, traits and ownership at a user level
- may not know compiler terminology
- may not know build graphs, intermediate representations, data-flow analysis, linking or code generation
- wants to understand both the general computer-science pattern and Beanstalk's implementation choices

Teach each new term when it first becomes useful. Do not begin with a glossary dump.

Use this explanatory order for major concepts:

1. The problem a compiler or build tool needs to solve
2. The common compiler pattern used to solve it
3. The form Beanstalk chooses
4. The reason Beanstalk chooses that form
5. The running example passing through that boundary
6. The data produced for the next stage

Avoid presenting architecture as a list of Rust types. Show data movement first, then link to the types and modules that implement it.

## The running example

Use one small project through the whole series.

```text
compiler_walkthrough/
├── config.bst
└── src/
    ├── #page.bst
    └── greeting.bst
```

The example should contain these ideas without growing into a demo application:

- one project config
- one normal module
- one ordinary source file
- one imported function
- one compile-time string constant
- one runtime value
- one function call
- one HTML template fragment
- enough runtime work to produce a JavaScript or mixed JavaScript and Wasm path

Working source shape:

```beanstalk
-- greeting.bst

greeting_prefix #= "Hello"

greet |name String| -> String:
    return [: [greeting_prefix], [name]!]
;
```

```beanstalk
-- #page.bst

import @greeting { greet }

visitor = "Priya"

[$html:
    <p>[greet(visitor)]</p>
]
```

The final relevant HTML fragment must remain:

```html
<p>Hello, Priya!</p>
```

Treat the exact surrounding document shell, script tags and generated filenames as article-specific details. The core result above must not change during the series.

### Example state ledger

Each outline must state what representation of the example enters the article and what leaves it.

Track these checkpoints:

```text
project tree
-> selected command and config values
-> canonical source graph
-> located tokens
-> prepared declaration and import shells
-> bound interfaces and ordered declarations
-> typed AST and semantic identities
-> folded constants, generated requests and finalised TIR handoff
-> structured diagnostics where source fails
-> validated HIR
-> borrow and ownership side-table facts
-> immutable module artefacts and fingerprints
-> entry assembly and reachable function union
-> deterministic target assignment
-> JavaScript and Wasm backend outputs
-> HTML document, runtime assets and owned output manifest
```

Every page must continue from the previous checkpoint. Do not restart the explanation from raw source unless the article needs to compare representations.

### Side examples

Use small side examples only when the main greeting project cannot demonstrate a Beanstalk-specific rule cleanly. Suitable side examples include:

- `copy` versus shared access
- `~place` for exclusive mutable access
- `Error!`, postfix `!` and `catch`
- a static trait bound
- a choice used instead of a trait object
- a `.bd` source asset
- a reactive source and template subscription
- a target-gated external function

Label every side example and return to the greeting project after it. Do not let a side example replace the main thread.

## Beanstalk-specific design coverage

The series must explain unusual choices rather than treating them as syntax trivia.

Introduce these choices in the early design article, then revisit them at the stage that owns them:

- a language and build system designed together
- source-backed Builder packages and binding-backed platform packages
- `#*.bst` normal module roots and `+*.bst` scoped support roots
- one canonical module compilation rather than entry-specific recompilation
- no general macro system
- templates as first-class language values with AST-local TIR
- `.bd` and `.md` source kinds lowered into ordinary synthetic declarations
- strict no-shadowing and one visible-name collision policy
- shared read-only access by default
- explicit `copy`
- explicit `~place` for exclusive access
- inferred moves with no move keyword
- no source-visible references or lifetime syntax
- GC-backed correctness plus optional ownership-aware lowering
- static traits without trait objects, inheritance, blanket implementations or associated-type machinery
- choices for runtime heterogeneity
- compiler-owned operators and casts
- `Error!`, postfix `!` and `catch` instead of first-class public `Result` values
- a backend-neutral frontend
- per-function mixed JavaScript and Wasm target assignment
- build-system-owned output writing and manifests

Every relevant article should include a section or callout titled **Why Beanstalk does it this way**. Compare the choice with one common alternative, state the tradeoff and avoid marketing language.

Do not imply that every unusual choice offers a universal improvement. Explain what complexity Beanstalk removes, what flexibility it gives up and why that trade fits the project's goals.

## Main reading order

The main path must end at generated artefacts and files on disk. No glossary, diagnostics or incremental-build article may follow the final output article in the primary sequence.

```text
00. Overview: source to page
01. What a compiler does
02. Why Beanstalk makes different choices
03. Starting a build
04. Project structure, modules and graphs
05. Tokenization
06. Declarations, imports and ordering
07. AST semantics and type identity
08. Compile-time semantics and static evidence
09. Templates, TIR and reactivity
10. Diagnostics as compiler data
11. HIR and explicit control flow
12. Automatic memory management and GC
13. Borrow validation, inferred ownership and drops
14. Module artefacts, reuse and deterministic compilation
15. Linking, entry activation and reachability
16. Target planning and validation
17. JavaScript and Wasm lowering
18. HTML assembly and output ownership
```

## Proposed file tree

```text
docs/src/docs/codebase/compiler-design/
├── #page.bst
├── overview.bd
│
├── what-a-compiler-does/
│   ├── #page.bst
│   └── what-a-compiler-does.bd
│
├── beanstalk-design-choices/
│   ├── #page.bst
│   └── beanstalk-design-choices.bd
│
├── starting-a-build/
│   ├── #page.bst
│   └── starting-a-build.bd
│
├── project-graphs-and-modules/
│   ├── #page.bst
│   └── project-graphs-and-modules.bd
│
├── tokenization/
│   ├── #page.bst
│   └── tokenization.bd
│
├── declarations-imports-and-ordering/
│   ├── #page.bst
│   └── declarations-imports-and-ordering.bd
│
├── ast-semantics-and-types/
│   ├── #page.bst
│   └── ast-semantics-and-types.bd
│
├── compile-time-semantics/
│   ├── #page.bst
│   └── compile-time-semantics.bd
│
├── templates-and-tir/
│   ├── #page.bst
│   └── templates-and-tir.bd
│
├── diagnostics/
│   ├── #page.bst
│   └── diagnostics.bd
│
├── hir/
│   ├── #page.bst
│   └── hir.bd
│
├── memory-management-and-gc/
│   ├── #page.bst
│   └── memory-management-and-gc.bd
│
├── borrow-validation-and-drops/
│   ├── #page.bst
│   └── borrow-validation-and-drops.bd
│
├── module-artefacts-and-reuse/
│   ├── #page.bst
│   └── module-artefacts-and-reuse.bd
│
├── linking-entries-and-reachability/
│   ├── #page.bst
│   └── linking-entries-and-reachability.bd
│
├── target-planning-and-validation/
│   ├── #page.bst
│   └── target-planning-and-validation.bd
│
├── backend-lowering/
│   ├── #page.bst
│   └── backend-lowering.bd
│
└── html-assembly-and-output/
    ├── #page.bst
    └── html-assembly-and-output.bd
```

Keep semantic names in paths. Do not prefix directories with numbers. The landing page, previous and next links define the reading order.

## Standard article shape

Each main article should target a 10 to 15 minute read during the later writing pass. Plan for roughly 1,800 to 2,700 words depending on the topic. The overview may remain shorter.

Each `.bd` outline must include these planning sections:

```markdown
# Working title

## Reader promise
- What the reader will understand by the end

## Entry state
- Representation received from the previous article
- Terms the reader already knows

## Opening problem
- The concrete question or failure this stage solves
- The running-example moment that exposes the problem

## Section sequence
### Proposed section heading
- General compiler concept
- Beanstalk ownership and design choice
- Running-example checkpoint
- Important terminology
- Source or authority links

## What can fail here
- User-facing failures owned by this stage
- Internal invariant failures, if useful
- Do not duplicate the diagnostics article

## Why Beanstalk does it this way
- Main alternative
- Beanstalk's choice
- Benefit
- Cost or lost flexibility

## Roadmap and current status
- Accepted design
- Current implementation difference, only when material
- Relevant roadmap plan
- Deferred or outside-scope boundaries

## Visual plan
- Diagram or image brief
- Question each visual answers
- Expected labels and data

## Exit state
- Representation produced for the next article
- Exact next-page question

## Links to include
- Canonical design section
- Main implementation owner
- Focused supporting docs
```

The final prose may merge or rename these headings. The outline must preserve their teaching functions.

### Cross-page rhythm

Each article should:

1. Open with a problem, not a definition list.
2. Show the current pipeline position near the top.
3. Explain the general concept before naming Beanstalk types.
4. Transform the running example visibly.
5. explain one or more Beanstalk-specific choices.
6. Include a small failure path.
7. State the representation handed to the next stage.
8. End by raising the next question rather than summarising the article.

## Visual and diagram plan

Each full article should plan for three to five visual breaks. Prefer diagrams that teach data movement over decorative images.

### Reusable visual language

Create a consistent set of diagram shapes:

- rounded boxes for owned data
- outlined boxes for views or borrowed interfaces
- arrows for transformations
- dotted arrows for references or dependencies
- stacked lanes for parallel artefact data
- highlighted pipeline stage for the current article
- red callouts for rejected input
- amber callouts for current versus accepted design gaps
- labels that do not rely on colour alone

### Reusable visuals

Plan these once and reuse them where useful:

1. **Pipeline strip:** project tree to output files, with the current stage highlighted.
2. **Running-example state card:** the exact form of the greeting project at the current boundary.
3. **Owner card:** build system, frontend, AST, HIR, analysis, builder or backend.
4. **Accepted versus current marker:** small status label linked to the progress matrix or roadmap.

### Article visual requirements

Each outline must include:

- at least one data-flow or graph diagram
- at least one before-and-after representation
- at least one running-example visual or annotated code block
- one visual that explains a Beanstalk-specific choice when the page contains one

Use project-owned diagrams and screenshots where possible. External image placeholders must follow the informative-writer skill and use public-domain sources only. Do not add stock imagery that fails to explain the compiler.

Every planned visual needs:

- a useful alt-text brief
- a caption brief
- the question it answers
- a note saying whether the diagram shows an exact compiler structure or a teaching simplification

## Page-by-page outline briefs

### 00. `overview.bd` - From Beanstalk source to a webpage

**Reader promise**

Give the reader a map of the whole journey without teaching each stage yet.

**Section sequence**

1. Open with the two-line contrast: Beanstalk source on one side and `<p>Hello, Priya!</p>` on the other.
2. Introduce the running project tree.
3. Explain that compilation creates a sequence of more precise representations.
4. Introduce the four large owners:
   - build system
   - compiler frontend and analysis
   - artefact builder
   - target backends
5. Show the complete pipeline strip.
6. State the educational authority boundary. Link the compiler overview, build-system design and progress matrix inline.
7. Present the ordered reading path.
8. State that the series teaches accepted architecture and labels current gaps.

**Beanstalk focus**

Preview templates, integrated build orchestration, GC-backed memory safety and mixed JavaScript or Wasm output. Save detailed reasons for later articles.

**Visual plan**

- Hero transformation from source snippet to final paragraph.
- Complete pipeline strip.
- Four-owner responsibility map.

**Exit state**

The reader sees the destination and asks what a compiler does between source and output.

---

### 01. `what-a-compiler-does.bd` - What a compiler actually does

**Reader promise**

Explain compiler, interpreter, transpiler, linker, build system, frontend, backend and intermediate representation without assuming compiler theory.

**Section sequence**

1. Start from the misleading shortcut that compilers only turn code into machine instructions.
2. Separate source-language meaning from target representation.
3. Introduce frontend, middle analysis, backend and linking as responsibilities rather than rigid products.
4. Explain why compilers use multiple representations.
5. Distinguish a compiler library from the project tool that discovers inputs and writes outputs.
6. Place Beanstalk in this vocabulary.
7. Show the running example at the raw-source checkpoint.
8. Explain that each later stage should create a fact once and pass it on.

**Why Beanstalk does it this way**

Contrast a backend-neutral compiler library with a compiler tied directly to one target or one command-line path.

**Visual plan**

- Compiler versus interpreter versus transpiler responsibility diagram.
- Source meaning versus target encoding comparison.
- Representation ladder.

**Exit state**

The reader understands the vocabulary and asks why Beanstalk's language design creates an unusual compiler shape.

---

### 02. `beanstalk-design-choices.bd` - Why Beanstalk makes different choices

**Reader promise**

Give readers the design lens they need before they encounter unfamiliar syntax or architecture.

**Section sequence**

1. Explain Beanstalk's goal: keep the source language small while allowing compiler complexity where it removes user-facing complexity.
2. Group the major choices rather than listing syntax:
   - language and build system designed together
   - templates instead of a general macro or UI framework layer
   - static traits and choices instead of dynamic trait objects
   - `Error!` and `catch` instead of first-class public result values
   - shared access, explicit copy and exclusive `~` access
   - GC fallback with inferred ownership optimisation
   - no shadowing, no general closures and no broad reflection
3. Explain what Beanstalk gives up for each family of choices.
4. Preview module roots, Builder packages and backend-neutral compilation.
5. Mark outside-scope choices clearly. Do not describe them as missing features.
6. Show how the running example benefits from templates and integrated HTML building.

**Roadmap notes**

Distinguish accepted design boundaries from incomplete implementation. Link the design-scope page and roadmap rather than copying their lists.

**Visual plan**

- Choice map pairing mainstream approach with Beanstalk approach.
- Complexity-placement diagram: source language versus compiler.
- Small example showing template code replacing a separate string or markup layer.

**Exit state**

The reader understands the design bias and asks what happens when `bean build` starts.

---

### 03. `starting-a-build.bd` - A build starts before source compilation

**Reader promise**

Explain command selection, builder capabilities, project config and the build-system versus frontend boundary.

**Section sequence**

1. Begin at `bean build` or `bean dev`.
2. Explain why the command selects the artefact builder, build profile, tooling overlays and target intent before config schema validation.
3. Introduce the builder capability surface.
4. Explain the accepted self-contained `config.bst` model.
5. Introduce the project record, builder section and synthetic `@project` interface at a high level.
6. Explain source `#Import` contracts without teaching their full validation rules.
7. Show which values the running project needs before source discovery.
8. Draw the ownership boundary:
   - build system discovers and schedules
   - compiler prepares and semantically compiles
9. Add a current-versus-accepted config callout when the queued config plan has not landed.

**Why Beanstalk does it this way**

Compare a command-selected capability surface and typed compile-time config with a free-form build script that can execute arbitrary host code.

**Visual plan**

- Command to capability-surface flow.
- Config bootstrap boundary.
- Accepted config value flow into `@project`.

**Exit state**

The build owns validated project settings and asks which files and modules belong to the compilation.

---

### 04. `project-graphs-and-modules.bd` - Turning folders into a compiler graph

**Reader promise**

Teach source ownership, modules, packages, roots, dependency graphs and deterministic provider scheduling.

**Section sequence**

1. Start with the ambiguity of compiling a directory full of files.
2. Explain nodes, directed edges, providers and consumers.
3. Introduce Beanstalk's module terminology:
   - normal `#*.bst` root
   - scoped `+*.bst` support root
   - project package facade
   - ordinary owned source files
   - source-backed and binding-backed packages
4. Explain physical ownership versus semantic reachability.
5. Build the running project's canonical source index and module node.
6. Explain module-root-relative imports and facade boundaries.
7. Introduce `OwnedSourceSet`, `SemanticSourceSet` and check-only orphan units.
8. Explain provider-first compile waves and why one physical module compiles once.
9. Cover deterministic identity assignment at a high level.
10. Mark current entry-closure compilation as transitional if the canonical module plan has not landed.

**Why Beanstalk does it this way**

Compare structural scoped packages with a global search path, nearest-match resolution or package-folder configuration.

**Visual plan**

- Filesystem tree mapped to module and package nodes.
- Provider graph for the running example.
- Ownership versus reachability Venn or lane diagram.
- Compile-wave diagram.

**Exit state**

Stage 0 has chosen source files and asks the compiler to prepare their text once.

---

### 05. `tokenization.bd` - From characters to located tokens

**Reader promise**

Explain lexical analysis, token kinds, source spans and Beanstalk's source-kind entry modes.

**Section sequence**

1. Show why a parser needs boundaries rather than a raw character stream.
2. Tokenise a short line from `#page.bst`.
3. Explain token kind, authored spelling and source location.
4. Cover identifiers, literals, delimiters, operators, comments and template context.
5. Explain lexical diagnostics such as symbolic spacing and invalid escapes.
6. Show the shared numeric text grammar without assigning numeric semantic types yet.
7. Compare `.bst`, `.bd` and `.md` preparation:
   - code-mode tokenization
   - implicit Beandown template body
   - plain Markdown preparation without a tokenizer entry mode
8. Explain string-table and source-identity use only after the core token idea lands.
9. State what tokenization must not decide.

**Why Beanstalk does it this way**

Explain why source kinds reuse ordinary later declarations instead of creating separate AST, HIR or backend pipelines.

**Visual plan**

- Character stream to token cards.
- Source span overlay.
- Three source-kind entry paths converging on prepared declarations.

**Exit state**

The compiler owns located tokens and asks what top-level declarations, imports and root activity they contain.

---

### 06. `declarations-imports-and-ordering.bd` - Finding the program's shape

**Reader promise**

Explain declaration shells, retained import syntax, interface binding, symbol visibility and topological ordering.

**Section sequence**

1. Explain why the compiler needs a module's top-level shape before it checks function bodies.
2. Extract declaration and import shells from the running example.
3. Separate the three relationship classes:
   - structural provider references for Stage 0
   - imported symbol bindings for visibility and AST
   - local declaration-ordering edges for Stage 3
4. Explain prepared syntax before provider interfaces exist.
5. Explain interface binding after providers compile.
6. Introduce stable imported identities and canonical type facts.
7. Build a local declaration dependency graph.
8. Run a stable topological sort and explain cycle diagnostics.
9. Explain same-file source-order rules and why function-body references do not order declarations.
10. Show why imported declarations never become local graph nodes.

**Why Beanstalk does it this way**

Compare parsing once and binding later with reparsing or copying provider declarations into every consumer.

**Visual plan**

- Three-edge-class diagram.
- Prepared header before and after interface binding.
- Declaration DAG and sorted sequence.
- Cycle diagnostic example.

**Exit state**

The module now has bound visibility and an ordered declaration stream ready for semantic checking.

---

### 07. `ast-semantics-and-types.bd` - Giving syntax checked meaning

**Reader promise**

Explain AST semantics, name resolution, type identity, coercion, executable-body checking and terminality.

**Section sequence**

1. Contrast a parsed syntax tree with a typed semantic tree.
2. Resolve `greet(visitor)` through bound visibility.
3. Introduce scopes, no-shadowing and collision policy.
4. Explain the module-local `TypeEnvironment` and local `TypeId` handles.
5. Explain canonical cross-module type identity without exposing donor-local IDs.
6. Show natural expression typing versus contextual receiving boundaries.
7. Cover calls, assignments, returns and field construction as receiving boundaries.
8. Explain value-producing blocks and terminality at a high level.
9. Introduce public-surface validation and hidden anonymous record identity where useful.
10. State what AST owns and what HIR or backends must not rediscover.

**Why Beanstalk does it this way**

Cover strict expression typing, explicit casts, no shadowing and semantic identity over rendered type names.

**Visual plan**

- Syntax tree versus typed AST.
- Local `TypeId` mapped to canonical identity.
- Natural type to receiving-context coercion.
- Scope and collision map.

**Exit state**

The compiler understands names, types and source behaviour. It still needs to finish compile-time work and specialised static evidence.

---

### 08. `compile-time-semantics.bd` - Constants, generics, traits and casts before HIR

**Reader promise**

Explain the major AST-owned computations that disappear or become concrete before backend-facing IR.

**Section sequence**

1. Start with the `greeting_prefix` constant and explain compile-time folding.
2. Explain const records and folded backend-neutral values.
3. Show how source `#Import` becomes an ordinary folded constant after build-system resolution.
4. Introduce generic templates, call-site inference and generated-function requests.
5. Explain static trait evidence and why HIR receives concrete call targets rather than trait objects.
6. Explain compiler-owned casts and the difference between contextual coercion and explicit conversion.
7. Introduce checked numeric semantics and the planned `NumberN` family as a roadmap-aware example of stage ownership.
8. Explain public interface projection of folded values and static evidence.
9. State what does not cross the AST boundary.

**Why Beanstalk does it this way**

Compare static traits and concrete generated requests with dynamic trait objects, broad implicit conversion and operator overloading.

**Roadmap notes**

Use the accepted numeric design where relevant, but label `Number` and `Byte` implementation status. Do not let the numeric roadmap dominate the article.

**Visual plan**

- Constant source to folded value.
- Generic template to request to concrete sidecar preview.
- Static trait evidence to concrete call target.
- Coercion versus cast fork.

**Exit state**

Most static meaning has become concrete. Templates still need their own structural preparation before HIR.

---

### 09. `templates-and-tir.bd` - Why templates need their own temporary IR

**Reader promise**

Explain first-class templates, TIR, compile-time versus runtime template work, source-kind adapters and constrained reactivity.

**Section sequence**

1. Start with the HTML paragraph in the running example.
2. Explain why a string template contains more structure than a normal string concatenation expression.
3. Introduce TIR as AST-local structural authority.
4. Walk through `Parsed -> Composed -> Formatted -> Finalized`.
5. Explain slots, wrappers, child templates and control flow at a high level.
6. Separate fully folded output from neutral runtime handoff data.
7. Explain why TIR never reaches completed AST, public interfaces, HIR or backends.
8. Show `.bd` and `.md` adapters producing ordinary synthetic `content #String` declarations.
9. Introduce reactivity as stable source and subscription metadata rather than a general closure system.
10. Link post-TIR performance work without teaching a cache that does not exist.

**Why Beanstalk does it this way**

Compare first-class constrained templates with general macros, JSX-style external transforms or a second markup compiler.

**Visual plan**

- Template source to TIR phase sequence.
- Slot and wrapper structure diagram.
- Folded versus runtime fork.
- TIR disappearing at the AST boundary.
- `.bst`, `.bd` and `.md` convergence diagram.

**Exit state**

The AST owns complete, checked meaning and neutral runtime handoff data. Before lowering, the reader needs to understand how failures travel through these stages.

---

### 10. `diagnostics.bd` - Compiler errors as structured data

**Reader promise**

Explain how Beanstalk detects, carries and renders errors without ending the full tutorial on diagnostics.

**Section sequence**

1. Follow one deliberate source mistake through tokenizer, parser or AST ownership.
2. Explain why the stage with the best semantic context should own the diagnostic.
3. Introduce `CompilerDiagnostic` for user failures and `CompilerError` for internal or infrastructure failures.
4. Explain stable codes, descriptors, structured reasons, source locations and secondary labels.
5. Show type diagnostics carrying semantic identity rather than formatted strings.
6. Explain deferred rendering for CLI, dev server and future tooling.
7. Introduce diagnosed and blocked modules in graph compilation.
8. Explain deterministic diagnostic merge order under parallel work.
9. Show how AST owns many compile-time errors while borrow and target validation retain their own diagnostic lanes.
10. Link the active diagnostics roadmap without turning the article into an implementation changelog.

**Why Beanstalk does it this way**

Compare structured diagnostic data with constructing final prose inside parser or type-checker branches.

**Visual plan**

- Source mistake to structured payload to two renderers.
- Diagnostic versus internal error lane.
- Canonical module failure and blocked consumer graph.

**Exit state**

The reader understands failure handling. The successful path now lowers typed semantic meaning into explicit backend-facing IR.

---

### 11. `hir.bd` - Lowering meaning into explicit control flow

**Reader promise**

Explain intermediate representation, lowering, control-flow graphs, temporaries, places and HIR validation.

**Section sequence**

1. Compare the nested AST form of `greet(visitor)` with a sequence of explicit operations.
2. Define lowering as changing representation without changing language behaviour.
3. Introduce functions, blocks, statements, terminators, locals, places and regions.
4. Show expression side effects becoming ordered statement preludes.
5. Lower runtime template construction into ordinary control flow and string operations.
6. Explain local, cross-module, generated and binding-backed call targets.
7. Introduce per-function link facts emitted alongside HIR.
8. Explain HIR validation before borrow validation.
9. List what HIR deliberately excludes: TIR, compile-time fragments, source imports, generic solving and final ownership.
10. Preview structured derived HIR views for future Wasm lowering without making them another semantic authority.

**Why Beanstalk does it this way**

Compare a backend-neutral HIR with lowering directly from AST into JavaScript or Wasm.

**Visual plan**

- AST expression to HIR statements.
- Small control-flow graph.
- HIR and side-fact lanes.
- Call-target classes.

**Exit state**

The compiler owns validated explicit control flow. That representation can now support memory-safety analysis.

---

### 12. `memory-management-and-gc.bd` - Automatic memory management before the borrow checker

**Reader promise**

Give readers enough memory-management background to understand Beanstalk's hybrid design and its tradeoffs.

**Section sequence**

1. Explain the underlying problem: heap values must stay alive while observable and must eventually release resources.
2. Compare the main strategies:
   - manual allocation and freeing
   - reference counting
   - tracing garbage collection
   - ownership and deterministic destruction
   - region or arena allocation as a supporting technique
3. Explain common failure modes:
   - use after free
   - double free
   - leaks
   - invalid aliasing
   - conflicting mutation
   - data races when concurrency exists
4. Explain what GC solves and what it does not solve.
5. Explain what Rust-style ownership solves and what complexity it adds to source APIs.
6. Introduce Beanstalk's semantic baseline:
   - GC-backed correctness
   - mandatory access and borrow validation
   - optional ownership-aware lowering
7. Explain shared access by default, explicit copies, exclusive `~place` and inferred moves.
8. Compare JavaScript host GC with possible Wasm GC, linear-memory, handle or hybrid implementations.
9. State that ownership optimisation remains deferred until GC-first correctness where the roadmap says so.
10. Make clear that concurrency syntax remains deferred. Borrow rules still prevent the aliasing pattern that would cause data races under a future concurrent execution model.

**Why Beanstalk does it this way**

Explain why Beanstalk chooses a Rust-like safety model without source lifetimes, explicit references, a move keyword or separate owned and borrowed function signatures.

**Tradeoff coverage**

- GC lowers source complexity and gives a correctness fallback.
- Borrow validation rejects conflicting access even when GC could keep memory alive.
- Ownership-aware lowering may reduce collector pressure and release values earlier.
- Conservative analysis may keep more values under GC or reject some ambiguous access patterns.
- The model cannot promise deterministic destruction for correctness.

**Visual plan**

- Memory-strategy comparison timeline.
- Liveness versus access-safety diagram.
- Beanstalk layered model from source rules to GC or ownership-aware lowering.
- Host GC versus Wasm runtime options.

**Exit state**

The reader understands why GC and a borrow checker solve different problems. The next article can explain Beanstalk's analysis in detail.

---

### 13. `borrow-validation-and-drops.bd` - Borrow checking, race prevention and inferred destruction

**Reader promise**

Explain why Rust and Beanstalk validate borrowing, how Beanstalk analyses HIR and how the result guides safe ownership transfer and drops.

**Section sequence**

1. Start with two aliases and one attempted mutation.
2. Explain the core access rule:
   - many shared reads
   - one exclusive access
   - no overlapping shared and exclusive access
3. Connect the rule to iterator invalidation, mutation safety and data-race prevention.
4. State that current Beanstalk concurrency remains deferred. Do not claim existing threaded execution.
5. Introduce places, storage roots and alias root sets.
6. Explain analysis states such as uninitialised, slot, alias and conservative joins as internal facts, not source types.
7. Show forward fixed-point data-flow across branches and loops.
8. Explain future-use analysis and inferred moves.
9. Explain function access and return-alias summaries across module boundaries.
10. Introduce advisory drop sites, `drop_if_owned` and the unified ownership ABI.
11. Show how a GC backend may erase ownership operations while preserving the same validation result.
12. Explain reactive liveness and why observable state must not be freed early.
13. Cover conservative precision and future improvements without exposing source lifetimes.

**Why Beanstalk does it this way**

Compare explicit source lifetimes and move syntax with Beanstalk's inferred transfer, mandatory validation and GC fallback.

**Visual plan**

- Shared versus exclusive access timeline.
- Data race pattern with two workers, clearly labelled as a general or future-concurrency example.
- Branch and loop fixed-point diagram.
- Future-use decision tree.
- HIR plus side-table facts to GC or ownership-aware backend.
- `drop_if_owned` control-flow exit map.

**Exit state**

The module has validated HIR and read-only analysis facts. The compiler can package them into an immutable artefact.

---

### 14. `module-artefacts-and-reuse.bd` - Packaging a module once

**Reader promise**

Explain successful module results, separate artefact lanes, generated sidecars, fingerprints, parallel scheduling and incremental reuse.

**Section sequence**

1. Explain why the compiler should not hand later stages a loose mutable AST or a bag of unrelated fields.
2. Introduce the four module artefact lanes:
   - public semantic interface
   - executable HIR and borrow facts
   - per-function link facts
   - compiler and builder metadata
3. Place each running-example fact into the correct lane.
4. Introduce the five base fingerprints.
5. Explain stable origin identities and export bindings.
6. Explain success, diagnosed and blocked graph outcomes.
7. Introduce generated concrete functions as immutable sidecars rather than mutations of base modules.
8. Explain project-wide fixed-point request processing.
9. Show how public-interface changes differ from implementation, root-activity, runtime-dependency and documentation changes.
10. Explain deterministic parallel compilation:
    - canonical identity before workers
    - file and module deltas merged in canonical order
    - diagnostics independent of completion order
11. Explain in-memory reuse and future persistent artefacts.

**Why Beanstalk does it this way**

Compare immutable canonical module artefacts with recompiling shared modules for every entry or letting backends inspect mutable frontend state.

**Visual plan**

- Four-lane artefact box.
- Fingerprint invalidation matrix.
- Generated sidecar relation to base module.
- Parallel wave and deterministic merge diagram.
- First build versus second build of `greeting.bst`.

**Exit state**

The build system owns a coherent set of successful module artefacts. It now needs to select which dormant work and callable functions form an output entry.

---

### 15. `linking-entries-and-reachability.bd` - Choosing what runs

**Reader promise**

Explain dormant root work, implicit `start`, entry assembly, call reachability, page fragments and package assembly.

**Section sequence**

1. Separate compiling a module from activating it.
2. Explain the compiler-synthesised, non-exported and infallible `start` for normal roots.
3. Show the running example's `visitor` binding and paragraph fragment as dormant root work.
4. Explain why imported modules never run their root work.
5. Introduce entry candidates and final entry selection.
6. Build an `EntryAssembly` from already compiled facts.
7. Explain compile-time fragments, runtime fragments and insertion indexes.
8. Introduce call graphs and exact reachable function unions.
9. Explain generated functions, binding-backed calls, helpers, capabilities and assets in the reachable union.
10. Contrast entry assembly with `ProjectPackageAssembly`.
11. State that linking never triggers deferred source compilation.

**Why Beanstalk does it this way**

Compare compile-once dormant root work with active-root parser modes or executing imported module initialisers.

**Visual plan**

- Compilation versus activation timeline.
- Dormant `start` selected by one entry.
- Call graph with reachable and unreachable functions.
- Compile-time and runtime fragment merge order.

**Exit state**

The build has explicit roots and an exact reachable union. It can now decide where each function may run.

---

### 16. `target-planning-and-validation.bd` - Deciding where code can run

**Reader promise**

Explain target capabilities, target affinity, per-function JavaScript or Wasm assignment and validation before code generation.

**Section sequence**

1. Explain why semantically valid code may still rely on a target-specific capability.
2. Introduce validation roots supplied by the build system.
3. Explain target affinity from semantic package and capability metadata.
4. Walk the accepted mixed-target sequence:
   - reachable union
   - affinity analysis
   - deterministic assignment
   - compiler target validation
   - cross-target edge validation
5. Explain JavaScript-owned `start`.
6. Explain DOM or browser requirements forcing JavaScript and propagating backwards to callers.
7. Explain neutral functions defaulting to Wasm where supported.
8. Explain JavaScript-to-Wasm wrappers and the prohibition on Wasm-to-JavaScript Beanstalk calls after propagation.
9. Show every assignment carrying an explicit reason.
10. Explain entry-specific partitioning and physical variant keys.
11. Explain that `check` performs the same planning and validation but stops before lowering.
12. Mark current whole-module HTML-Wasm mode as experimental and transitional when it still exists.

**Why Beanstalk does it this way**

Compare per-function partitioning with compiling an entire page to one target or letting backends discover unsupported features during emission.

**Visual plan**

- Capability-labelled call graph.
- Backwards JavaScript requirement propagation.
- Completed JavaScript and Wasm partition.
- Permitted wrapper edge and rejected reverse edge.
- Entry-specific physical variant reuse.

**Exit state**

Every reachable function has a validated target and explicit imports, capabilities and layout needs. Backend lowerers can now translate without guessing.

---

### 17. `backend-lowering.bd` - Turning HIR into JavaScript and Wasm

**Reader promise**

Explain target code generation, runtime helpers, Wasm LIR, ABI and how two backends preserve one language contract.

**Section sequence**

1. Explain that lowerers receive validated selected functions, not source files.
2. Show one HIR operation taking different JavaScript and Wasm forms.
3. Explain JavaScript lowering through host values, functions and GC reachability.
4. Explain Wasm lowering through a structured derived HIR view and backend-owned structured LIR.
5. Introduce ABI, layouts, handles, memory and runtime imports at a beginner level.
6. Explain page-local shared Wasm runtime and memory in the accepted design.
7. Explain JavaScript companion facades and wrappers for Wasm-owned functions.
8. Cover external binding-backed calls and demand-driven glue.
9. Connect borrow facts to optional Wasm ownership lowering.
10. State that lowerers return output records and must not write project outputs directly.
11. Mark old dispatcher-loop LIR, `bst_start`, `i64` Int bridges and per-module memory as removed final-design paths, not concepts to teach.

**Why Beanstalk does it this way**

Compare a shared semantic HIR with separate frontends or backend-specific source interpretation.

**Visual plan**

- One HIR function branching into JavaScript and Wasm lowering.
- Structured HIR view to structured Wasm LIR.
- ABI boundary and wrapper diagram.
- Page runtime and shared memory layout.
- Borrow facts to optional `DropIfOwned` lowering.

**Exit state**

The backends have produced selected code, runtime modules, wrappers and asset records. The final article can assemble them into the page and write owned outputs.

---

### 18. `html-assembly-and-output.bd` - Building and writing the final page

**Reader promise**

Finish the walkthrough by combining compiler metadata, runtime code and builder policy into an HTML document and output manifest.

**Section sequence**

1. Return to the exact target fragment: `<p>Hello, Priya!</p>`.
2. List the inputs the HTML builder receives:
   - entry assembly
   - folded compile-time fragments
   - runtime fragment slots
   - selected JavaScript functions
   - selected Wasm variants
   - runtime helpers
   - external JavaScript glue
   - tracked assets
   - entry config and document metadata
3. Explain document-shell creation and route planning.
4. Show compile-time fragments inserted at recorded runtime indexes.
5. Show `start` invoked once and runtime fragments hydrated in source order.
6. Explain reactive mount work as a target-specific extension, not ordinary fragment assembly.
7. Explain demand-driven external JavaScript assets and import maps.
8. Explain tracked path usages and builder-owned asset decisions.
9. Introduce output records, output-root validation, skip-unchanged writes, manifests and stale cleanup.
10. Explain builder and profile ownership. One builder must not delete another builder's files.
11. Show the conceptual final output tree.
12. End on the final HTML fragment and the files that make it run. Do not add another primary tutorial page after this one.

**Why Beanstalk does it this way**

Compare central output ownership and explicit manifests with backends writing files independently or builders rediscovering source structure.

**Visual plan**

- Final assembly funnel.
- Fragment insertion and hydration timeline.
- Final HTML document anatomy.
- Output tree with HTML, JavaScript, Wasm, runtime assets and manifest.
- Manifest ownership and stale cleanup diagram.
- Full pipeline strip with final output highlighted.

**Exit state**

The project has generated, validated and owned artefacts on disk. The walkthrough ends here.

## Wrapper and navigation plan

Each article directory should contain a small `#page.bst` wrapper that:

- imports the matching `.bd` content
- imports the shared docs navbar, section and theme helpers
- sets a specific page title and description
- renders breadcrumbs
- renders the article content
- adds previous and next links in the main sequence
- links back to the compiler-design landing page
- does not duplicate article prose

The landing `#page.bst` should:

- show the hero transformation
- render the ordered article list
- group no articles by internal compiler subsystem
- mark optional deep links without interrupting the main sequence

Do not edit generated files under `docs/release/**` directly.

## Migration map from the current structure

Use the current pages as source material, not as a structure to preserve.

| Current area | New home |
|---|---|
| root compiler-design overview | new landing overview |
| `stages/project-structure` | starting a build, project graphs and modules |
| `build-system-and-frontend-boundary` | starting a build, project graphs and modules |
| `stages/tokenization` | tokenization |
| `stages/header-parsing` | declarations, imports and ordering |
| `stages/dependency-sorting` | declarations, imports and ordering |
| `imports-packages-and-bindings` | project graphs and declarations/imports |
| `stages/ast-construction` | AST semantics and types |
| `ast/environment-and-type-resolution` | AST semantics and types |
| `type-identity-and-coercion` | AST semantics and types |
| `ast/expressions-statements-and-terminality` | AST semantics and types |
| `ast/constants-and-folding` | compile-time semantics |
| `ast/generics-traits-and-casts` | compile-time semantics |
| `ast/templates-tir-and-reactivity` | templates and TIR |
| `diagnostics-paths-and-symbols` | diagnostics |
| `stages/hir-generation` | HIR |
| `stages/borrow-validation` | borrow validation and drops |
| memory-management docs | linked authority for the two memory articles, not copied wholesale |
| `parallelism-and-determinism` | project graphs plus module artefacts and reuse |
| `entry-start-and-page-fragments` | linking, entries and reachability |
| `stages/backend-lowering` | target planning plus backend lowering |
| `backend/validation-and-output-writing` | target planning plus final output |
| `backend/js-lowering` | backend lowering |
| `backend/wasm-lowering` | backend lowering |
| `backend/html-project-assembly` | HTML assembly and output |
| `backend/external-js-and-runtime-assets` | backend lowering plus final output |
| `backend/tracked-assets` | HTML assembly and output |

Before deleting or moving any route:

1. Search the full repository for inbound links.
2. Update source links in docs, README files and roadmap documents where appropriate.
3. Do not preserve stale pages as duplicate compatibility copies.
4. Keep one teaching owner for each concept.
5. Rebuild generated documentation after source changes.

## Agent workflow

### Phase 1: Inventory without rewriting

1. Read every required authority and roadmap file.
2. List every current compiler-design `.bd` and `#page.bst` file.
3. Record each page's useful concepts, stale details and duplicate material.
4. Build a concept-to-authority table.
5. Build a concept-to-current-owner table for source links.
6. Record accepted-design versus current-implementation gaps.
7. Do not modify docs yet.

**Deliverable:** inventory and conflict notes.

### Phase 2: Freeze the master teaching sequence

1. Confirm the 18-article sequence above.
2. Confirm each article has one main question.
3. Confirm no concept requires knowledge from a later page.
4. Confirm diagnostics appears after AST-owned semantic work.
5. Confirm memory management comes before borrow-validation mechanics.
6. Confirm the final primary page covers artefact assembly and files on disk.
7. Confirm the running example can survive every stage.

**Deliverable:** master sequence, dependency map and example state ledger.

### Phase 3: Verify the running example

1. Check the current compiler status.
2. Keep the main `.bst` example runnable where possible.
3. Decide whether current config syntax or accepted config syntax appears in the runnable fixture.
4. Label accepted future syntax separately when it has not landed.
5. Record the exact conceptual output fragment as `<p>Hello, Priya!</p>`.
6. Record every temporary side example and the page that owns it.

**Deliverable:** one canonical example sheet used by all outlines.

### Phase 4: Create the landing outline

1. Rewrite `overview.bd` as the source-to-page map.
2. Add the hero transformation.
3. Add the full pipeline strip brief.
4. Add the ordered reading path.
5. Add the authority and status note.
6. Remove the current area-based browse structure from the primary path.

**Deliverable:** outline-only `overview.bd`.

### Phase 5: Create each article outline in order

For each page:

1. Copy the standard article-outline skeleton.
2. Fill the reader promise.
3. Record the entry and exit representation.
4. Add four to seven section headings.
5. Attach the running-example checkpoint to each major section.
6. Add a **Why Beanstalk does it this way** section.
7. Add a focused failure section.
8. Add roadmap and status notes.
9. Add three to five visual briefs.
10. Add inline authority and source links to include later.
11. Add the exact question that leads into the next article.
12. Remove any topic already owned by another page unless this page needs a one-paragraph reminder.

**Deliverable:** one outline-only `.bd` per page.

### Phase 6: Plan wrappers and navigation

1. Define each route title and description.
2. Add breadcrumb shape.
3. Add previous and next route links.
4. Ensure the final output page has no next article in the primary sequence.
5. Ensure optional memory, language or design-scope deep links do not reorder the tutorial.

**Deliverable:** wrapper plan and route map.

### Phase 7: Cross-cutting audits

Run these audits across all outlines:

#### Terminology audit

- Introduce each compiler term once before reuse.
- Keep module, package, binding, prelude, builder and backend distinct.
- Keep AST, TIR, HIR and Wasm LIR distinct.
- Keep compile-time fragment, runtime fragment and runtime string construction distinct.
- Keep access, borrowing, ownership and allocation distinct.

#### Ownership audit

- Stage 0 owns discovery and scheduling.
- Header preparation owns retained top-level syntax.
- Interface binding owns imported semantic facts and final visibility.
- Stage 3 owns local declaration ordering.
- AST owns source meaning, folding, generics, traits, casts and TIR.
- HIR owns explicit runtime control flow.
- Borrow validation reads HIR and writes side tables.
- The build system owns roots, link plans, target assignment and output writing.
- The compiler owns target-contract validation.
- Lowerers consume explicit validated inputs and do not rediscover source.

#### Roadmap audit

- No queued plan appears as completed work.
- No current experimental backend shape appears as final architecture.
- Accepted architecture remains the main explanatory path.
- Deferred and outside-scope features use the correct label.
- Concurrency discussion does not imply current async or thread support.
- Ownership optimisation does not appear required for correctness.

#### Beanstalk-choice audit

Confirm the outlines explain:

- integrated build and language design
- constrained static features rather than broad metaprogramming
- template and TIR design
- module-root and package model
- no-shadowing and strict name collisions
- static traits and choices
- `Error!` handling
- shared access, explicit copy and exclusive `~`
- inferred moves and no source lifetimes
- GC fallback and optional inferred destruction
- mixed target planning
- central output ownership

#### Running-example audit

- The name remains Priya.
- The final fragment remains `<p>Hello, Priya!</p>`.
- File names and declarations remain stable.
- Each representation follows from the previous one.
- No article silently changes source to make its explanation easier.

#### Visual audit

- Every article has three to five planned visual breaks.
- Every visual answers a named question.
- No diagram relies on colour alone.
- Conceptual diagrams say that they simplify exact structures.
- External placeholders satisfy the informative-writer skill.

### Phase 8: Outline review passes

Apply the informative-writer workflow at outline level:

1. **Structural pass:** Check order, coverage, authority and missing concepts.
2. **Style pass:** Check titles, section rhythm, curiosity and accessible wording.
3. **Initial review:** Remove duplicate sections, split dense article plans and strengthen weak visual explanations.
4. **Final review:** Check the core writing rules, links, placeholder policy and article handoffs.

Do not polish full prose during these passes.

### Phase 9: Technical validation

For a documentation-only outline change:

1. Run `bean check docs` during iteration when useful.
2. Run `bean build docs --release` as the required final gate.
3. Use the equivalent Cargo command when a suitable `bean` release build is unavailable.
4. Inspect every changed route.
5. Inspect generated diffs.
6. Verify links, headings, tables and code blocks.
7. Confirm no non-documentation file changed.
8. Do not edit generated `docs/release/**` files by hand.

Report exactly which commands ran and whether they passed.

## Writing rules for the later article pass

The outline agent must record these constraints so the writing agent cannot miss them:

- Use British English.
- Use straight `'` apostrophes.
- Use direct, active prose.
- Avoid filler, hollow intensifiers and vague qualifiers.
- Do not use semicolons, Oxford commas or em dashes in prose.
- Avoid the banned vocabulary from the informative-writer skill.
- Define jargon at first use.
- Use analogies sparingly and state where each analogy stops matching the compiler.
- Avoid corporate tone, hype and forced jokes.
- Keep a curious, technical voice anchored by the repository README and supplied tech-writing references.
- Vary paragraph and sentence length.
- Use code and diagrams to carry explanation instead of expanding every point into prose.
- Link real code and design files inline.
- Do not add a bibliography or dense citation section.
- Do not copy canonical design text into the educational pages.
- Do not end sections with repetitive summaries.
- Do not describe Beanstalk's choices as universally better. State benefits and costs.

## Completion checklist

The outline task finishes only when all of these statements hold:

- [ ] The landing page presents one chronological route.
- [ ] Every article has a unique teaching purpose.
- [ ] Every `.bd` file has an outline, visual plan, status note and handoff.
- [ ] The running example ends in `<p>Hello, Priya!</p>`.
- [ ] The series teaches general compiler concepts before Beanstalk-specific implementation names.
- [ ] The series explains Beanstalk's unusual language and compiler choices.
- [ ] The series distinguishes accepted design, current implementation, roadmap work and outside scope.
- [ ] The memory section compares manual memory management, GC and ownership approaches.
- [ ] The borrow section covers alias safety, mutation conflicts, data-race prevention, future-use analysis and inferred drops.
- [ ] The borrow section does not claim current concurrency support.
- [ ] GC remains the semantic correctness baseline throughout.
- [ ] Diagnostics appears before HIR and final lowering, with per-stage failure notes elsewhere.
- [ ] The primary sequence ends with backend lowering, HTML assembly and output ownership.
- [ ] Existing useful material has one mapped destination.
- [ ] No generated HTML was edited manually.
- [ ] The documentation-only release build passed or the report states the exact blocker.

