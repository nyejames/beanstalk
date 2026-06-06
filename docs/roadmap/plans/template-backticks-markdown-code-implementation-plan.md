# Template Body Backticks, Markdown Inline Code, and Template Escape Removal Implementation Plan

## Goal

Make backticks inside template bodies ordinary formatter-visible text, then teach `$markdown` to render isolated paired single backticks as inline `<code>...</code>` spans. At the same time, remove template-body backslash escaping so `\` is ordinary template body text. Literal template delimiters must be inserted through normal template expressions, usually string literals such as `[: ["[code]"]]`.

This is a hard Alpha surface change. Do not add compatibility shims, deprecation diagnostics, or migration warnings.

## Current repo anchors

Verify these anchors at the start of implementation, because this repo is moving quickly:

- `src/compiler_frontend/tokenizer/lexer.rs`
  - Template body mode is handled before expression-position raw strings.
  - Normal template bodies route to `tokenize_template_body(...)` unless the current character is `[` or `]`.
  - Expression-position backticks still route to `tokenize_raw_string(...)` outside template-body mode.
- `src/compiler_frontend/tokenizer/text_modes.rs`
  - `tokenize_template_body(...)` currently treats `\` as a template-body escape prefix.
  - `append_template_body_escape(...)` drops the backslash and keeps only the escaped character.
  - `tokenize_string(...)` and `tokenize_raw_string(...)` must keep their current string-literal behavior.
  - `$code(...)`-style balanced template bodies use `tokenize_code_template_body(...)`, which already preserves backslashes literally except for bracket-balance bookkeeping.
- `src/compiler_frontend/ast/templates/template_formatting.rs`
  - Parent formatters receive text plus opaque anchors for child templates and dynamic expressions.
  - Do not flatten child template or dynamic expression output into `$markdown`.
- `src/compiler_frontend/ast/templates/styles/markdown/`
  - `$markdown` currently splits formatter input into `MarkdownLine` values and renders inline atoms through `render_inline_atoms(...)`.
  - `output.rs` owns `MarkdownOutputBuilder`, including escaped text and opaque-anchor output.
  - `tests/markdown_tests.rs` is the focused place for markdown formatter unit coverage.
- `docs/src/styles/docs.bst`
  - `codesnippet` is a docs-site helper/style, not a compiler feature.
  - Move the same inline-code look to normal `code` elements and delete `codesnippet` once no source imports use it.
- `docs/src/docs/progress/#page.bst`
  - The templates/style-directives row and Beandown row contain watch-point text that must be updated.
- `docs/language-overview.md`
  - The string-template section currently says `""` and backticks create string slices; refine this to expression-position backticks only.
  - The Beandown section currently documents escaped `]`; replace this with string-literal insertion guidance.
- `docs/roadmap/roadmap.md`
  - Replace the current TODO item for this work with a link to this plan if the plan is committed into `docs/roadmap/plans/`, then remove or update it when the implementation lands.

Before coding, run:

```bash
rg -n 'codesnippet|\\\\\[|\\\\\]|\\\\`|backtick|tokenize_template_body|append_template_body_escape|render_inline_atoms|TemplateBodyMode::Balanced|\$code' \
  src docs tests libraries
```

Use the result as the final current-state map. Do not depend on stale assumptions from this plan when the repo has moved.

## Agreed design decisions

### Template-body text

- Backticks inside template bodies are ordinary body text.
- Backticks in expression position still create raw string slices.
- `\` inside template bodies is ordinary body text.
- Template-body lexing must no longer remove backslashes before `[`, `]`, `` ` ``, `n`, `t`, `*`, or any other character.
- Normal quoted string literals keep escape behavior everywhere else in the language.
- Literal `[` / `]` in template body output must be authored through inserted expressions:

```beanstalk
[: ["["]]
[: ["]"]]
[: ["[code]"]]
```

- `.bd` content follows the same rules because it uses the same implicit `$markdown` template machinery.

### `$markdown` inline code

- `$markdown` treats paired isolated single backticks on the same markdown line as inline code.
- The output shape is plain `<code>...</code>` with no class.
- Inline code contents preserve whitespace exactly and HTML-escape parent-authored text.
- Markdown formatting does not run inside code spans.
- Inline code is parsed before emphasis and links for span contents, but it can appear inside already-open emphasis:

```html
<p><em>Use <code>Thing</code> here</em></p>
```

- Empty spans are not supported. Consecutive backtick runs such as `` `` `` or `` ``` `` render literally.
- Unmatched single backticks render literally.
- Backticks do not create multiline spans.
- CommonMark fences, variable-length code-span delimiters, multiline code spans, and markdown-level backtick escaping are not part of Beanstalk's markdown flavour.
- Dynamic expression anchors may appear inside a parent-authored code span. `$markdown` emits `<code>`, the opaque dynamic anchor, and `</code>`.
- `$markdown` must not inspect dynamic expression output. Backticks produced by dynamic expressions do not participate in inline-code parsing.
- Child-template anchors are hard barriers. Code spans must not contain child templates or pair across them.
- Inserted string literals are the canonical way to include literal template syntax inside markdown inline code, but they remain normal opaque expression anchors. Do not special-case constant string inserts as markdown-visible text.

### Docs and styling

- Replace authored docs usage of `[codesnippet: ...]` / `[codesnippet, ...]` with markdown inline backticks where the snippet is small.
- Use `$code(...)` for anything beyond a tiny inline expression or very small declaration.
- Do not introduce triple-backtick markdown fences.
- Do not keep `codesnippet` as an escape hatch.
- Move the docs-site inline-code styling from `codesnippet` to ordinary `code` CSS.
- Avoid changing compiler `$code(...)` output unless it lacks a stable selector needed for docs CSS.
- It is acceptable if docs inline `code` CSS also affects `$code(...)` blocks, as long as the result remains readable and consistent.
- Run `cargo run build docs --release` after source docs/style changes and include generated release output if the repo tracks it.

## Non-goals

- No new syntax for escaping template body characters.
- No markdown-level escaped backticks.
- No CommonMark fenced code blocks.
- No CommonMark variable-length code spans.
- No multiline inline-code spans.
- No formatter pass that flattens child templates or dynamic expression output.
- No new AST/HIR template representation for inline code.
- No new diagnostics for the old template-body escape behavior.
- No compatibility wrapper for `[codesnippet: ...]`.

## Implementation phases

## Phase 0 — Current-state audit and baseline

### Context

The implementation touches tokenizer text modes, markdown inline rendering, docs styling, docs source content, the progress matrix, and generated docs artifacts. Start by confirming the exact current shape so the implementation stays anchored to the repository rather than this plan's snapshot.

### Steps

- [ ] Run the repository search command from **Current repo anchors**.
- [ ] Inspect every result for:
  - [ ] template-body escape handling;
  - [ ] markdown inline rendering helpers;
  - [ ] existing markdown unit-test helpers;
  - [ ] docs `codesnippet` imports/usages;
  - [ ] docs references to escaped template delimiters such as `\]`;
  - [ ] `$code(...)` output/styling shape.
- [ ] Confirm whether `$code(...)` has a wrapper/class or only a plain `<code>` element with internal `<span>` highlighting.
- [ ] Confirm whether generated docs release output is tracked by `git status --short --ignored docs`.
- [ ] Record any shifted paths before editing.

### Audit / style review / validation

- [ ] Confirm no code has been changed in this phase.
- [ ] Confirm the phase notes identify all touched files before implementation starts.

## Phase 1 — Remove template-body backslash escaping

### Context

Backslash escaping in template bodies creates two competing ways to author literal syntax. Removing it simplifies the tokenizer and makes formatter-local use of `\` possible later. This must affect normal template bodies only. Quoted strings and raw strings outside template bodies keep their current behavior.

### Steps

- [ ] Update `src/compiler_frontend/tokenizer/text_modes.rs`.
- [ ] In `tokenize_template_body(...)`, remove the `match ch { '\\' => ... }` escape branch.
- [ ] Treat `\` like any ordinary body character.
- [ ] Keep `[` and `]` as template delimiters in normal template bodies.
- [ ] Keep carriage-return normalization behavior.
- [ ] Simplify `append_template_body_char(...)` so it no longer calls `append_template_body_escape(...)`.
- [ ] Delete `append_template_body_escape(...)` if it becomes unused.
- [ ] Do not change `tokenize_string(...)`.
- [ ] Do not change `tokenize_raw_string(...)`.
- [ ] Do not change `tokenize_code_template_body(...)` unless the Phase 0 audit finds it incorrectly consumes body backslashes.
- [ ] Update comments in `lexer.rs` / `text_modes.rs` that imply template-body backslash escapes still exist.

### Tests

Add or update tokenizer/parser/integration coverage for:

- [ ] `[: \n]` renders backslash + `n`, not a newline.
- [ ] `[: \[]` is no longer a supported way to output `[`. It either renders `\` then starts template syntax, or fails according to ordinary parser rules.
- [ ] `[: ["[code]"]]` is the canonical literal bracketed output form.
- [ ] `[: ["]"]]` is the canonical literal closing bracket output form.
- [ ] Regular string literal escaping remains valid: `value = "line one\nline two"`.
- [ ] Backticks in normal template bodies tokenize as body text.

Prefer a small integration fixture that checks final HTML output, plus focused tokenizer tests if the tokenizer test harness already has suitable helpers. Do not add diagnostics assertions for old escapes.

### Audit / style review / validation

- [ ] Verify no compatibility paths were added.
- [ ] Verify no new user-facing diagnostics were added.
- [ ] Verify backslash logic is not duplicated in multiple tokenizer helpers.
- [ ] Run targeted tokenizer/template tests.
- [ ] Run `cargo fmt`.

## Phase 2 — Add `$markdown` inline code spans

### Context

The existing markdown formatter already has the right architecture: parent formatters operate on markdown text plus opaque anchors. Inline code should reuse that architecture and stay local to the markdown inline renderer.

### Implementation shape

Use one narrow helper path. Avoid a second parser layer.

Recommended structure:

- [ ] Add `src/compiler_frontend/ast/templates/styles/markdown/inline_code.rs` if the helper set is more than a few small functions.
- [ ] Otherwise keep the helpers next to `render_inline_atoms(...)` in `mod.rs`.
- [ ] If a new file is added, include a file-level doc comment and expose it with `mod inline_code;` from `markdown/mod.rs`.

Suggested types/functions:

```rust
pub(super) struct ParsedInlineCodeSpan {
    pub(super) content: Vec<MarkdownInlineAtom>,
    pub(super) consumed_atoms: usize,
}

pub(super) fn try_parse_inline_code_span_at_atoms(
    atoms: &[MarkdownInlineAtom],
    start_index: usize,
) -> Option<ParsedInlineCodeSpan>;
```

Rules for `try_parse_inline_code_span_at_atoms(...)`:

- [ ] Return `None` unless `atoms[start_index]` is exactly one `MarkdownInlineAtom::Char('`')`.
- [ ] Treat consecutive backtick runs as literal text, not delimiters.
- [ ] Search only until a newline, carriage return, or end of the atom slice.
- [ ] Accept only an isolated single backtick as the closing delimiter.
- [ ] Allow `FormatterOpaqueKind::DynamicExpression` inside the span.
- [ ] Reject/return `None` if a `FormatterOpaqueKind::ChildTemplate` appears before the closing delimiter.
- [ ] Do not inspect opaque anchor output.
- [ ] Preserve every content atom in source order.

Add a small render helper:

```rust
fn render_inline_code_span(
    output: &mut MarkdownOutputBuilder,
    span: ParsedInlineCodeSpan,
);
```

Rules for `render_inline_code_span(...)`:

- [ ] Emit `<code>` and `</code>` using `push_raw(...)`.
- [ ] Escape parent-authored text with `push_escaped_char(...)`.
- [ ] Emit dynamic anchors with `push_opaque(...)`.
- [ ] Do not allow child anchors here. Treat one as an internal invariant if the parser helper already excludes it.

Integrate into `render_inline_atoms(...)`:

- [ ] Check for an inline-code span before link parsing and before ordinary character rendering.
- [ ] If `pending_open_strength > 0`, literalize pending stars before opening `<code>`.
- [ ] Do not close an already-active emphasis span before rendering code.
- [ ] Advance `atom_index` by `span.consumed_atoms` when a span is rendered.
- [ ] For a backtick run that is not a valid span, render the current backtick/run as literal escaped text through the normal output path.

### Edge cases to preserve

- [ ] `` `x` `` -> `<code>x</code>`.
- [ ] `` `  x < y  ` `` -> `<code>  x &lt; y  </code>`.
- [ ] `` `*not emphasis*` `` -> `<code>*not emphasis*</code>`.
- [ ] `` `@/docs (Docs)` `` -> `<code>@/docs (Docs)</code>`.
- [ ] `*Use `Thing` here*` -> `<em>Use <code>Thing</code> here</em>`.
- [ ] `Use `x` and `y`` supports multiple spans.
- [ ] `Use `x` across a soft paragraph line boundary` does not form a multiline span.
- [ ] Unmatched backticks remain literal.
- [ ] `` `` `` and `` ``` `` remain literal.
- [ ] Parent-authored code spans can contain dynamic anchors.
- [ ] Parent-authored code spans cannot contain or cross child-template anchors.

### Audit / style review / validation

- [ ] Verify no AST/HIR representation was added.
- [ ] Verify no formatter flattening of child templates or dynamic expressions was added.
- [ ] Verify helper names are explicit and avoid boolean-heavy APIs.
- [ ] Verify comments explain opaque-anchor behavior and the child-anchor barrier.
- [ ] Run markdown formatter unit tests.
- [ ] Run `cargo fmt`.

## Phase 3 — Test coverage

### Context

This change needs focused markdown tests and at least one end-to-end case. Unit tests should pin formatter edge cases. Integration tests should prove real Beanstalk source produces the intended HTML and that the old template-body escape path is gone.

### Markdown formatter unit tests

Add tests to `src/compiler_frontend/ast/templates/tests/markdown_tests.rs` for:

- [ ] basic inline code in a paragraph;
- [ ] multiple inline code spans on one line;
- [ ] unmatched single backtick renders literally;
- [ ] consecutive backtick runs render literally;
- [ ] HTML escaping inside code spans;
- [ ] emphasis and links do not parse inside code spans;
- [ ] code spans inside active emphasis;
- [ ] inline code in headings;
- [ ] inline code in list items;
- [ ] dynamic expression anchor inside a code span;
- [ ] backticks emitted by dynamic expression anchors do not participate in parsing;
- [ ] child-template anchor blocks code-span pairing;
- [ ] code spans do not cross soft line boundaries.

Use existing helpers such as `to_markdown(...)` and `markdown_formatter_output_from_text_and_anchors(...)` where possible. Extend them only if necessary.

### Integration tests

Add one canonical success fixture under `tests/cases/`, for example:

```text
tests/cases/template_markdown_inline_code/
├── input/
│   └── #page.bst
├── expect.toml
└── golden/
    └── html/
```

Register it in `tests/cases/manifest.toml` with tags:

```toml
[[case]]
id = "template_markdown_inline_code"
path = "template_markdown_inline_code"
tags = ["integration", "templates", "markdown"]
```

The fixture should cover:

- [ ] parent-authored inline code;
- [ ] HTML escaping inside inline code;
- [ ] inserted string literal for literal template syntax inside inline code;
- [ ] backslash rendered literally in a template body;
- [ ] literal bracketed text through string insertion.

Keep the fixture small. Do not use host `io(...)` unless unavoidable. Prefer a top-level page fragment and golden HTML output.

If the repo already has markdown/template integration cases, extend one only if doing so keeps the case clearer than adding a new one.

### Audit / style review / validation

- [ ] Verify tests assert behavior, not implementation internals.
- [ ] Verify diagnostics are not asserted for old escape syntax.
- [ ] Run targeted unit tests.
- [ ] Run `cargo run tests` or the narrow integration-test command if available.
- [ ] Run `cargo fmt`.

## Phase 4 — Docs-site styling and authored docs migration

### Context

`[codesnippet: ...]` is docs-site styling. It should be removed from authored docs and replaced by normal markdown inline code. The compiler should not know about this helper.

### Styling

Update `docs/src/styles/docs.bst`:

- [ ] Move the old `codesnippet` inline look to ordinary `code` CSS inside `theme_css`.
- [ ] Prefer a selector scoped to docs content, such as `.container code`, unless the repo already has a better docs-content wrapper.
- [ ] Do not add compiler-specific classes for markdown inline code.
- [ ] Audit `$code(...)` output. If it is affected by normal `code` styling but remains readable, leave it alone.
- [ ] If `$code(...)` becomes unreadable and already has a wrapper/class, use CSS to target that wrapper.
- [ ] If `$code(...)` lacks any stable selector and genuinely needs one, add the smallest stable selector in the `$code(...)` output path. Do not otherwise refactor `$code(...)`.
- [ ] Delete `codesnippet #= ...` once all imports/usages are removed.

### Authored docs migration

Update `docs/src/docs/**`, `docs/src/**/*.bst`, and any authored docs/library pages found by `rg`:

- [ ] Remove `codesnippet` from imports.
- [ ] Replace simple `[codesnippet:thing]` with markdown inline code: `` `thing` ``.
- [ ] Replace `[codesnippet, "..."]` with markdown inline code when the snippet is short and does not need complex literal delimiters.
- [ ] For literal `[` / `]` snippets, use string-literal insertion inside the code span:

```beanstalk
`["[$slot]"]`
`["[else if ...]"]`
`["[break]"]`
```

- [ ] For literal backtick snippets, use string insertion such as `["`"]`.
- [ ] Convert longer or noisy snippets to `$code(...)` blocks.
- [ ] Do not introduce triple-backtick markdown fences.
- [ ] Replace docs guidance that recommends `\[` / `\]` with string-literal insertion.
- [ ] Update Beandown docs to say `.bd` follows normal template and `$markdown` rules.
- [ ] Update examples such as `[: \[code\]]` to `[: ["[code]"]]`.

### Docs build

- [ ] Run `cargo run build docs --release`.
- [ ] Include generated docs output if tracked.
- [ ] Do not hand-edit generated release files.

### Audit / style review / validation

- [ ] Run `rg -n 'codesnippet|\\\\\[|\\\\\]|escaped outer|escape.*template' docs/src docs/language-overview.md` and resolve stale results.
- [ ] Verify normal inline `<code>` output has the intended docs style.
- [ ] Verify `$code(...)` examples remain readable.
- [ ] Verify docs source remains readable; convert noisy inline snippets to `$code(...)` rather than overusing string insertion.

## Phase 5 — Compiler and user-facing docs updates

### Context

The compiler-facing language overview should state the new semantics precisely. The user-facing docs should explain how to write inline code and literal template syntax without implying CommonMark compatibility.

### `docs/language-overview.md`

Update the string/template section:

- [ ] Say expression-position backticks create raw string slices.
- [ ] Say template-body backticks are ordinary body text preserved for formatters.
- [ ] Say template-body backslashes are ordinary body text.
- [ ] Say regular quoted string literals still support escapes.
- [ ] Add the canonical literal-delimiter form:

```beanstalk
[: ["[literal]"]]
```

Update the `$markdown` directive description:

- [ ] Document paired isolated single-backtick inline code spans.
- [ ] Document same-line only behavior.
- [ ] Document that unmatched and repeated backticks render literally.
- [ ] Document that CommonMark fenced code blocks, variable-length code spans, multiline code spans, and markdown-level backtick escaping are not part of Beanstalk markdown.
- [ ] Document that dynamic expressions inside code spans remain expression output and are not inspected by `$markdown`.
- [ ] Document that child templates are opaque to parent markdown formatting.

Update the Beandown section:

- [ ] Remove escaped `]` guidance.
- [ ] State that literal template delimiters in `.bd` use the same string insertion pattern as templates.
- [ ] State that `.bd` always follows normal template and `$markdown` behavior.

### `docs/src/docs/templates/#page.bst`

- [ ] Add a short inline-code subsection under `$markdown` or Style Directives.
- [ ] Show small examples of inline code.
- [ ] Show string-literal insertion for literal template syntax.
- [ ] State that longer code examples should use `$code(...)`.
- [ ] State that triple-backtick fences are not part of Beanstalk markdown.

### `docs/src/docs/beandown/#page.bst` and related pages

- [ ] Replace escape guidance with string-literal insertion guidance.
- [ ] Clarify that `.bd` uses the same template/markdown body semantics as Beanstalk templates.

### Audit / style review / validation

- [ ] Keep `docs/language-overview.md` concise and compiler-facing.
- [ ] Keep user-facing docs examples short and practical.
- [ ] Avoid repeating the full rules in every page; link or cross-reference where appropriate.

## Phase 6 — Roadmap and progress matrix

### Context

The progress matrix should describe what is supported now and which markdown features are deliberately outside Beanstalk's markdown flavour. The roadmap should no longer carry this item as an unplanned TODO once implementation starts or lands.

### `docs/src/docs/progress/#page.bst`

Update the **Templates and style directives** row:

- [ ] Add that `$markdown` supports isolated same-line single-backtick inline code spans rendered as `<code>`.
- [ ] Add that template-body backticks and backslashes are ordinary formatter-visible text.
- [ ] Add that literal template delimiters should be inserted through string literals.
- [ ] Add that CommonMark fenced code blocks, variable-length code spans, multiline code spans, and markdown-level backtick escaping are not part of Beanstalk markdown.

Update the **Beandown `.bd` content assets** row:

- [ ] Remove escaped outer-close guidance.
- [ ] State that `.bd` bodies follow normal template-body and `$markdown` semantics.
- [ ] State that literal delimiters use string insertion.

### `docs/roadmap/roadmap.md`

- [ ] If this plan is committed under `docs/roadmap/plans/`, update the roadmap TODO to link to it while implementation is pending.
- [ ] Once implemented, remove the TODO or move any genuinely remaining follow-up to `Notes`.
- [ ] Do not leave CommonMark fences/backtick escaping as generic future TODOs. They are not part of Beanstalk markdown.

### Audit / style review / validation

- [ ] Verify roadmap/matrix language does not imply CommonMark compatibility is planned.
- [ ] Verify matrix text is concise enough to remain readable.
- [ ] Verify `codesnippet` migration did not make the matrix source noisy; convert long inline snippets to `$code(...)` where needed.

## Phase 7 — Full validation and final cleanup

### Context

This change affects tokenization, template parsing, markdown formatting, docs source, and generated docs output. End with a full cleanup pass rather than only targeted tests.

### Steps

- [ ] Run `cargo fmt`.
- [ ] Run targeted markdown/tokenizer/template tests.
- [ ] Run `cargo run tests`.
- [ ] Run `cargo run build docs --release`.
- [ ] Run `just validate`.
- [ ] Run final searches:

```bash
rg -n 'codesnippet|\\\\\[|\\\\\]|escaped outer|CommonMark|triple backtick|fenced code|append_template_body_escape' \
  src docs tests libraries
```

- [ ] Resolve all stale references, except where explicitly documenting unsupported CommonMark features.
- [ ] Run `git status --short` and inspect all changed files.
- [ ] Verify generated docs files are included only if produced by the docs build and tracked by the repo.

### Final audit checklist

- [ ] No template-body backslash escape compatibility remains.
- [ ] Expression-position string and raw string behavior is unchanged.
- [ ] `$markdown` inline code uses existing formatter atoms and anchors.
- [ ] Child templates remain opaque to parent formatters.
- [ ] Dynamic expressions remain opaque to parent formatters.
- [ ] No new AST/HIR/codegen structures were added for inline code.
- [ ] No new diagnostics were added for old syntax.
- [ ] No old `[codesnippet: ...]` authored docs usages remain.
- [ ] Docs inline `code` styling is applied through normal CSS.
- [ ] `$code(...)` remains readable.
- [ ] Roadmap/matrix/docs describe the deliberate non-CommonMark surface correctly.

## Suggested implementation notes

### Keep the tokenizer change small

The tokenizer change should be mostly deletion. Do not replace template-body escaping with another abstraction. The intended normal body behavior is:

- collect body text until `[` or `]`;
- normalize carriage returns;
- preserve all other characters exactly.

### Keep markdown inline code local

Inline code is a markdown formatter concern. It should not affect:

- tokenization;
- AST template content representation;
- template composition;
- HIR lowering;
- backend lowering.

### Avoid helper over-design

The inline-code helper only needs to answer: “does a valid inline code span start at this atom index?” Avoid broad token streams, parser structs, or state machines unless later markdown work genuinely requires them.

### Preserve formatter opacity

Do not “improve” markdown by flattening anchors. That would undo the current architecture and allow parent formatters to reinterpret child template output. This is the main complexity boundary to preserve.

### Prefer deletion over compatibility

When old escaping or `codesnippet` paths become unused, delete them. Do not keep wrappers or parallel APIs for Alpha-only legacy behavior.
