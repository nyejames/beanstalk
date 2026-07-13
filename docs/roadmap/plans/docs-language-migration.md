# Beanstalk language documentation migration

## 1. Purpose

This project will replace the single long-form language reference with focused Beandown references embedded in the public documentation site.

`docs/language-overview.md` remains untouched and authoritative throughout the migration. It is the parity baseline and backup reference until every language area has been migrated, reviewed and explicitly approved for an authority switch.

The new structure separates three jobs:

1. **`#page.bst` entry files** provide the public website experience: structure, introductions, examples, personality, visual layout, concept ordering and navigation.
2. **`<concept>-basic.bd` files** teach each concept with definitions, mental models and progressive examples while avoiding unstable edge cases and excessive technical detail.
3. **`<concept>.bd` files** are detailed semantic replacements written in the intended final canonical shape. They describe exact behaviour, restrictions, edge cases, inference boundaries, deferred behaviour and outside-scope decisions.

The unsuffixed file intentionally has no `-advanced` suffix. It feeds the website's **Advanced** panel and is intended to become the final semantic reference after the migration passes parity review. During migration it does not supersede `docs/language-overview.md`.

Presentation belongs to the page entry. Reusable teaching and semantic content belongs in Beandown.

`AGENTS.md` and `docs/language-overview.md` are protected throughout the migration. Ordinary route workers must not edit either file.

---

## 2. Fixed decisions

These decisions are project constraints. Route-level implementation plans must not reinterpret them.

| Area | Decision |
|---|---|
| Active language authority | `docs/language-overview.md` until explicit final approval |
| Detailed replacement | Unsuffixed `<concept>.bd` |
| Beginner teaching | `<concept>-basic.bd` |
| Website composition | `#page.bst` imports both levels |
| Website default | Basic |
| Alternate website level | Advanced |
| Toggle granularity | One independent toggle per concept |
| Toggle implementation | Two native radios with CSS `:checked` selectors |
| JavaScript | None for the initial implementation |
| ARIA tabs | Do not use |
| Page heading | One H1 per page |
| Concept heading | One stable H2 outside both variants |
| Fragment headings | H3 and deeper only |
| Navigation | Explicit Previous and Next page links |
| Sequence endpoints | Dedicated Previous-only or Next-only pager |
| Sidebar and glossary | Designed for later, not implemented now |
| Cross-page level persistence | Not implemented now |
| Migration cadence | Small, reviewable patches |
| Monolith editing | Prohibited during migration |
| `AGENTS.md` editing | Prohibited during migration |
| Shadow authority files | Do not create alternate copies such as `language-review.md` or `language-overview-new.md` |
| Monolith removal | Only after full parity review and explicit user approval |
| Generated release artifacts | May remain modified during and after a patch |
| Generated HTML editing | Prohibited |
| Import/export repairs | Allowed when current strict export rules require a narrow fix |
| Public LLM references | Allowed when they describe the language, tooling, project philosophy or a deliberate joke |
| Reader treatment | Never address or route the reader as though they might be an agent or LLM |

---

## 3. Authority model

### 3.1 Authority during migration

During the migration:

| Source | Responsibility |
|---|---|
| `docs/language-overview.md` | Complete compiler-facing language authority and stable parity baseline |
| Unsuffixed `<concept>.bd` | Detailed semantic replacement under review |
| `<concept>-basic.bd` | Simplified teaching version consistent with the detailed replacement |
| `#page.bst` | Website structure, tone, navigation and editorial context |
| `docs/src/docs/codebase/language/overview.bd` | Index of focused replacements completed so far |
| Progress matrix | Current implementation and backend coverage |
| Roadmap | Sequencing, active plans and unaccepted work |
| Compiler-design docs | Compiler stage ownership and implementation contracts |
| Memory docs | Access, borrowing, ownership and lowering architecture |

No ordinary route patch transfers authority.

The duplicate semantic coverage between the monolith and the new detailed files is intentional during migration. It provides a stable comparison target and prevents an incomplete replacement from silently becoming authoritative.

The unsuffixed files must be complete enough to replace the monolith later. They remain replacement candidates until the whole migration has passed review.

The monolith is stable evidence, not an infallible description of the current
compiler surface. When focused compiler probes and implementation review show
that a useful, coherent form is deliberately supported, the focused references
should document current language behavior and record the monolith disparity.

### 3.2 Final authority model

After every language area has been migrated and the complete parity audit has passed, the user may approve a separate authority-switch patch.

The intended final hierarchy is:

| Source | Responsibility |
|---|---|
| Unsuffixed `<concept>.bd` | Exact language semantics |
| `<concept>-basic.bd` | Simplified explanation consistent with the detailed reference |
| `#page.bst` | Website structure, tone, navigation and editorial context |
| `docs/src/docs/codebase/language/overview.bd` | Routing map to focused semantic references |
| Progress matrix | Current implementation and backend coverage |
| Roadmap | Sequencing, active proposals and unaccepted work |
| Compiler-design docs | Compiler stage ownership and implementation contracts |
| Memory docs | Access, borrowing, ownership and lowering architecture |

The final authority switch is a distinct user-approved operation. Migration workers must not edit `AGENTS.md`, shorten the monolith, turn it into an index or delete it.

### 3.3 Focused-reference index

Use:

```text
docs/src/docs/codebase/language/overview.bd
```

to list focused replacement sets completed so far.

During migration it should identify:

- each completed unsuffixed detailed file
- its paired basic file or files
- the public route that combines them
- the fact that `docs/language-overview.md` remains authoritative
- directly related language, compiler and memory references

It remains a neutral technical index. It must not claim that authority has moved.

### 3.4 Public codebase language page

The website route under:

```text
docs/src/docs/codebase/language/
```

may explain where compiler-facing language references live. It should stay useful to a reader and avoid migration-management chatter.

It may say that focused references are under review. It must not announce that a concept has superseded the monolith before final approval.

---

## 4. Goals

### 4.1 Semantic goals

The migration must:

- preserve every accepted rule in `docs/language-overview.md`
- preserve important valid examples, invalid examples and edge cases
- reconcile stale website content against the monolith
- keep deferred behaviour distinct from outside-scope behaviour
- distinguish accepted language design from current implementation coverage
- give every normative fact one planned final owner
- keep detailed references efficient to load directly
- make subtle compiler-relevant behaviour easy to update
- report implementation conflicts rather than silently choosing one side
- retain the monolith unchanged for final whole-project review

### 4.2 Public documentation goals

The website must:

- offer an approachable path through the language
- begin with concepts and examples rather than edge-case inventories
- let a reader switch one concept to Advanced without changing the rest of the page
- keep headings, anchors and navigation stable while switching
- provide enough examples that Basic is useful on its own
- expose detailed behaviour without sending readers to another site
- retain Beanstalk's playful and conversational voice
- maintain one clear learning sequence across pages
- remain useful on mobile, desktop and printed output

### 4.3 Maintenance goals

The structure should reduce unnecessary duplicated editing:

- subtle semantic changes normally update the unsuffixed file
- a basic file changes only when the stable mental model changes
- page copy changes when presentation, tone or navigation changes
- exact rules are not repeated in page introductions
- jokes and editorial examples do not become normative language rules
- future navigation work does not require rewriting concept content
- final direct references can be loaded without page layout or beginner prose

A basic file may be longer than its detailed partner because teaching takes examples and explanation. It should be semantically narrower, not necessarily shorter.

---

## 5. Non-goals

This project will not initially:

- migrate every language route in one patch
- add a global Basic or Advanced preference
- remember the selected level across page navigation
- add JavaScript for toggle behaviour
- implement the planned glossary
- implement the planned responsive sidebar
- redesign the complete documentation theme
- move compiler architecture into language references
- edit, shorten or replace sections in `docs/language-overview.md`
- edit `AGENTS.md`
- create a second monolithic authority or migration copy
- delete the monolith before complete parity review and explicit approval
- require documentation builds to leave a clean worktree
- treat current website pages as reliable semantic sources
- convert the async design draft into supported-language documentation
- generate basic files mechanically from detailed files
- use automated prose rewriting across the documentation tree
- turn every current compiler implementation detail into accepted language semantics

---

## 6. Layer responsibilities

### 6.1 `#page.bst`

The page entry owns:

- imports
- browser title
- page description
- theme head
- navbar
- breadcrumb
- the page's single H1
- friendly introduction
- page-level mental model
- ordering of concepts
- toggle component calls
- transition prose between concepts
- optional editorial examples
- jokes and conversational asides
- related-page links
- Previous and Next navigation

The page should not own:

- exhaustive syntax rules
- inference boundaries
- precise rejected forms
- backend-specific edge-case matrices
- normative deferred or outside-scope inventories
- compiler implementation paths
- duplicate copies of detailed examples

A page may summarise stable concepts, but a semantic fact needed by a direct reader of the Advanced reference must also exist in the unsuffixed `.bd` file.

### 6.2 `<concept>-basic.bd`

The basic file teaches.

It should usually contain:

1. A plain definition
2. Why the concept exists
3. The smallest useful example
4. A step-by-step explanation
5. Progressively richer examples
6. Common mistakes
7. A stable rule to remember

Basic content should:

- introduce terminology before using it
- avoid assuming knowledge of several other languages
- use realistic names and examples
- explain unusual punctuation
- avoid exhaustive edge-case lists
- avoid compiler implementation terms
- avoid backend detail unless the user must act differently
- avoid large support-status tables
- remain accurate even when it omits complexity

### 6.3 Unsuffixed `<concept>.bd`

The unsuffixed file specifies the detailed replacement contract.

It should usually contain:

1. A concise definition or contract
2. Canonical syntax forms
3. Exact semantic rules
4. Type and inference behaviour
5. Receiving-context or scope constraints
6. Mutation, access or ownership effects where relevant
7. Evaluation-order or lifetime rules when they are accepted language behaviour
8. Edge cases
9. Invalid and rejected forms
10. Deferred behaviour
11. Outside-scope behaviour
12. Links to adjacent detailed concepts

Detailed content should:

- use precise normative language
- prefer compact rule lists over long teaching prose
- preserve exact terminology
- include representative examples
- preserve unique important examples from the monolith
- distinguish invalid, deferred and outside-scope behaviour
- describe backend restrictions only when they affect observable language behaviour
- link to the progress matrix rather than becoming a status dashboard
- link to codebase design docs rather than duplicating compiler architecture
- remain useful when opened directly from the repository
- contain essential high-level framing even when that framing also appears in `#page.bst`

During migration, compare this file against `docs/language-overview.md`. It is a future canonical reference, not active authority yet.

### 6.4 Progress and roadmap

Use the progress matrix for:

- current implementation support
- partial support
- clean rejection
- backend coverage
- test coverage

Use the roadmap for:

- sequencing
- active plans
- unresolved proposals
- planned work not yet accepted as language design

Do not turn detailed language references into implementation-status logs.

---

## 7. File and naming architecture

### 7.1 One pair per independently toggleable concept

A page with several independently useful concepts needs several file pairs.

Example:

```text
docs/src/docs/errors/
├── #page.bst
├── error-values.bd
├── error-values-basic.bd
├── error-returns.bd
├── error-returns-basic.bd
├── propagation.bd
├── propagation-basic.bd
├── catch-and-recovery.bd
├── catch-and-recovery-basic.bd
├── options.bd
├── options-basic.bd
├── assertions.bd
└── assertions-basic.bd
```

A whole-page pair would permit only one toggle for the entire page and would hide too much material behind one choice.

### 7.2 Naming rules

Use lower kebab case:

```text
mutable-access.bd
mutable-access-basic.bd
```

Do not use:

```text
mutable-access-advanced.bd
advanced.bd
basic.bd
overview.bd
```

for ordinary concept content.

The unsuffixed file is the detailed replacement and Advanced-panel source. It becomes canonical only after the final authority switch.

### 7.3 Import aliases

Use explicit aliases that preserve the relationship:

```beanstalk
import @./mutable-access {
    content as mutable_access,
}

import @./mutable-access-basic {
    content as mutable_access_basic,
}
```

Do not use generic aliases such as `content`, `advanced`, `basic` or `reference` when a page imports several concepts.

### 7.4 Concept keys

Every toggle receives a manually authored stable key:

```text
mutable-access
error-propagation
fixed-collections
```

The key owns:

- the stable concept anchor
- radio input IDs
- the radio group name
- future sidebar deep links
- future glossary links

Keys must:

- be unique within the page
- use lowercase ASCII kebab case
- remain stable when display wording changes
- never be generated automatically from a heading

### 7.5 Heading rules

Each page has exactly one H1.

Each toggled concept has one H2 outside both content variants.

Basic and detailed Beandown files:

- must not begin with H1 or H2
- may use H3 and deeper headings
- should begin with a definition or contract paragraph
- must not repeat the concept title
- must not own page navigation

---

## 8. Information preservation and parity review

### 8.1 The monolith is read-only evidence

`docs/language-overview.md` is the stable comparison target.

Route workers must:

- read the relevant section
- quote or inventory its rules in their private work notes
- compare the final detailed files against it
- leave the source file byte-for-byte unchanged

They must not:

- replace migrated sections with pointers
- add migration notices
- shorten examples
- correct unrelated wording
- split the file
- create a shadow copy under another name

Before and after each patch, inspect:

```sh
git diff -- docs/language-overview.md AGENTS.md
```

Any patch-created change is a blocking scope violation.

### 8.2 Per-concept rule inventory

Before writing a concept, inventory:

- every normative paragraph
- every table row
- every syntax form
- every valid example that carries unique information
- every invalid or removed form
- every edge case
- every deferred feature
- every outside-scope decision
- every statement containing words such as `must`, `must not`, `only`, `never`, `invalid`, `requires`, `deferred` or `outside scope`

Assign each item to one planned unsuffixed owner.

Do not start prose until the ownership map is clear.

### 8.3 Example parity

The Advanced panel completely replaces Basic. An important example present only in Basic is not visible when Advanced is selected.

For that reason:

- preserve important unique monolith examples in the detailed file
- replace an example only with an equivalent or stronger example
- record why an example was intentionally omitted
- do not assume page-level or Basic examples satisfy direct-read parity

Basic may contain additional teaching examples that do not belong in the detailed file.

### 8.4 Direct-reading gate

Review every unsuffixed file as though the website page and Basic file were unavailable.

It must answer:

- What is this feature?
- What syntax forms are accepted?
- What exact behaviour is guaranteed?
- What types and contexts are valid?
- What forms are rejected?
- What edge cases matter?
- What is deferred?
- What is outside scope?
- Which adjacent reference owns related rules?

High-level framing that is essential to understanding the feature must not live only in `#page.bst`.

### 8.5 Basic consistency gate

For every statement in a Basic file, ask:

> Is this still true under every detailed rule that Basic omits?

If not, rewrite the Basic explanation. Do not use a convenient simplification that becomes false in an edge case.

### 8.6 Implementation-derived information

Implementation and tests are evidence, not automatic language authority.

When code exposes behaviour absent from the monolith:

1. Decide whether the behaviour is an intentional observable language contract
2. Check relevant compiler or memory design references
3. Check tests for deliberate coverage
4. Report ambiguity instead of silently canonising an implementation accident
5. Add it to the detailed file only when accepted as language behaviour

Examples include:

- expression evaluation order
- whether a source expression is evaluated once or repeatedly
- mutation visibility during iteration
- backend-specific string ordering
- runtime failure timing
- internal lowering shapes

Compiler-internal representation never belongs in language semantics merely because it is current implementation.

### 8.7 Compiler-surface verification loop

Before documenting an ambiguous or disputed source form:

1. Create one temporary probe file per focused question under
   `tmp/docs-language-probes/`.
2. Run `bean check` on a minimal expected-valid file and a nearby expected-invalid
   file when the boundary matters.
3. Record the command, result, diagnostic code, failure stage, implementation
   owner and focused tests found.
4. Inspect the parser, semantic owner and tests to distinguish parsing-only
   acceptance from full support.
5. Classify the finding using the evidence table in the current correction plan.
6. Apply the matching documentation action.
7. Delete all probe files before final validation.

Never modify compiler or test source during a documentation migration slice.

### 8.8 Conflict resolution order

For each concept, review evidence in this order:

1. Explicit user decisions for the migration
2. Current accepted semantics in `docs/language-overview.md`
3. Relevant compiler and memory design references
4. Progress matrix for implementation coverage
5. Tests and implementation when documentation and behaviour appear inconsistent
6. Existing website pages as non-authoritative teaching material

When implementation conflicts with accepted design:

- do not edit the monolith
- do not silently document the implementation as final semantics
- report the conflict
- keep implementation status in the progress matrix when an update is actually required

---
## 9. Writing and audience standards

### 9.1 Shared prose rules

Use:

- straight apostrophes
- natural contractions
- varied sentence lengths
- direct examples
- concise headings
- friendly confidence
- exact code syntax

Avoid:

- em dashes
- curly apostrophes
- prose semicolons
- unnecessary Oxford commas
- generic transitions such as `however`, `therefore` and `consequently`
- long document-mechanics preambles
- vague support wording when a precise semantic rule exists
- visible template escape artifacts

Do not change literal code syntax to satisfy prose preferences.

### 9.2 Audience-neutral documentation

Do not write:

- “agents should read this”
- “for agents”
- “agent-only”
- “if you are an LLM”
- “humans can skip this”
- “maintainer mode” as a public toggle label

The UI labels remain:

```text
Basic
Advanced
```

The unsuffixed file is not publicly described as an agent version.

### 9.3 Legitimate LLM references

There is no global ban on mentioning LLMs.

Keep references when they genuinely describe:

- Beanstalk's LLM-aware design goals
- terse diagnostic output being useful to LLM workflows
- editor or tool integration
- historical jokes
- deliberate playful examples such as the strawberry misspelling
- why the language favours readability and constrained syntax

Remove only wording that treats the reader as though they might be an LLM or routes them according to agent workflow.

### 9.4 Examples

Basic examples should:

- compile when presented as valid
- introduce one new idea at a time
- progress from small to realistic
- avoid unrelated advanced features
- explain intent with short comments when useful

Detailed examples should:

- be compact
- demonstrate exact boundaries
- show accepted syntax precisely
- include invalid forms when the rejection matters
- avoid decorative noise
- preserve unique semantic evidence from the monolith

Page-only examples may carry more personality. Unique semantic evidence belongs in Beandown.

### 9.5 Type-coherent examples

A valid example must be coherent as one statically typed snippet.

Do not put unrelated literal pattern types under one scrutinee, use a mutable
operation on an immutable binding, or call a returning function without its
declared return slot.

When a block demonstrates invalid forms, label the block or surrounding prose
clearly. Do not mix valid and invalid lines without explaining which are
rejected.

### 9.6 Inline template syntax

Do not expose escape artifacts such as:

```text
["#[...]"]
["[source]"]
```

Use full code blocks when template syntax cannot be represented cleanly inline.

---

## 10. Toggle and shared component design

### 10.1 Accepted interaction model

Each concept uses:

- one `fieldset`
- one visually hidden `legend`
- two native radios
- one shared radio `name`
- two unique input IDs
- Basic checked by default
- labels styled as a restrained segmented selector
- CSS sibling selectors to reveal one panel
- no JavaScript
- no ARIA tab roles

The heading stays outside both panels so its anchor remains stable.

### 10.2 Accessibility requirements

The component must:

- use native radio semantics
- group each pair with `fieldset` and `legend`
- visually clip inputs rather than applying `display: none`
- provide visible `:focus-visible` styling on the matching label
- maintain sufficient contrast in both colour schemes
- let normal radio keyboard behaviour change selection
- remove the inactive panel with `display: none`
- keep the concept heading stable
- avoid `role="tab"`, `role="tabpanel"` and related tab attributes

Without CSS, both explanations may appear. That is an acceptable fallback because all content remains available.

### 10.3 Unique group rule

Within one concept, the two inputs share a name:

```text
mutable-access-level
```

Different concepts must use different names. Selecting Advanced in one concept must not reset another concept.

### 10.4 Default and persistence

Basic is the authored default.

The initial version does not remember selection:

- across page navigation
- after reload
- across different concept selectors

A later JavaScript enhancement may introduce a shared preference. It must remain progressive enhancement over the native-radio implementation.

### 10.5 Shared style ownership

Reusable documentation styles live in:

```text
docs/src/styles/docs.bst
```

The shared foundation should contain:

```text
docs_content_css
language_docs_css
language_theme_head
codebase_theme_head
doc_level
doc_pager
doc_pager_previous
doc_pager_next
```

#### `docs_content_css`

Owns article content shared by language and codebase documentation:

- code-block containers
- horizontal overflow for source examples
- tables
- table cells and headers
- responsive table sizing

Both `language_theme_head` and `codebase_theme_head` include it.

This is required because imported Beandown code blocks may render with an empty inline style. Their visual treatment must come from shared CSS rather than page-local helper identity.

#### `language_docs_css`

Owns:

- explanation selector styles
- visually hidden utility
- selected label styles
- focus-visible styles
- panel visibility
- narrow-screen layout
- reduced-motion handling
- pager styles

#### `language_theme_head`

Composes:

```text
theme_head
docs_content_css
language_docs_css
```

#### `codebase_theme_head`

Composes:

```text
theme_head
docs_content_css
```

### 10.6 Pager components

Use:

```text
doc_pager
```

when both neighbours exist.

Use:

```text
doc_pager_previous
```

for a final page with only a Previous destination.

Use:

```text
doc_pager_next
```

for a first page with only a Next destination.

Do not emit an empty anchor to simulate a missing neighbour.

### 10.7 Strict export-block compatibility

Recent language changes use explicit `export:` blocks.

The current docs root reexports shared docs declarations through:

```text
docs/src/#page.bst
```

When adding a shared helper:

- define it in `docs/src/styles/docs.bst`
- add it to the explicit export import list in `docs/src/#page.bst`
- use current `export:` syntax
- do not restore removed legacy export syntax

A narrow import/export repair is allowed when `bean check docs` reports a real missing export or broken import.

Allowed repair owners may include:

```text
docs/src/#page.bst
docs/src/**/#mod.bst
libraries/html/#mod.bst
```

Keep the repair tied to the exact diagnostic. Do not use export churn as an excuse to redesign unrelated facades or libraries.

### 10.8 Beandown markup

In `.bd` files:

- the file body is an implicit compile-time `$md` template
- use Beanstalk Markdown and Beandown helpers, not full CommonMark
- avoid raw HTML
- keep headings at H3 or deeper
- do not rely on page-local imports
- do not use Markdown pipe tables
- do not use fenced code blocks

Use `[codeblock, $code("language"): ...]` for multiline source examples where that
helper is available in the importing page. Use `$code("text")` for literal grammar
displays. Put template syntax in full code blocks, not inline.

Use a structured list when detailed semantic data would otherwise be tabular.

A `#page.bst` file may build a visual table with Beanstalk table helpers when
that presentation genuinely helps the website. The unsuffixed `.bd` must still
preserve the complete information in directly readable list form.

Do not add route-local facades or exports solely to force table helpers into
Beandown.

Nested templates in `.bd` default to `$md` unless they declare an explicit
directive. Any explicit directive overrides the Beandown Markdown default for
that nested template.

---

## 11. Page composition contract

A migrated page should read as one coherent article:

```text
navbar
breadcrumb
H1
friendly introduction
optional page map or opening example

concept 1
    stable H2
    Basic / Advanced selector
    selected explanation

transition prose

concept 2
    stable H2
    Basic / Advanced selector
    selected explanation

related concepts
Previous / Next navigation
```

Example:

```beanstalk
import @./mutable-access {
    content as mutable_access,
}

import @./mutable-access-basic {
    content as mutable_access_basic,
}

import @styles/docs {
    navbar,
    section,
    language_theme_head,
    doc_level,
    doc_pager,
}

page_title #= "Values and bindings"
page_description #= "Bindings, mutability, shared access and explicit copies in Beanstalk."
page_head #= language_theme_head

#[navbar]

#[section, $md:
    @../ (Docs) / Values and bindings

    # Values and bindings

    Values are simple. The interesting part is how names are allowed to observe or change them.
]

#[section:
    [doc_level:
        [$insert("key"):mutable-access]
        [$insert("title"):Mutable access]
        [$insert("advanced"):[mutable_access]]

        [mutable_access_basic]
    ]
]

#[section:
    [doc_pager:
        [$insert("previous_path"):../language-overview/]
        [$insert("previous_title"):Language basics]
        [$insert("next_path"):../numbers/]
        [$insert("next_title"):Numbers]
    ]
]
```

Use `doc_pager_next` or `doc_pager_previous` at sequence endpoints.

---

## 12. Natural documentation flow

### 12.1 Learning sequence

The initial sequence is:

1. Getting Started
2. Language Basics
3. Values and Bindings
4. Numbers
5. Casts
6. Functions
7. Branching
8. Loops
9. Structs
10. Choices
11. Errors, Options and Assertions
12. Collections and Maps
13. Templates
14. Constants and Compile-Time Behaviour
15. Aliases
16. Generics
17. Traits
18. Reactivity
19. Project Structure
20. Libraries and Imports
21. Beandown
22. Markdown Imports

Suggested routes:

```text
/docs/getting-started/
/docs/language-overview/
/docs/bindings/
/docs/numbers/
/docs/casts/
/docs/functions/
/docs/branching/
/docs/loops/
/docs/structs/
/docs/choices/
/docs/errors/
/docs/collections/
/docs/templates/
/docs/constants/
/docs/aliases/
/docs/generics/
/docs/traits/
/docs/reactivity/
/docs/project-structure/
/docs/libraries/
/docs/beandown/
/docs/markdown/
```

Existing routes remain stable. New routes are expected for:

```text
bindings
numbers
casts
constants
```

### 12.2 Previous and Next navigation

Every page declares its neighbours explicitly.

Do not infer adjacency from directory order.

The first page uses `doc_pager_next`.

A final page with no onward language topic uses `doc_pager_previous`, unless the user explicitly chooses a related project page as the next destination.

### 12.3 Concept anchors

Each concept heading has a stable ID:

```text
/docs/bindings/#shared-access
/docs/errors/#catch-and-recovery
/docs/templates/#template-slots
```

These anchors support future:

- sidebar navigation
- glossary links
- cross-page references
- search results
- copied deep links

### 12.4 Future sidebar compatibility

Do not add the sidebar or hamburger menu yet.

Prepare for them by preserving:

- stable route order
- stable page titles
- stable concept keys
- one H1 per page
- one H2 per toggle concept
- explicit neighbour relationships

---
## 13. Proposed concept map

This is the initial ownership map. A route plan may split a concept further when one toggle would become too large. It should not merge concepts merely to reduce file count.

### 13.1 Language Basics

Route:

```text
docs/src/docs/language-overview/
```

Pairs:

```text
blocks-and-statements.bd
blocks-and-statements-basic.bd

comments-and-naming.bd
comments-and-naming-basic.bd

core-values.bd
core-values-basic.bd

strings-and-characters.bd
strings-and-characters-basic.bd
```

Owns:

- block punctuation
- statement shape
- comments
- naming conventions
- primitive value forms
- strings, raw strings and characters
- syntax tour

Does not own mutable access, detailed numeric behaviour, template semantics or project structure.

### 13.2 Values and Bindings

Route:

```text
docs/src/docs/bindings/
```

Pairs:

```text
bindings.bd
bindings-basic.bd

mutable-bindings.bd
mutable-bindings-basic.bd

shared-access.bd
shared-access-basic.bd

explicit-copies.bd
explicit-copies-basic.bd

shadowing.bd
shadowing-basic.bd
```

Owns declarations, reassignment, mutability, shared access, explicit copies, no-shadowing semantics and the distinction between binding mutability and call-site exclusive access.

Memory implementation strategy stays in the memory docs. This route owns observable language behaviour.

### 13.3 Numbers

Route:

```text
docs/src/docs/numbers/
```

Pairs:

```text
numeric-types.bd
numeric-types-basic.bd

numeric-literals.bd
numeric-literals-basic.bd

operators.bd
operators-basic.bd

checked-arithmetic.bd
checked-arithmetic-basic.bd
```

Owns `Int`, `Float`, literal grammar, operator spacing, result types, integer and real division, overflow, numeric failures and finite-Float rules.

### 13.4 Casts

Route:

```text
docs/src/docs/casts/
```

Pairs:

```text
cast-syntax.bd
cast-syntax-basic.bd

cast-targets.bd
cast-targets-basic.bd

fallible-casts.bd
fallible-casts-basic.bd

cast-evidence.bd
cast-evidence-basic.bd
```

Owns typed-boundary target selection, `cast`, `cast!`, local recovery, supported targets, invalid contexts, conversion behaviour and compiler-owned cast traits.

### 13.5 Functions

Route:

```text
docs/src/docs/functions/
```

Pairs:

```text
function-declarations.bd
function-declarations-basic.bd

parameters-and-defaults.bd
parameters-and-defaults-basic.bd

calls-and-access.bd
calls-and-access-basic.bd

returns-and-multiple-values.bd
returns-and-multiple-values-basic.bd
```

Owns signatures, parameters, defaults, named arguments, call ordering, success returns, multiple returns and immediate call access syntax.

Shared and exclusive access semantics are linked from Bindings rather than duplicated exhaustively.

### 13.6 Branching

Route:

```text
docs/src/docs/branching/
```

Pairs:

```text
statement-if.bd
statement-if-basic.bd

value-producing-if.bd
value-producing-if-basic.bd

pattern-matching.bd
pattern-matching-basic.bd

patterns-and-exhaustiveness.bd
patterns-and-exhaustiveness-basic.bd
```

Owns statement `if`, `else`, value-producing `if`, `then`, full match syntax, patterns, captures, guards, exhaustiveness and bodyless fallback arms.

Catch recovery uses value-producing blocks but remains owned by Errors.

### 13.7 Loops

Route:

```text
docs/src/docs/loops/
```

Pairs:

```text
conditional-loops.bd
conditional-loops-basic.bd

collection-loops.bd
collection-loops-basic.bd

range-loops.bd
range-loops-basic.bd

loop-control.bd
loop-control-basic.bd
```

Owns conditional loops, collection iteration, numeric ranges, inclusive and exclusive bounds, inferred direction, steps, bindings, `break` and `continue`.

Loops is the prototype route for the migration architecture.

### 13.8 Structs

Route:

```text
docs/src/docs/structs/
```

Pairs:

```text
struct-declarations.bd
struct-declarations-basic.bd

construction-and-fields.bd
construction-and-fields-basic.bd

receiver-methods.bd
receiver-methods-basic.bd

mutable-receivers.bd
mutable-receivers-basic.bd
```

Owns nominal identity, fields, defaults, construction, field access, receiver methods, receiver ownership restrictions and mutable receiver calls.

### 13.9 Choices

Route:

```text
docs/src/docs/choices/
```

Pairs:

```text
choice-declarations.bd
choice-declarations-basic.bd

variant-construction.bd
variant-construction-basic.bd

payload-patterns.bd
payload-patterns-basic.bd

choice-equality.bd
choice-equality-basic.bd
```

Owns unit and payload variants, construction, immutable payloads, payload matching, structural equality and unsupported equality payloads.

Generic declaration syntax is introduced here but specified under Generics.

### 13.10 Errors, Options and Assertions

Route:

```text
docs/src/docs/errors/
```

Pairs:

```text
error-values.bd
error-values-basic.bd

error-returns.bd
error-returns-basic.bd

propagation.bd
propagation-basic.bd

catch-and-recovery.bd
catch-and-recovery-basic.bd

options.bd
options-basic.bd

assertions.bd
assertions-basic.bd
```

Owns `Error`, error return slots, `return!`, postfix `!`, `catch`, recovery with `then`, optional values, postfix `?`, assertions and expected failure versus invariant failure.

General value-producing-block syntax stays under Branching.

### 13.11 Collections and Maps

Route:

```text
docs/src/docs/collections/
```

Pairs:

```text
collection-literals.bd
collection-literals-basic.bd

growable-collections.bd
growable-collections-basic.bd

fixed-collections.bd
fixed-collections-basic.bd

collection-operations.bd
collection-operations-basic.bd

hash-maps.bd
hash-maps-basic.bd
```

Owns collection types and literals, empty-literal inference, growable collections, fixed capacity and type identity, fallible operations, map key restrictions, insertion order and mutation/access behaviour.

The page may preserve playful examples such as the strawberry joke.

### 13.12 Templates

Route:

```text
docs/src/docs/templates/
```

Pairs:

```text
template-basics.bd
template-basics-basic.bd

template-directives.bd
template-directives-basic.bd

template-slots.bd
template-slots-basic.bd

child-wrappers.bd
child-wrappers-basic.bd

template-control-flow.bd
template-control-flow-basic.bd

markdown-formatting.bd
markdown-formatting-basic.bd
```

Owns template head and body, capture, directives, slots, inserts, `$children`, `$fresh`, template `if`, template `loop`, Markdown formatting and compile-time versus runtime behaviour.

Builder page-fragment assembly remains under Project Structure. Const-template folding is linked from Constants.

### 13.13 Constants and Compile-Time Behaviour

Route:

```text
docs/src/docs/constants/
```

Pairs:

```text
constant-bindings.bd
constant-bindings-basic.bd

constant-folding.bd
constant-folding-basic.bd

const-records.bd
const-records-basic.bd

const-templates.bd
const-templates-basic.bd
```

Owns `#` bindings, immutability, foldability, dependency and source-order rules, const records, compile-time templates and const template limits.

Project-entry fragment positioning remains under Project Structure.

### 13.14 Aliases

Route:

```text
docs/src/docs/aliases/
```

Pairs:

```text
type-aliases.bd
type-aliases-basic.bd

import-aliases.bd
import-aliases-basic.bd

payload-capture-aliases.bd
payload-capture-aliases-basic.bd
```

Owns transparent type aliases, import renaming, collision rules, facade-export alias distinctions and match payload capture aliases.

### 13.15 Generics

Route:

```text
docs/src/docs/generics/
```

Pairs:

```text
generic-declarations.bd
generic-declarations-basic.bd

type-application.bd
type-application-basic.bd

generic-inference.bd
generic-inference-basic.bd

generic-instances.bd
generic-instances-basic.bd

generic-limits.bd
generic-limits-basic.bd
```

Owns declaration-site parameters, `of`, concrete aliases, generic functions, immediate inference, instance restrictions and rejected or outside-scope surfaces.

Trait-bound semantics are linked from Traits.

### 13.16 Traits

Route:

```text
docs/src/docs/traits/
```

Pairs:

```text
trait-declarations.bd
trait-declarations-basic.bd

trait-requirements.bd
trait-requirements-basic.bd

conformance.bd
conformance-basic.bd

generic-trait-bounds.bd
generic-trait-bounds-basic.bd

trait-incompatibility.bd
trait-incompatibility-basic.bd

core-cast-traits.bd
core-cast-traits-basic.bd

trait-design-scope.bd
trait-design-scope-basic.bd
```

Owns trait contracts, `This`, `~This`, explicit conformance, evidence visibility, generic bounds, incompatibility, compiler-owned cast traits, static versus runtime heterogeneity and excluded trait-system complexity.

### 13.17 Reactivity

Route:

```text
docs/src/docs/reactivity/
```

Pairs:

```text
reactive-sources.bd
reactive-sources-basic.bd

subscriptions.bd
subscriptions-basic.bd

reactive-parameters.bd
reactive-parameters-basic.bd

mutation-and-invalidation.bd
mutation-and-invalidation-basic.bd

runtime-sinks.bd
runtime-sinks-basic.bd

reactivity-scope.bd
reactivity-scope-basic.bd
```

Owns reactive declarations, source identity, snapshot reads, subscriptions, function boundaries, invalidation, live sinks, backend restrictions, deferred reactivity and the relationship to closures and function values.

### 13.18 Project Structure

Route:

```text
docs/src/docs/project-structure/
```

Pairs:

```text
project-config.bd
project-config-basic.bd

module-entries.bd
module-entries-basic.bd

module-facades.bd
module-facades-basic.bd

page-fragments.bd
page-fragments-basic.bd

output-layout.bd
output-layout-basic.bd
```

Owns project config, entry roots, module entries, facades, runtime `start`, page fragments, output routes and build folders.

### 13.19 Libraries and Imports

Route:

```text
docs/src/docs/libraries/
```

Pairs:

```text
import-forms.bd
import-forms-basic.bd

source-libraries.bd
source-libraries-basic.bd

facade-exports.bd
facade-exports-basic.bd

builder-libraries.bd
builder-libraries-basic.bd

external-packages.bd
external-packages-basic.bd

javascript-libraries.bd
javascript-libraries-basic.bd

visibility-and-collisions.bd
visibility-and-collisions-basic.bd
```

Owns import syntax, namespaces, source libraries, facades, builder libraries, external packages, JavaScript import metadata, visibility and collisions.

### 13.20 Beandown and Markdown

Existing routes remain separate.

Beandown pairs:

```text
beandown-files.bd
beandown-files-basic.bd

beandown-imports.bd
beandown-imports-basic.bd

beandown-scope.bd
beandown-scope-basic.bd
```

Markdown pairs:

```text
markdown-files.bd
markdown-files-basic.bd

markdown-imports.bd
markdown-imports-basic.bd

markdown-boundaries.bd
markdown-boundaries-basic.bd
```

Beandown files are Beanstalk-aware imported content. Plain Markdown files are raw Markdown content without Beanstalk scope.

---

## 14. Future single-owner map

Every normative fact must have one planned unsuffixed owner after the final authority switch.

Examples:

| Rule | Planned detailed owner |
|---|---|
| Existing values use shared access by default | `bindings/shared-access.bd` |
| A call uses `~place` for an existing mutable argument | `functions/calls-and-access.bd`, linked to Bindings |
| `then` targets a value-producing block | `branching/value-producing-if.bd` or a dedicated value-block concept |
| `then` inside `catch` recovers success values | `errors/catch-and-recovery.bd` |
| Choice payload aliases use `as` | `aliases/payload-capture-aliases.bd` |
| Payload pattern shape | `branching/patterns-and-exhaustiveness.bd` |
| Generic bounds use traits | `traits/generic-trait-bounds.bd` |
| Generic declaration and inference | `generics/*.bd` |
| Const templates fold | `constants/const-templates.bd` |
| Entry fragments are assembled into pages | `project-structure/page-fragments.bd` |
| Template loop syntax | `templates/template-control-flow.bd` |
| Ordinary loop syntax | `loops/*.bd` |

Other pages may summarise and link. They must not carry a second exhaustive rule list after the final authority switch.

---

## 15. Migration ledger

Maintain a section-level parity ledger for the duration of the project.

Recommended location if explicitly requested:

```text
docs/roadmap/language-documentation-migration.md
```

Do not create the ledger in an ordinary route patch unless its implementation plan includes it.

Each ledger row should record:

| Field | Meaning |
|---|---|
| Monolith heading | Original source section |
| Detailed target | Unsuffixed `.bd` |
| Basic target | `-basic.bd` |
| Website route | Importing `#page.bst` |
| Related owner | Planned semantic cross-link |
| Detailed reference complete | Yes or no |
| Basic reference complete | Yes or no |
| Monolith parity reviewed | Yes or no |
| Important examples preserved | Yes or no |
| Implementation conflict reviewed | Yes, no or not applicable |
| Generated route inspected | Yes or no |
| Remaining discrepancy | Description or none |

A section is not complete merely because its prose was copied somewhere.

The ledger tracks replacement readiness. It does not transfer authority.

---
## 16. Patch strategy

### 16.1 Generated-output baseline

At the start of a patch:

1. Record the current branch and worktree state
2. Record pre-existing changes
3. Record existing `docs/release/**` changes separately
4. Read protected-file diffs
5. Run `bean check docs` when a source baseline is needed
6. Build documentation when generated inspection is required

A documentation build may update tracked release artifacts. Those changes may remain in the workspace and may be committed with the owning source change.

Do not require a clean post-build worktree. Review generated diffs instead.

### 16.2 Shared foundation

The shared foundation consists of:

- `docs_content_css`
- `language_docs_css`
- `language_theme_head`
- `codebase_theme_head`
- `doc_level`
- `doc_pager`
- `doc_pager_previous`
- `doc_pager_next`
- stable concept IDs
- Basic selected by default
- independent radio groups
- generated route inspection

Loops is the prototype route.

The foundation does not transfer language authority.

### 16.3 Route migration order

After the foundation is accepted, migrate one route per patch unless two routes are inseparable.

Recommended order:

1. Language Basics
2. Values and Bindings
3. Numbers
4. Casts
5. Functions
6. Branching
7. Errors, Options and Assertions
8. Structs
9. Choices
10. Collections and Maps
11. Templates
12. Constants
13. Aliases
14. Generics
15. Traits
16. Reactivity
17. Project Structure
18. Libraries and Imports
19. Beandown
20. Markdown
21. Core-library language surfaces

Loops is already the prototype and should receive only targeted follow-up corrections unless a review finds additional parity gaps.

### 16.4 Final parity and authority review

After every ledger row is complete:

- audit every detailed file against the untouched monolith
- audit deferred and outside-scope rules
- inspect every language route
- confirm Basic files remain accurate simplifications
- verify generated output
- review every implementation conflict
- present remaining discrepancies to the user

The user then decides whether and when to switch authority.

Migration workers must not edit `AGENTS.md`, shorten the monolith, turn it into an index or delete it unless a separate explicit instruction authorises that work.

---

## 17. Per-patch implementation contract

Every route migration plan should require these phases.

### 17.1 Read and reconcile

Read:

- the current tracked `AGENTS.md`
- this design brief
- the relevant monolith sections
- the current website page
- relevant compiler design references
- relevant memory references
- progress rows
- implementation and tests when rules appear inconsistent

Record conflicts before writing.

Reading `AGENTS.md` does not authorise editing it.

### 17.2 Record protected-file state

Run:

```sh
git diff -- AGENTS.md docs/language-overview.md
```

Record any pre-existing changes.

Do not stage, modify or overwrite either file.

### 17.3 Define future ownership

List concepts and assign every normative item to one planned unsuffixed `.bd`.

Do not start prose until the owner map is clear.

### 17.4 Write detailed files first

The unsuffixed files establish the detailed replacement contract while the monolith remains authoritative.

Review them for:

- complete rule coverage
- exact syntax
- normative language
- type and context rules
- edge cases
- rejected forms
- deferred behaviour
- outside-scope behaviour
- unique examples
- correct cross-links
- direct-reading quality

### 17.5 Perform detailed parity review

Compare each unsuffixed file against the monolith before writing Basic.

Do not rely on memory or a generated summary.

Record anything intentionally omitted and why.

### 17.6 Write Basic files manually

Do not mechanically shorten the detailed file.

Write the stable mental model, progressive examples and common mistakes.

Check that every simplified statement remains true.

### 17.7 Rewrite the page entry

The page should:

- own one H1
- introduce the page
- order the concepts
- provide transitions
- call `doc_level` for each pair
- preserve appropriate personality
- add related links
- add the correct pager

It should not duplicate exhaustive semantic rules.

### 17.8 Update the focused-reference index

Update `docs/src/docs/codebase/language/overview.bd` only when a replacement set is complete.

State that:

- the monolith remains authoritative
- the new files are detailed replacements under review
- the public route combines Basic and Advanced

Do not claim authority transfer.

### 17.9 Repair exports only when required

Run `bean check docs`.

If it reports a missing import or export:

- identify the exact owner
- update the current explicit export block
- avoid unrelated facade changes
- document the repair in the final report

### 17.10 Validate

For documentation-only patches:

```sh
bean check docs
bean build docs --release
```

Do not run the full Rust validation gate unless implementation code changes and the user explicitly changes the scope.

### 17.11 Inspect generated output

Inspect:

- one H1
- concept heading levels
- Basic selected by default
- Advanced fully replacing Basic
- keyboard focus
- radio exclusivity
- independence between concepts
- unique IDs
- no ARIA tab roles
- desktop layout
- narrow-screen layout
- dark mode
- reduced-motion behaviour
- code blocks
- tables
- links
- pager destinations
- no visible Beandown escape artifacts
- no manually patched HTML

### 17.12 Final audit

Verify:

- protected files remain unchanged
- the monolith still contains the original section
- detailed parity is complete
- Basic is accurate
- page prose does not become a second semantic authority
- import/export repairs are narrow
- generated artifacts are retained and explained
- no implementation or test source changed

---

## 18. Validation and generated artifacts

### 18.1 Allowed commands

Documentation-only migration patches use:

```sh
bean check docs
bean build docs --release
```

`bean check docs` may be repeated during iteration.

Do not run:

```text
just validate
cargo check
cargo test
cargo clippy
bean tests
benchmarks
Cargo wrappers around bean
```

unless a later user instruction changes the scope.

### 18.2 Generated output policy

Generated release artifacts:

- may be modified before work begins
- may change during the patch
- may remain modified at the end
- may be committed with the source patch

Do not reset them merely to make the worktree clean.

Do not edit them directly.

Review:

```sh
git diff -- docs/release
```

Distinguish:

- changes caused by the current source edits
- normal compiler-wide regeneration churn
- pre-existing generated changes

Report unrelated generated churn rather than hiding it.

### 18.3 Generated inspection over command claims

A successful command is not enough.

Inspect the generated route and confirm:

- content appears once
- headings are correct
- links resolve
- selectors work structurally
- code and tables are styled
- no source template syntax leaked into HTML

### 18.4 Report the inspection actually performed

Do not claim every character or every route was reviewed when inspection used
targeted searches.

A final report should distinguish:

- routes opened and read manually
- structural checks such as H1, toggle and pager counts
- key semantic phrases checked in Advanced panels
- generated files that were not inspected in full

Targeted inspection is acceptable when it covers the changed contract. The
report must describe it accurately.

---

## 19. Manual-edit policy

The migration must be manually authored and reviewed.

Use read-only inventory commands such as:

```text
rg
find
git diff
git show
```

Do not write or run scripts that:

- rewrite prose
- convert contractions
- change punctuation globally
- generate Basic explanations
- split monolith paragraphs into concept files
- alter headings across the tree
- update generated HTML directly

Do not use Python, Node, Perl, shell rewrite loops or complex multi-file regular expressions for migration edits.

Edit one file at a time and inspect its complete diff.

---

## 20. Risks and controls

### 20.1 Replacement drift

**Risk:** A detailed replacement drifts from the monolith while the monolith remains authoritative.

**Control:** Keep the monolith read-only. Perform rule-by-rule parity review and record discrepancies.

### 20.2 Accidental protected-file edits

**Risk:** A worker follows an older plan and edits `AGENTS.md` or the monolith.

**Control:** List both as protected in every route plan. Run protected-file diffs before and after work. Treat any patch-created change as blocking.

### 20.3 Basic and detailed drift

**Risk:** A subtle semantic change updates only the detailed reference while Basic becomes false.

**Control:** Every detailed edit must answer:

> Does this change the stable mental model described in the paired Basic file?

Update Basic only when the answer is yes.

### 20.4 Page-level semantic duplication

**Risk:** Friendly page prose becomes another exact rule source.

**Control:** Keep page summaries stable and broad. Put exact rules in the detailed file.

### 20.5 Important information visible only in Basic

**Risk:** Selecting Advanced hides a unique example or rule.

**Control:** Preserve important monolith evidence in the detailed file. Review Advanced as a standalone reference.

### 20.6 Implementation accident becomes language design

**Risk:** A current lowering detail is documented as permanent semantics.

**Control:** Require an explicit semantic decision before adding implementation-derived behaviour to a detailed language contract.

### 20.7 Context-sensitive syntax is overgeneralised

**Risk:** A rule that is valid in ordinary code is described as universal even
though template or content contexts tokenize it differently.

**Control:** Check the owning tokenizer or parser context before using words
such as always or everywhere. Record explicit exceptions in the detailed
reference. Comment syntax inside template and Beandown bodies is the first
known example.

### 20.8 Closed compiler tables are copied incompletely

**Risk:** A detailed reference lists a few examples from a compiler-owned table
and silently omits supported rows.

**Control:** When the compiler owns a closed table, inspect the complete table
and migrate it as one unit. Cast policies, cast traits, operator precedence and
operator type policy are current examples.

### 20.9 Toggle group collisions

**Risk:** Selecting Advanced in one concept changes another concept.

**Control:** Require unique names and IDs for every component.

### 20.10 Hidden accessibility regression

**Risk:** Inputs are removed from keyboard or assistive access.

**Control:** Visually clip radio inputs. Never apply `display: none` to the inputs.

### 20.11 Imported Beandown content loses styling

**Risk:** Imported code blocks or tables render without the docs site's article styling.

**Control:** Keep shared code and table rules in `docs_content_css` and include it in both language and codebase theme heads.

### 20.12 Export-block regressions

**Risk:** A new helper exists in `docs.bst` but is not exported through the current root entry.

**Control:** Update the explicit export block in `docs/src/#page.bst`. Make further repairs only when a compiler diagnostic identifies the owner.

### 20.13 Oversized concepts

**Risk:** One toggle hides an entire long page.

**Control:** Split by semantic responsibility. Each concept should be independently understandable and useful to toggle.

### 20.14 Public tone becomes sterile

**Risk:** Moving semantics out of the page removes personality.

**Control:** Keep introductions, transitions, jokes and editorial examples in `#page.bst`.

### 20.15 Public tone becomes misleading

**Risk:** A joke obscures the real rule.

**Control:** Back every playful section with an accurate Basic or Advanced explanation.

### 20.16 Generated-output noise

**Risk:** A documentation build changes many tracked release files.

**Control:** Record the starting state, keep generated output, inspect its diff and report unrelated churn. Do not require a clean worktree after the build.

---

## 21. Current migration status

### 21.1 Loops prototype

The Loops route is the prototype for:

- granular Basic and Advanced pairs
- independent native-radio selectors
- stable concept anchors
- direct reading of unsuffixed files
- page-level Previous and Next navigation
- generated-output inspection

Its concept files are:

```text
conditional-loops.bd
conditional-loops-basic.bd
collection-loops.bd
collection-loops-basic.bd
range-loops.bd
range-loops-basic.bd
loop-control.bd
loop-control-basic.bd
```

The prototype does not transfer language authority.

### 21.2 Foundation lessons from the prototype

The prototype established several requirements for later patches:

- shared code-block styling must not depend on page-local helper identity
- shared table styling belongs in `docs_content_css`
- both language and codebase themes need the shared article stylesheet
- the pager needs two-sided, Previous-only and Next-only forms
- new docs helpers must be exported through the current explicit root export block
- strict export syntax may require narrow entry or library repairs
- Advanced direct-read parity must include important monolith examples
- high-level framing cannot live only in the public page
- the monolith and `AGENTS.md` remain protected
- generated release artifacts may remain in the workspace

### 21.3 Completed Functions, Branching, Structs and Choices batch

The completed batch covers:

- Functions
- Branching
- Structs
- Choices
- navigation from Casts through Errors
- focused-reference indexing for those routes

Review found three follow-up requirements:

- one literal-pattern example mixed unrelated scrutinee types
- option exhaustiveness needs its `none` plus present-capture exception
- full-match general capture and exhaustiveness are inconsistent in the current compiler

The first two are documentation corrections.

The third remains an implementation/design discrepancy. Until resolved,
`else =>` is the documented full-match catch-all.

### 21.4 Current Errors, Collections, Templates and Constants batch

The next batch covers:

- Errors, Options and Assertions
- Collections and Maps
- Templates
- Constants and Compile-Time Behaviour
- navigation through Aliases
- focused-reference indexing for those routes

It also codifies that Markdown pipe tables are unsupported in `.bd` and that
generated inspection reports must describe their actual coverage.

### 21.5 Implementation/design discrepancy ledger

Keep these visible until separately resolved. Classification:

| # | Surface | Monolith | Compiler | Classification |
|---|---|---|---|---|
| 1 | Parameter-alias return syntax `-> first or fallback` | Absent | Parser accepts | Parser-only / incomplete |
| 2 | Inline bound catch `catch \|err\| then expr` | Absent | Full frontend support | Confirmed current language |
| 3 | Inline choice predicate `if status is Ready then ...` | Absent | Full frontend support, invalid cross-choice variant not rejected | Confirmed current language with validation gap |
| 4 | Option payload equality | Accepted | Rejected by nested choice-payload equality query | Probable implementation gap |
| 5 | Template-backed `String` payload equality | Accepted | May be indistinguishable from ordinary `String` | Unresolved design |
| 6 | General capture exhaustiveness | Accepted as catch-all | Marks later arms unreachable but does not satisfy exhaustiveness | Probable implementation gap |
| 7 | Inline map nesting beyond two levels | Not discussed | Rejected with explicit diagnostic | Confirmed Alpha restriction |
| 8 | Raw string slices with backticks | Accepted | Rejected | Probable stale monolith |
| 9 | Error-only `return!` inside nested blocks | Accepted | Rejected | Probable implementation gap |
| 10 | Block value-producing `if` with `then` | Accepted | Infrastructure failure | Probable implementation gap |
| 11 | Stored named inserts passed as loose contributions | Accepted | Rejected | Probable implementation gap |
| 12 | Assert optional message arity | Required | Optional literal accepted | Confirmed current language |
| 13 | Option ordering operators | Not discussed | Rejected | Confirmed rejection |
| 14 | Function parameter default cross-parameter dependency | Not discussed | Rejected | Confirmed rejection |
| 15 | Unused generic parameter policy | Silent | Rejected with BST-RULE-0043 | Confirmed rejection |
| 16 | Aligned declaration-site generic receiver methods | Mentioned | Accepted with `type A \|this Box of A\|` syntax | Confirmed current language |
| 17 | Receiver methods on concrete generic instances | Rejected | Rejected with explicit diagnostic | Confirmed rejection |
| 18 | Nested inline `of` type application | Rejected | Rejected with explicit diagnostic | Confirmed Alpha restriction |
| 19 | Recursive generic nominal types | Rejected | Rejected at parser | Confirmed rejection |
| 20 | Explicit generic call-site syntax | Rejected | Rejected with explicit diagnostic | Confirmed rejection |
| 21 | Composed `This` forms in trait requirements | Rejected | Rejected with explicit diagnostic | Confirmed rejection |
| 22 | Trait conformance with semicolon terminator | Rejected | Rejected with explicit diagnostic | Confirmed rejection |
| 23 | `where` clauses for generic bounds | Rejected | Rejected with explicit diagnostic | Confirmed rejection |
| 24 | Trait names as ordinary value types | Rejected | Rejected with explicit diagnostic | Confirmed rejection |
| 25 | Reactive parameter default values | Rejected | Rejected with explicit diagnostic | Confirmed rejection |
| 26 | Reactive syntax in struct fields, choice payloads or return slots | Rejected | Rejected with explicit diagnostic | Confirmed rejection |
| 27 | Field, call, expression or mutable-access template subscriptions | Rejected | Rejected with explicit diagnostic | Confirmed rejection |
| 28 | Empty or multiple subscription arguments | Rejected | Rejected with explicit diagnostic | Confirmed rejection |
| 29 | Whitespace-separated `$ (source)` subscription syntax | Rejected | Rejected with invalid-character diagnostic | Confirmed rejection |
| 30 | HTML-Wasm runtime support for reactive features | Not discussed | Rejected before lowering | Confirmed target gate |
| 31 | Alias name as constructor for struct/choice target | Not discussed | Rejected with BST-RULE-0037; only canonical name constructs | Confirmed rejection |
| 32 | Nested `of` inside collection element annotation | Not discussed | Rejected with BST-SYNTAX-0015; single `of` in element is accepted | Confirmed Alpha restriction |
| 33 | Concrete callee error solving generic error parameter | Not discussed | Rejected; parameter must be solved by immediate evidence | Confirmed rejection |

Ordinary documentation migration patches do not authorise compiler changes for
these discrepancies.

### 21.6 Aliases, Generics, Traits and Reactivity batch

This batch landed at commit `10ee84b29a1fd8a3cd24f7f9fef9fb129abf3643`.

Route pairs landed:

- Aliases: `type-aliases`, `import-aliases`, `payload-capture-aliases`
- Generics: `generic-declarations`, `type-application`, `generic-inference`, `generic-instances`, `generic-limits`
- Traits: `trait-declarations`, `trait-requirements`, `conformance`, `generic-trait-bounds`, `trait-incompatibility`, `core-cast-traits`, `trait-design-scope`
- Reactivity: `reactive-sources`, `subscriptions`, `reactive-parameters`, `mutation-and-invalidation`, `runtime-sinks`, `reactivity-scope`

Generated pages built and inspected for all four routes.

A correction pass was required because the implementation agent hit its usage limit during final
review. The correction pass repaired the following findings:

#### Review findings

- alias constructor spelling: the blanket claim "type aliases are not constructors" was suspect.
  Probes confirmed that only the target's canonical nominal name constructs; the alias spelling is
  rejected with `BST-RULE-0037` for both struct and choice aliases. Documentation updated with the
  exact rule.
- generic invalid placeholder constructor example: `parse_json` used `return A()`, which is not a
  valid generic operation. Replaced with the verified `empty()` / `consume()` nested-inference
  probe.
- generic nested `return!` example: `read_or_raise` used `return!` inside an `if` block, which the
  compiler rejects. Replaced with a top-level `always_fail` example that uses `return!` at function
  scope.
- broken intra-page concept links: detailed files used file-like links such as `@./type-application`
  instead of same-page anchors `@#type-application`. Cross-page concept links such as
  `@../traits/generic-trait-bounds` were corrected to `@../traits/#generic-trait-bounds`.
- stale `#mod.bst` wording: Aliases documentation referred to `#mod.bst` as a semantic facade.
  Corrected to use neutral module-root wording and link to Project Structure and Libraries.
- incomplete fallible cast conformance example: the Advanced `TRY_CASTABLE_TO_INT` example declared
  conformance without defining the required `try_to_int` method. Replaced with a complete verified
  implementation showing success with `return`, failure with `return!`, local recovery with
  `cast ... catch`, propagation with `cast!`, and the rejection of `cast! ... catch`.
- invalid Basic fallible cast example: the Basic `TRY_CASTABLE_TO_INT` example used `return 0,
  Error("not implemented")` instead of `return!`, and combined `cast!` with `catch`. Replaced with
  a coherent fallible method using `return!` and a local recovery call using `cast ... catch`.
- unhandled reactive collection mutation example: both Basic and Advanced used
  `~names.push("Grace")!` at top level, which is rejected because no error channel exists.
  Replaced with the handled `catch` form.

#### Verified discrepancy classifications

- unused generic parameter: the monolith is silent rather than affirmatively accepting unused
  parameters. The compiler rejects them with `BST-RULE-0043`.
- alias constructor spelling: verified. Only the target's canonical nominal name constructs. The
  alias spelling is rejected for structs, choices and builtins alike.
- collection-element nested `of`: verified. One inline `of` application inside a collection element
  annotation is accepted (such as `{Box of String}`). Nested `of` inside a collection element is
  rejected with `BST-SYNTAX-0015`, matching the general nested `of` restriction.
- generic error inference: verified. A concrete callee error type does not specialise an otherwise
  unsolved generic error parameter. The parameter must be solved by immediate declaration and
  call-site evidence.
- reactive metadata propagation: all listed paths (assignment, return, direct argument passing,
  ordinary `String` parameters inserted into templates) are deliberate and tested in the reactive
  template metadata test suite.

#### Confirmed current language or stale monolith

The previous verification pass confirmed the following current language facts and gaps, which
remain accurate:

- optional assert message
- inline bound catch
- error-only arrow syntax
- compile-time function defaults
- inline choice predicates
- fixed literal direct construction
- contextual empty maps
- current two-level inline map nesting behavior

#### Confirmed implementation gaps or absent forms

- nested-block `return!`
- block value-producing `if` with `then`
- raw backtick string slices
- stored named inserts

### Next route work

The Aliases, Generics, Traits and Reactivity batch is complete and has passed correction review.

Continue with:

1. Project Structure
2. Libraries and Imports
3. Beandown
4. Plain Markdown
5. Core-library language surfaces
6. Final whole-language parity and authority review

---

## 22. Completion criteria

The content migration is ready for final authority review only when:

- every monolith section has a detailed destination
- every normative rule has one planned unsuffixed owner
- every detailed concept has a paired `-basic.bd`
- every page imports both levels
- every concept has an independent selector
- Basic is the default
- detailed files are directly readable
- important monolith examples are preserved
- no detailed file identifies itself as being for agents
- no public page treats the reader as an LLM
- legitimate LLM-aware design and tooling references remain
- every language page has one H1
- every concept has a stable anchor
- every page has correct navigation
- the docs index reflects the learning sequence
- all routes build successfully
- generated HTML has been inspected
- every monolith section has passed parity review
- deferred and outside-scope behaviour is preserved
- implementation conflicts have been reported
- the progress matrix remains the implementation-status authority
- the roadmap remains the planning authority
- `docs/language-overview.md` remains unchanged
- `AGENTS.md` remains unchanged
- sidebar and glossary work can be added later without changing content ownership

At that point:

- the monolith still remains available
- the user performs or explicitly authorises the final authority switch
- `AGENTS.md` routing is updated only in that separate approved patch
- monolith retention or removal is decided separately

---

## 23. Required final report for each migration patch

Every implementation agent should report:

### Source changes

List every edited and created source file.

### Protected files

State that these remained untouched:

```text
AGENTS.md
docs/language-overview.md
```

Also list any other user-protected files named by the route plan.

### Parity result

State:

- which monolith headings were reviewed
- which detailed files received each rule
- whether any important example was replaced or omitted
- whether unresolved discrepancies remain

### Basic result

State that each Basic file is a manually written simplification consistent with its detailed partner.

### Import/export repairs

State either:

```text
No additional import/export repairs were required.
```

or list each repair, the compiler diagnostic that required it and the exact surface changed.

### Validation

Report the exact results of:

```sh
bean check docs
bean build docs --release
```

Do not claim commands that were not run.

### Generated output

List the generated routes inspected. State that generated artifacts were retained and that generated HTML was not edited manually.

### Editing method

State that:

- no migration script was written or run
- no automated multi-file prose replacement was used
- all prose and source changes were made manually, file by file

### Remaining uncertainty

Report semantic ambiguity, implementation conflict or incomplete parity honestly rather than hiding it.