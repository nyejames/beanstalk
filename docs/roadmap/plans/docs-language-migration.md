# Beanstalk language documentation migration — design brief

## 1. Purpose

This migration will replace `docs/language-overview.md` as the monolithic language authority with a set of focused Beandown references embedded in the public documentation site.

The new structure must serve three jobs without forcing them into the same prose:

1. **`#page.bst` entry files** provide the public website experience: structure, introductions, examples, personality, visual layout, concept ordering and navigation.
2. **`<concept>-basic.bd` files** teach each concept slowly with definitions, mental models and plenty of examples while avoiding unstable edge cases and excessive technical detail.
3. **`<concept>.bd` files** are the canonical semantic references. They describe exact language behavior, restrictions, edge cases, inference boundaries, deferred behavior and outside-scope decisions.

The canonical file intentionally has **no `-advanced` suffix**. Once migrated, it is the normal source of truth for that language concept. `-basic` marks the distilled teaching version.

This extends the structural precedent established by the codebase migration: presentation belongs to the page entry while reusable technical content belongs in Beandown. The current tracked `AGENTS.md`, not the earlier migration snapshots, remains authoritative for repository behavior. 

---

# 2. Fixed design decisions

These decisions should not be reinterpreted independently by later implementation plans.

| Area                         | Decision                                                                          |
| ---------------------------- | --------------------------------------------------------------------------------- |
| Canonical semantics          | `<concept>.bd`                                                                    |
| Beginner teaching            | `<concept>-basic.bd`                                                              |
| Website composition          | `#page.bst` imports both                                                          |
| Website default              | Basic                                                                             |
| Alternate website level      | Advanced                                                                          |
| Toggle granularity           | One toggle per concept, not one per page                                          |
| Toggle implementation        | Two native radios with CSS `:checked` selectors                                   |
| JavaScript                   | None for the initial implementation                                               |
| ARIA tabs                    | Do not use                                                                        |
| Page heading                 | One H1 per page                                                                   |
| Concept heading              | One stable heading outside both variants                                          |
| Navigation                   | Explicit Previous and Next page links                                             |
| Sidebar and glossary         | Designed for later, not implemented now                                           |
| Cross-page level persistence | Not implemented now                                                               |
| Migration cadence            | Small, reviewable patches                                                         |
| Monolith removal             | Only after complete parity and authority review                                   |
| Public LLM references        | Allowed where they genuinely describe the language, tooling or project philosophy |
| Reader treatment             | Never address or route the reader as though they might be an agent or LLM         |

---

# 3. Goals

## 3.1 Semantic goals

The migration must:

* preserve every accepted rule currently owned by `docs/language-overview.md`
* reconcile stale website material against the current language authority
* give every normative rule one canonical home
* distinguish accepted semantics from current implementation coverage
* distinguish deferred work from intentionally excluded language design
* keep the exact behavior easy to update when compiler semantics change
* make canonical references efficient to read directly without loading page presentation

`docs/language-overview.md` currently combines syntax, semantics, edge cases, deferred features, project rules and design-scope decisions across a very large reference. That breadth makes it a useful migration ledger, but a poor long-term unit of ownership.

## 3.2 Public documentation goals

The website must:

* offer an approachable path through the language
* begin with concepts and examples rather than edge-case inventories
* let the reader switch a single concept to Advanced without changing the rest of the page
* keep headings, anchors and page navigation stable while switching
* provide enough examples that a beginner can learn without opening Advanced
* expose detailed behavior without sending readers to a separate maintainer site
* retain Beanstalk's playful, conversational voice
* maintain one clear learning sequence across pages

## 3.3 Maintenance goals

The structure should reduce duplicated work:

* subtle compiler behavior normally changes only the canonical `.bd`
* a basic file changes only when the high-level model changes
* page copy changes only when presentation, tone or navigation changes
* exact semantic details do not need to be repeated in page introductions
* editorial jokes do not become normative language rules
* future navigation work does not require rewriting concept content

The basic files may be **longer** than the canonical files because examples and beginner explanations take space. They should be semantically narrower, not necessarily shorter.

---

# 4. Non-goals

This project will not initially:

* migrate every language route in one patch
* add a global Basic or Advanced preference
* remember the selected level across page navigation
* add JavaScript for toggle behavior
* implement the planned glossary
* implement the planned responsive sidebar
* redesign the complete documentation theme
* move compiler architecture into language references
* delete `docs/language-overview.md` before complete parity review
* treat the current website pages as reliable semantic sources
* convert the async design draft into supported-language documentation
* generate basic files mechanically from canonical files
* use automated prose rewriting across the documentation tree

---

# 5. Content authority model

## 5.1 Final authority hierarchy

After migration, authority should be:

| Source                                        | Responsibility                                            |
| --------------------------------------------- | --------------------------------------------------------- |
| Canonical `<concept>.bd`                      | Exact language semantics                                  |
| `<concept>-basic.bd`                          | Simplified explanation consistent with the canonical file |
| `#page.bst`                                   | Website structure, tone, navigation and editorial context |
| `docs/src/docs/codebase/language/overview.bd` | Routing map to canonical concept references               |
| Progress matrix                               | Current implementation and backend coverage               |
| Roadmap                                       | Sequencing, active proposals and unaccepted work          |
| Compiler-design docs                          | Compiler stage ownership and implementation contracts     |
| Memory docs                                   | Access, borrowing, ownership and lowering architecture    |

A `#page.bst` file is never the canonical location for subtle semantic rules. It may summarize stable ideas, but a rule that another compiler contributor needs to implement must have a canonical `.bd` owner.

A basic file is not an independent specification. It can omit complexity, but it cannot simplify a rule into something false.

## 5.2 Authority during incremental migration

A full authority switch at the end would require maintaining two complete semantic sources throughout the project. Avoid that.

Authority should transfer **concept by concept**.

### Migration routing index

Repurpose:

```text
docs/src/docs/codebase/language/overview.bd
```

as the language authority map.

During migration it should identify:

* concepts already owned by canonical Beandown files
* their canonical paths
* concepts still owned by `docs/language-overview.md`
* directly related language and codebase references

It remains a neutral technical routing document. It should not talk about agents or identify reader types.

### Per-concept handoff

A concept patch transfers authority only when all of these land together:

1. Canonical `<concept>.bd`
2. Paired `<concept>-basic.bd`
3. Updated `#page.bst`
4. Updated language authority map
5. Updated `docs/language-overview.md` section
6. Relevant links and navigation
7. Successful generated-output review

The migrated monolith section should be replaced with a concise canonical pointer rather than retaining a second full copy:

```markdown
## Loops

Canonical loop semantics are maintained in:

`docs/src/docs/loops/loops.bd`
```

This prevents two active copies from drifting.

### First authority-routing patch

The first migration patch that transfers a concept should update `AGENTS.md` so language work follows this sequence:

1. Read `docs/src/docs/codebase/language/overview.bd`
2. Read the canonical concept files it selects
3. Use `docs/language-overview.md` only for concepts not migrated yet
4. Use the progress matrix for current support

The current tracked rules still identify the monolith as the remaining language authority, so this change is necessary once the first concept moves.

## 5.3 Resolving semantic conflicts

For each concept, review evidence in this order:

1. Explicit user decisions for the migration
2. Current accepted semantics in `docs/language-overview.md`
3. Relevant compiler and memory design references
4. Progress matrix for implementation coverage
5. Tests and implementation when documentation and behavior appear inconsistent
6. Existing website page only as non-authoritative teaching material

Do not silently canonize an accidental compiler behavior.

When implementation conflicts with accepted design, the patch must identify the conflict. The canonical language file should record the accepted behavior. The progress matrix should explain current support where relevant.

---

# 6. File and naming architecture

## 6.1 One pair per concept

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

This is intentionally more granular than:

```text
errors.bd
errors-basic.bd
```

A whole-page pair would permit only one toggle for the entire page.

## 6.2 Naming rules

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

The unsuffixed file is canonical.

## 6.3 Import aliases

Use explicit aliases that preserve the relationship:

```beanstalk
import @./mutable-access {
    content as mutable_access,
}

import @./mutable-access-basic {
    content as mutable_access_basic,
}
```

Do not use generic aliases such as:

```beanstalk
content
advanced
basic
reference
```

when a page imports several concepts.

## 6.4 Concept keys

Every toggle receives a manually authored stable key:

```text
mutable-access
error-propagation
fixed-collections
```

The key owns:

* the stable concept anchor
* radio input IDs
* the radio group name
* future sidebar deep links
* future glossary links

Keys must:

* be unique within the page
* use lowercase ASCII kebab case
* remain stable even when the displayed heading changes
* never be derived automatically from heading text

## 6.5 Heading rules

Each page has exactly one H1.

Each toggled concept has one stable H2 outside both variants.

The basic and canonical Beandown files:

* must not begin with H1 or H2
* may use H3 and deeper headings
* should begin with a definition or contract paragraph
* must not repeat the concept title
* must not own page navigation

This keeps the direct source readable while preventing duplicate page headings.

---

# 7. Responsibility of each layer

## 7.1 `#page.bst`

The page entry owns:

* imports
* browser title
* page description
* theme head
* navbar
* breadcrumb
* the page's single H1
* friendly introduction
* page-level mental model
* ordering of concepts
* toggle component calls
* transition prose between concepts
* optional editorial examples
* jokes and conversational asides
* related pages
* Previous and Next navigation

It should not own:

* exhaustive syntax rules
* inference boundaries
* backend-specific edge-case matrices
* precise rejected forms
* normative deferred or outside-scope inventories
* compiler implementation paths
* duplicate copies of canonical examples

The page can say:

> Collections are Beanstalk's ordered groups of values. They use braces, not square brackets, because square brackets have a more interesting job.

The exact fixed-capacity identity and inference rules belong in the canonical files.

## 7.2 `<concept>-basic.bd`

The basic file teaches.

It should usually contain:

1. A plain definition
2. Why the concept exists
3. The smallest useful example
4. A step-by-step explanation
5. A few progressively richer examples
6. Common mistakes
7. A stable high-level rule to remember

Basic content should:

* introduce terminology before using it
* avoid assuming familiarity with several other languages
* use realistic names and examples
* explain punctuation that may look unusual
* avoid exhaustive edge-case lists
* avoid compiler implementation terms
* avoid backend details unless the user must act differently
* avoid large support-status tables
* be accurate without being exhaustive

A basic file may include many examples. That is expected.

## 7.3 Canonical `<concept>.bd`

The canonical file specifies.

It should usually contain:

1. A concise definition or contract
2. Canonical syntax forms
3. Exact semantic rules
4. Type and inference behavior
5. Receiving-context or scope constraints
6. Mutation, access or ownership effects where relevant
7. Edge cases
8. Invalid and rejected forms
9. Deferred behavior
10. Outside-scope behavior
11. Links to adjacent canonical concepts

Canonical content should:

* use precise normative language
* prefer concise rule lists over long teaching prose
* preserve exact terminology
* include representative examples, not every possible beginner example
* distinguish “invalid”, “deferred” and “outside scope”
* describe backend-visible restrictions only when they affect language behavior
* link to the progress matrix rather than becoming a status dashboard
* link to codebase design docs rather than duplicating compiler architecture
* remain useful when opened directly from the repository

This file is what compiler maintainers and language-aware tools should normally load.

---

# 8. LLM references and public tone

There is **no global ban** on mentioning LLMs.

## Allowed

Keep references when they genuinely describe:

* Beanstalk's LLM-aware design goals
* terse diagnostic output being useful to LLM workflows
* tool integration
* historical jokes
* playful examples such as the deliberate strawberry misspelling
* reasons the language favors readability and constrained syntax

The comment about spelling strawberry incorrectly “to confuse LLMs” can remain as page-level personality.

The Getting Started statement that terse output is useful for LLM workflows can remain because it describes the tool.

## Not allowed

Remove or avoid wording that treats the reader as an agent:

* “Agents should read this section”
* “If you are an LLM”
* “This page is primarily for agents”
* “Humans can skip this”
* “Agent version”
* “Agent-only explanation”

The UI labels remain simply:

```text
Basic
Advanced
```

The canonical source is not described publicly as “the agent version”.

---

# 9. Toggle component design

## 9.1 Accepted interaction model

The initial implementation uses:

* one `fieldset`
* one visually hidden `legend`
* two native radios
* one shared radio `name`
* two unique radio IDs
* Basic checked by default
* labels styled as a segmented selector
* CSS sibling selectors to reveal one panel
* no JavaScript
* no ARIA tab roles

The supplied radio and CSS pattern is the accepted baseline. Implementation work may adapt names to Beanstalk's component API and shared theme tokens, but it must not redesign the interaction model.

## 9.2 Component location

Add the reusable component to:

```text
docs/src/styles/docs.bst
```

That file already owns the site theme, navbar, sections, tables, code blocks and title helpers, but currently has no explanation-level component.

Recommended additions:

```text
language_docs_css
language_theme_head
doc_level
doc_pager
```

### `language_docs_css`

Owns:

* explanation selector styles
* visually hidden utility
* selected label styles
* focus-visible styles
* panel visibility
* narrow-screen layout
* reduced-motion handling
* Previous and Next navigation styles

### `language_theme_head`

Composes:

```text
theme_head
language_docs_css
```

Do not place the language-only selector CSS into `codebase_theme_head`.

### `doc_level`

Conceptual interface:

```text
doc_level(
    key,
    title,
    basic_content,
    advanced_content
)
```

The exact Beanstalk function or template signature should be proven in the first patch.

The component must emit static HTML only.

## 9.3 Required markup contract

For a concept key `mutable-access`, generated markup should follow this shape:

```html
<fieldset class="doc-level">
    <legend class="visually-hidden">
        Choose the explanation level for mutable access
    </legend>

    <input
        class="doc-level__input doc-level__input--basic"
        type="radio"
        name="mutable-access-level"
        id="mutable-access-basic"
        value="basic"
        checked
    >

    <input
        class="doc-level__input doc-level__input--advanced"
        type="radio"
        name="mutable-access-level"
        id="mutable-access-advanced"
        value="advanced"
    >

    <header class="doc-level__header">
        <h2 class="doc-level__title" id="mutable-access">
            Mutable access
        </h2>

        <div class="doc-level__switch">
            <label
                class="doc-level__option doc-level__option--basic"
                for="mutable-access-basic"
            >
                Basic
            </label>

            <label
                class="doc-level__option doc-level__option--advanced"
                for="mutable-access-advanced"
            >
                Advanced
            </label>
        </div>
    </header>

    <div class="doc-level__panel doc-level__panel--basic">
        ...
    </div>

    <div class="doc-level__panel doc-level__panel--advanced">
        ...
    </div>
</fieldset>
```

The sibling order is part of the component contract because the CSS relies on `~` selectors.

## 9.4 Accessibility requirements

The component must:

* use native radio semantics
* group each pair with `fieldset` and `legend`
* visually clip inputs rather than applying `display: none`
* provide a visible `:focus-visible` indicator on the matching label
* maintain sufficient contrast in both color schemes
* let normal radio keyboard behavior select Basic or Advanced
* remove the inactive panel from layout and the accessibility tree through `display: none`
* keep the heading stable during switching
* avoid `role="tab"`, `role="tabpanel"` and related ARIA tab attributes

Without CSS, both explanations may appear. That is an acceptable fallback because all content remains available.

## 9.5 Unique group rule

Within one component:

```text
name="mutable-access-level"
```

is shared by its two inputs.

Across components, names must differ:

```text
mutable-access-level
explicit-copies-level
shared-access-level
```

Otherwise selecting Advanced in one concept would reset another concept.

## 9.6 Default and persistence

Basic is the authored default:

```html
checked
```

The first version does not remember selection:

* across page navigation
* after a reload
* across different concept selectors

A later JavaScript enhancement may introduce a shared preference. It must remain progressive enhancement over this native-radio implementation.

## 9.7 Visual direction

Keep the supplied restrained design:

* small segmented selector
* Beanstalk green for selected text and subtle outlines
* no giant Basic and Advanced buttons
* no large card around every explanation
* selector beside the heading on wide screens
* selector below the heading on narrow screens
* identical code-block and callout treatment in both variants
* Advanced may be denser, but not smaller or lower contrast
* transitions disabled when reduced motion is preferred

---

# 10. Page composition contract

A migrated page should read like one coherent article.

Conceptual structure:

```text
navbar
breadcrumb
H1
friendly introduction
page map or opening example

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

Example composition shape:

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
    [doc_level(
        key = "mutable-access",
        title = "Mutable access",
        basic_content = mutable_access_basic,
        advanced_content = mutable_access,
    )]
]

#[doc_pager(
    previous_path = "../language-overview",
    previous_title = "Language basics",
    next_path = "../numbers",
    next_title = "Numbers",
)]
```

This is a target composition model. The first component patch must confirm the cleanest valid Beanstalk signature before subsequent plans copy it.

---

# 11. Natural documentation flow

## 11.1 Canonical learning sequence

The initial linear path should be:

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
14. Constants and Compile-Time Behavior
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

Existing routes remain stable. New routes are:

```text
bindings
numbers
casts
constants
```

The current documentation index already groups the existing language routes, so it should be expanded rather than replaced with a completely different entry point.

## 11.2 Page-level Previous and Next navigation

Every page in the sequence gets an explicit pager.

The pager should render:

```text
Previous
Page title

Next
Page title
```

Use a semantic `<nav>` with an accessible label such as:

```html
<nav class="doc-pager" aria-label="Language documentation">
```

Do not infer adjacency from directories. Each page explicitly declares its neighbors.

The first page may have only Next. The final language page may point onward to project documentation.

## 11.3 Concept anchors

Each concept heading has a stable ID.

Example:

```text
/docs/bindings/#shared-access
/docs/errors/#catch-and-recovery
/docs/templates/#template-slots
```

These anchors become the future integration points for:

* sidebar navigation
* glossary links
* cross-page references
* search results
* copied deep links

## 11.4 Future sidebar compatibility

The initial implementation must not add the sidebar or hamburger menu.

It should prepare for them by preserving:

* stable route order
* stable page titles
* stable concept keys
* one H1 per page
* one H2 per toggle concept
* explicit Previous and Next relationships

No unused sidebar markup or JavaScript should be added yet.

---

# 12. Proposed concept ownership map

This is the initial taxonomy. A concept may be split further during migration if its canonical rules are too broad for one independent toggle. Two entries should be merged only when presenting them separately would be artificial.

## 12.1 Language Basics

Route:

```text
docs/src/docs/language-overview/
```

Concept pairs:

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

* block punctuation
* statement shape
* comments
* naming conventions
* primitive value forms
* strings, raw strings and characters
* syntax tour

Does not own:

* mutable access
* detailed numeric behavior
* template semantics
* project structure

## 12.2 Values and Bindings

New route:

```text
docs/src/docs/bindings/
```

Concept pairs:

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

Owns:

* declaration forms
* reassignment
* mutability
* shared access
* explicit copying
* no-shadowing semantics
* distinction between binding mutability and call-site exclusive access

The exact ownership and lowering strategy remains in memory/codebase docs. The language page owns observable source semantics.

## 12.3 Numbers

New route:

```text
docs/src/docs/numbers/
```

Concept pairs:

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

Owns:

* `Int`
* `Float`
* literal grammar
* operator spacing
* arithmetic result types
* integer and real division
* overflow and other numeric failures
* finite-float rules

## 12.4 Casts

New route:

```text
docs/src/docs/casts/
```

Concept pairs:

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

Owns:

* typed-boundary target selection
* `cast`
* `cast!`
* `cast ... catch`
* supported builtin targets
* invalid target contexts
* string and numeric conversion behavior
* compiler-owned cast traits

## 12.5 Functions

Existing route:

```text
docs/src/docs/functions/
```

Concept pairs:

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

Owns:

* signatures
* parameters
* default arguments
* named arguments
* call ordering rules
* success returns
* multiple returns
* immediate call access syntax

Shared and exclusive access semantics remain canonically owned by the Bindings page and are linked rather than duplicated.

## 12.6 Branching

Existing route:

```text
docs/src/docs/branching/
```

Concept pairs:

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

Owns:

* statement `if`
* `else`
* value-producing `if`
* `then`
* full match syntax
* patterns
* captures
* guards
* exhaustiveness
* bodyless fallback arms

Catch recovery uses value-producing blocks but remains canonically owned by Errors.

## 12.7 Loops

Existing route:

```text
docs/src/docs/loops/
```

Concept pairs:

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

Owns:

* conditional loops
* collection iteration
* numeric ranges
* inclusive and exclusive bounds
* inferred direction
* `by`
* index bindings
* `break`
* `continue`

This is the recommended prototype route because it has several independent concepts, useful code examples and a bounded semantic surface.

## 12.8 Structs

Existing route:

```text
docs/src/docs/structs/
```

Concept pairs:

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

Owns:

* nominal identity
* fields
* defaults
* construction
* field access
* receiver methods
* receiver ownership restrictions
* mutable receiver calls

## 12.9 Choices

Existing route:

```text
docs/src/docs/choices/
```

Concept pairs:

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

Owns:

* unit and payload variants
* construction
* immutable payloads
* payload matching
* structural equality
* unsupported equality payloads

Generic declaration syntax is introduced here but canonically specified under Generics.

## 12.10 Errors, Options and Assertions

Existing route:

```text
docs/src/docs/errors/
```

Concept pairs:

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

Owns:

* `Error`
* error return slots
* `return!`
* postfix `!`
* `catch`
* recovery with `then`
* optional values
* postfix `?`
* assertions
* expected failure versus invariant failure

General value-producing-block syntax remains owned by Branching.

## 12.11 Collections and Maps

Existing route:

```text
docs/src/docs/collections/
```

Concept pairs:

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

Owns:

* collection type and literal forms
* empty-literal inference
* growable collections
* fixed capacity and type identity
* fallible builtins
* map key restrictions
* insertion order
* mutation and access behavior

The page may keep playful examples, including the strawberry joke.

## 12.12 Templates

Existing route:

```text
docs/src/docs/templates/
```

Concept pairs:

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

Owns:

* template head and body
* capture
* directives
* default, named and positional slots
* inserts
* `$children`
* `$fresh`
* template `if`
* template `loop`
* Markdown formatting
* compile-time versus runtime template behavior

Builder page-fragment assembly remains owned by Project Structure. Const-template folding rules are also linked from Constants.

## 12.13 Constants and Compile-Time Behavior

New route:

```text
docs/src/docs/constants/
```

Concept pairs:

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

Owns:

* `#` bindings
* immutability
* foldability
* dependency and source-order rules
* const records
* compile-time template forms
* const template loop limits

Project-entry fragment positioning remains owned by Project Structure.

## 12.14 Aliases

Existing route:

```text
docs/src/docs/aliases/
```

Concept pairs:

```text
type-aliases.bd
type-aliases-basic.bd

import-aliases.bd
import-aliases-basic.bd

payload-capture-aliases.bd
payload-capture-aliases-basic.bd
```

Owns:

* transparent type aliases
* import renaming
* collision rules
* facade export alias distinction
* match payload capture aliases

## 12.15 Generics

Existing route:

```text
docs/src/docs/generics/
```

Concept pairs:

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

Owns:

* declaration-site parameters
* `of`
* concrete generic aliases
* generic functions
* immediate inference
* instance restrictions
* rejected and outside-scope generic surfaces

Trait-bound semantics are canonically owned by Traits and linked here.

## 12.16 Traits

Existing route:

```text
docs/src/docs/traits/
```

Concept pairs:

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

Owns:

* trait contracts
* `This`
* `~This`
* explicit conformance
* evidence visibility
* generic bounds
* incompatibility
* compiler-owned cast traits
* static versus runtime heterogeneity
* excluded trait-system complexity

## 12.17 Reactivity

Existing route:

```text
docs/src/docs/reactivity/
```

Concept pairs:

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

Owns:

* reactive declarations
* source identity
* snapshot reads
* subscriptions
* function boundaries
* invalidation
* live sinks
* backend restrictions
* deferred reactivity
* relationship to closures and function values

## 12.18 Project Structure

Existing route:

```text
docs/src/docs/project-structure/
```

Concept pairs:

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

Owns:

* project config
* entry roots
* module entry files
* facades
* runtime `start`
* page fragment behavior
* output routes
* build folders

## 12.19 Libraries and Imports

Existing route:

```text
docs/src/docs/libraries/
```

Concept pairs:

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

Owns:

* import syntax
* namespaces
* source libraries
* facades
* builder libraries
* external packages
* JavaScript import metadata
* visibility and collisions

## 12.20 Beandown and Markdown

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

Beandown files already compile into a generated `content #String` surface and are intended as imported content rather than standalone page entries.

---

# 13. Single-owner rule

Every normative fact must have one canonical owner.

Examples:

| Rule                                                         | Canonical owner                                                           |
| ------------------------------------------------------------ | ------------------------------------------------------------------------- |
| Existing values use shared access by default                 | `bindings/shared-access.bd`                                               |
| A function call uses `~place` for existing mutable arguments | `functions/calls-and-access.bd`, with access semantics linked to Bindings |
| `then` targets a value-producing block                       | `branching/value-producing-if.bd` or a shared value-block concept         |
| `then` inside `catch` recovers success values                | `errors/catch-and-recovery.bd`                                            |
| Choice payload aliases use `as`                              | `aliases/payload-capture-aliases.bd`                                      |
| Payload pattern shape                                        | `branching/patterns-and-exhaustiveness.bd`                                |
| Generic bounds use traits                                    | `traits/generic-trait-bounds.bd`                                          |
| Generic declaration and inference                            | `generics/*.bd`                                                           |
| Const templates fold                                         | `constants/const-templates.bd`                                            |
| Entry fragments are assembled into pages                     | `project-structure/page-fragments.bd`                                     |
| Template loop syntax                                         | `templates/template-control-flow.bd`                                      |
| Ordinary loop syntax                                         | `loops/*.bd`                                                              |

Other pages may summarize and link, but must not carry a second exhaustive rule list.

---

# 14. Writing standards

## 14.1 Shared prose rules

Use:

* straight apostrophes
* natural contractions
* varied sentence lengths
* direct examples
* concise headings
* friendly confidence
* exact code syntax

Avoid:

* em dashes
* curly apostrophes
* prose semicolons
* unnecessary Oxford commas
* “however”, “therefore” and similar filler transitions
* long document-mechanics preambles
* vague “currently supported” language where a precise rule is available
* visible template escape artifacts

## 14.2 Basic files

Prefer:

> A mutable binding is a name whose value can change. Add `~` when you create it, then use normal `=` when assigning a new value.

Avoid:

> Mutability is represented as an access-mode property orthogonal to semantic type identity.

That belongs in the canonical file.

## 14.3 Canonical files

Prefer:

> `~` on a declaration marks the binding as mutation-capable. Reassignment still uses `=`. Binding mutability is separate from call-site exclusive access and is not part of semantic type identity.

Avoid:

> Think of `~` as making the variable more flexible.

## 14.4 Page files

Page prose can be more expressive:

> Square brackets already have a full-time job. They build templates, so collections use braces instead.

Jokes should not become the only explanation of a rule.

## 14.5 Examples

Examples in basic files should:

* compile where presented as valid
* use one new concept at a time
* progress from small to realistic
* avoid unrelated advanced features
* use comments to explain intent

Examples in canonical files should:

* be compact
* demonstrate exact boundaries
* include invalid forms only when the rejection matters
* show accepted syntax precisely
* avoid decorative noise

Page-only examples may carry more personality, but unique semantic evidence belongs in the Beandown files.

---

# 15. Migration ledger

Maintain a section-level ledger for the duration of the project.

Recommended location if committed:

```text
docs/roadmap/language-documentation-migration.md
```

The roadmap is the appropriate owner because this is sequencing and active migration work.

Each ledger row should record:

| Field                      | Meaning                  |
| -------------------------- | ------------------------ |
| Monolith heading           | Original source section  |
| Canonical target           | Unsuffixed `.bd`         |
| Basic target               | `-basic.bd`              |
| Website route              | Importing `#page.bst`    |
| Related owner              | Any canonical cross-link |
| Semantic review            | Pending or complete      |
| Authority moved            | Yes or no                |
| Monolith pointer installed | Yes or no                |
| Generated route inspected  | Yes or no                |

The ledger must cover:

* every normative paragraph
* every table row
* every syntax form
* every important valid example
* every rejected example
* every deferred feature
* every outside-scope statement

A section is not complete merely because its prose was copied somewhere.

---

# 16. Patch strategy

## 16.1 Baseline patch

Before migration output is reviewed:

1. Confirm `main` is clean
2. Run `bean check docs`
3. Run `bean build docs --release`
4. Commit any legitimate generated synchronization separately
5. Establish a clean generated-output baseline

This prevents old generated drift from obscuring migration diffs.

## 16.2 Foundation and prototype patch

The first substantive patch should include:

* `language_docs_css`
* `language_theme_head`
* `doc_level`
* `doc_pager`
* stable concept IDs
* one migrated route
* initial authority routing changes
* Previous and Next navigation
* generated route inspection

**Loops is the recommended prototype.**

It exercises:

* several independent toggles
* Basic and Advanced code examples
* desktop and mobile selector layout
* a page-level learning sequence
* a bounded canonical semantic surface
* enough content to prove direct reading

The first patch should not simultaneously rewrite Language Basics, Templates or Errors.

## 16.3 Route migration patches

After the prototype is accepted, migrate one route per patch unless two routes are inseparable.

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

Loops is already complete if used for the prototype.

This order builds conceptual dependencies before pages that rely on them. Errors moves ahead of Collections because collection operations are fallible.

## 16.4 Final authority patch

After every ledger row is complete:

* audit all canonical files against the original monolith
* audit deferred and outside-scope rules
* update the language authority map
* update `AGENTS.md` to remove the monolith fallback
* convert `docs/language-overview.md` into a legacy index or consolidated notice
* update README and documentation links
* regenerate the entire site
* inspect all language routes
* retain or remove the legacy monolith only after explicit approval

---

# 17. Per-patch implementation contract

Every route migration plan should require these steps.

## 17.1 Read and reconcile

Read:

* monolith sections
* current website page
* relevant compiler design references
* relevant memory references
* progress rows
* implementation and tests when rules appear inconsistent

Record conflicts before writing.

## 17.2 Define canonical ownership

List the concepts and assign each normative rule to one unsuffixed `.bd`.

Do not start prose until the owner map is clear.

## 17.3 Write canonical files first

The canonical files establish semantics.

Review them for:

* completeness
* exact syntax
* normative language
* edge cases
* rejected forms
* deferred behavior
* outside scope
* correct cross-links

## 17.4 Write basic files from the canonical contract

The basic file should be authored manually.

Do not mechanically shorten the canonical file.

Check that every statement remains true even though details are omitted.

## 17.5 Rewrite the page entry

The page should:

* own one H1
* introduce the page
* order the concepts
* provide transitions
* call `doc_level` for each pair
* preserve appropriate personality
* add related links
* add Previous and Next navigation

## 17.6 Transfer authority

In the same patch:

* update language authority map
* replace migrated monolith sections with canonical pointers
* update relevant links
* update `AGENTS.md` routing when required

## 17.7 Validate

For documentation-only patches:

```sh
bean check docs
bean build docs --release
```

Do not run the full Rust validation gate unless implementation code changes.

## 17.8 Inspect generated output

Inspect:

* one H1
* concept heading levels
* Basic selected by default
* Advanced fully replaces Basic
* keyboard focus
* radio exclusivity per component
* independence between components
* unique IDs
* no ARIA tab roles
* desktop layout
* narrow-screen layout
* dark mode
* reduced-motion behavior
* code blocks
* links
* Previous and Next navigation
* no manually edited HTML

---

# 18. Manual-edit policy

The migration should be manually authored and reviewed.

Scripts may help inventory:

* headings
* file paths
* repeated terms
* links
* monolith sections

Scripts must not:

* rewrite prose
* convert contractions
* change punctuation globally
* generate basic explanations
* split paragraphs into concept files
* alter headings across the tree
* update generated HTML directly

This avoids repeating the kind of grammatical regressions caused by mechanical prose conversion.

---

# 19. Risks and controls

## Duplicate authority

**Risk:** The monolith and canonical file both remain complete.

**Control:** Transfer one concept at a time and replace the monolith section with a pointer in the same patch.

## Basic and canonical drift

**Risk:** A subtle semantic change updates only the canonical file while the basic file becomes false.

**Control:** Every canonical edit must answer:

> Does this change the stable mental model described in the paired basic file?

If no, the basic file stays unchanged. If yes, update both.

## Toggle group collisions

**Risk:** Selecting Advanced in one concept changes another concept.

**Control:** Require a unique radio `name` and IDs for every component.

## Oversized concepts

**Risk:** A toggle becomes an entire long page hidden behind one selector.

**Control:** Split by semantic responsibility. A concept should be independently understandable and independently useful to toggle.

## Hidden accessibility regression

**Risk:** Inputs are visually removed in a way that prevents keyboard or assistive use.

**Control:** Clip inputs visually. Never use `display: none` on the radio inputs.

## Page-level semantics drift

**Risk:** Friendly page prose becomes another source of exact rules.

**Control:** Keep page summaries stable and broad. Move details into canonical files.

## Canonical files become compiler-design documents

**Risk:** Exact language references accumulate AST, HIR and backend implementation detail.

**Control:** Describe observable semantics and link to codebase references for implementation ownership.

## Public tone becomes sterile

**Risk:** Moving semantics out of the page removes all personality.

**Control:** Keep introductions, transitions, jokes and editorial examples in `#page.bst`.

## Public tone becomes misleading

**Risk:** Jokes obscure the real rule.

**Control:** Every playful section must still be backed by an accurate Basic or Advanced explanation.

## Generated-output noise

**Risk:** Existing stale generated files make reviews unreliable.

**Control:** Establish a clean release-build baseline before migration patches.

---

# 20. Completion criteria for the whole migration

The migration is complete only when:

* every monolith section has a canonical destination
* every normative rule has one owner
* every canonical concept has a paired `-basic.bd`
* every page imports both levels
* every concept has an independent selector
* Basic is the default
* canonical files are directly readable
* no canonical file identifies itself as being for agents
* no public page treats the reader as an LLM
* valid LLM-aware design and tooling references remain
* every language page has one H1
* every concept has a stable anchor
* every page has correct Previous and Next navigation
* the docs index reflects the learning sequence
* all routes build successfully
* generated HTML has been inspected
* the authority map points only to canonical files
* `AGENTS.md` routes language work through canonical files
* `docs/language-overview.md` no longer owns active semantics
* the progress matrix remains the implementation-status authority
* the roadmap remains the planning authority
* sidebar and glossary work can be added later without changing concept ownership

---

# 21. First implementation-plan target

The first agent implementation plan derived from this brief should cover only:

1. Clean generated-output baseline
2. `language_docs_css`
3. `language_theme_head`
4. `doc_level`
5. `doc_pager`
6. Loops concept split
7. Basic and Advanced radio behavior
8. Previous and Next links
9. Initial language authority-routing changes
10. Documentation check, release build and generated-page inspection

That patch proves the architecture. It should not begin the broad semantic migration until the component, file naming, direct-reading quality and website interaction have been manually accepted.
