# Beanstalk HTML Markdown Import Implementation Plan

Target feature: HTML-builder support for importing plain `.md` Markdown files as generated `content #String` constants.

---

## Goal

Add support for extensionless source imports of plain Markdown files in HTML projects:

```beanstalk
import @docs/intro

[: [intro.content] ]
```

A project file such as `docs/intro.md` should be read as raw UTF-8 Markdown, rendered to HTML at compile time, and exposed to the importing Beanstalk file as if it had generated exactly one declaration:

```beanstalk
content #String = "<rendered html>"
```

The generated declaration must use the existing source import, facade re-export, dependency sorting, AST constant folding, HIR handoff, borrow validation, and backend pipeline. There should be no Markdown-specific AST, HIR, borrow-checker, or backend path.

---

## Non-goals and guardrails

- [ ] Do not tokenize `.md` files as Beanstalk.
- [ ] Do not support Beanstalk imports, declarations, interpolation, templates, frontmatter, metadata blocks, or config inside `.md` files.
- [ ] Do not add Markdown link/image tracking, asset copying, route rewriting, or output-path rewriting in this feature.
- [ ] Do not add a sanitizer or raw-HTML policy config key in this feature. V1 preserves raw HTML and documents that behavior.
- [ ] Do not write a custom CommonMark parser for V1.
- [ ] Do not change `.bd` semantics except where shared generated-content helper code removes duplication without changing behavior.
- [ ] Do not add new diagnostics unless the existing import/source-kind diagnostics are materially misleading.
- [ ] Do not add compatibility wrappers or transitional APIs.

---

## Current repo anchor

Re-check this snapshot before coding. If any listed owner has drifted, update the affected phase before implementation.

Observed on `nyejames/beanstalk` default branch `main` on 2026-06-17:

| Area | Current shape | Anchor file / observed SHA |
|---|---|---|
| Source kinds | `SourceFileKind` currently has `Beanstalk` and `Beandown` only. | `src/libraries/source_file_kind_registry.rs` — `75dde33012a5057a7ef5b336c539242d5da63160` |
| Recognized source extensions | `from_extension`, `extension`, `extension_suffix`, and `recognized_kinds()` cover `.bst` and `.bd` only. | `src/libraries/source_file_kind_registry.rs` — `75dde33012a5057a7ef5b336c539242d5da63160` |
| Source-kind tests | `source_file_kind_registry_tests.rs` already exists and should be extended, not duplicated. | `src/libraries/tests/source_file_kind_registry_tests.rs` — `e37ba127f6a530048c7887f44b9025c19edb61ba` |
| Tokenizer entry mode | `TokenizerEntryMode::for_source_file_kind` returns `Self`, so adding a non-tokenized kind requires changing this API or branching before calling it. | `src/compiler_frontend/tokenizer/tokens.rs` — `1f30c9be160e3d06b2c6f9a5b70795b293a87fc5` |
| File preparation | `prepare_file_frontend_local` currently tokenizes before matching `SourceFileKind`. This must change for `.md`. | `src/compiler_frontend/pipeline.rs` — `2d4be46629ae97c44477cc6e5fad42c918309174` |
| Beandown synthetic header | `.bd` preparation already generates one private `content #String` constant using a synthetic initializer token stream. | `src/compiler_frontend/headers/beandown_prepare.rs` — `3c0401a4cea3cb1997cf5ba154e2d14e2dfb9023` |
| Stage 0 traversal | Reachable-file discovery skips import scanning only for `Beandown`, and queues the same-directory facade for Beandown scope support. | `src/build_system/create_project_modules/reachable_file_discovery.rs` — `09aa4654a00391a6f9ffefefe26b174b5f993b08` |
| HTML builder source assets | `HtmlProjectBuilder::libraries()` currently registers Beandown only. | `src/projects/html_project/html_project_builder.rs` — `d1b34a6e6bf16e4db6d92c63f79dc831ccbdef3e` |
| Dependencies | `Cargo.toml` has no Markdown parser dependency. | `Cargo.toml` — `6a89b1f78c732381fd2ab11a4553f8ab3f1d8567` |
| Frontend module map | New frontend module entries belong in `src/compiler_frontend/mod.rs`. | `src/compiler_frontend/mod.rs` — `0e512cd097d876eb38c06447031524a884f3e356` |

Existing systems to reuse:

- [ ] `SourceFileKindRegistry` for builder-owned source-kind support.
- [ ] `SourceFileKind::recognized_kinds()` and import candidate generation for `.md` candidate discovery.
- [ ] Existing explicit-source-extension diagnostics for `import @docs/intro.md`.
- [ ] Existing recognized-but-unsupported source-kind diagnostics for builders that do not register `.md`.
- [ ] Existing invalid-source-file-entry diagnostics for `.md` used as a single-file entry.
- [ ] Existing ambiguous-import-target diagnostics for `intro.bst` / `intro.bd` / `intro.md` / `intro/` collisions.
- [ ] Existing facade re-export and grouped import machinery for `content`.

---

## Dependency decision

Use `pulldown-cmark` behind a small Beanstalk-owned adapter.

At the start of Phase 1, verify the latest compatible version and feature flags. The currently observed docs.rs latest is `0.13.4`. The desired dependency shape is:

```toml
pulldown-cmark = { version = "0.13.4", default-features = false, features = ["html"] }
```

Rationale:

- [ ] `pulldown-cmark` directly parses CommonMark-style Markdown and renders HTML.
- [ ] The `html` feature is the only renderer feature needed for this implementation.
- [ ] `default-features = false` avoids the default CLI-oriented `getopts` feature while keeping HTML rendering explicit.
- [ ] The dependency must be imported only inside the plain Markdown adapter.

V1 options:

```rust
Options::ENABLE_TABLES
Options::ENABLE_TASKLISTS
Options::ENABLE_STRIKETHROUGH
Options::ENABLE_FOOTNOTES
Options::ENABLE_GFM
```

Keep disabled in V1:

- [ ] `ENABLE_SMART_PUNCTUATION` — it rewrites authored text such as `--`, quotes, and ellipses.
- [ ] `ENABLE_HEADING_ATTRIBUTES` — useful later, but not baseline Markdown import behavior.
- [ ] `ENABLE_MATH` — needs an explicit rendering/runtime policy.
- [ ] metadata/frontmatter flags — this feature treats `.md` as plain content.
- [ ] wikilinks, subscript, superscript, definition lists — dialect-specific and not required for the first import path.

---

## Simplification opportunities to apply during implementation

- [ ] Keep the Markdown renderer as one small module, `src/compiler_frontend/plain_markdown.rs`, instead of a directory with `options.rs` and `render.rs`. Split later only if it grows real responsibilities.
- [ ] Add a focused `headers/synthetic_content_header.rs` helper for generated `content #String` headers. Use it from both `beandown_prepare.rs` and `plain_markdown_prepare.rs` so `content` name interning, `HeaderKind::Constant`, `ParsedTypeRef::BuiltinString`, private export mode, and empty header token construction are not duplicated.
- [ ] Extract source identity lookup from `CompilerFrontend::tokenize_source` into one helper and reuse it for both tokenized files and raw Markdown files.
- [ ] Extend existing source-kind tests instead of adding a parallel test owner.
- [ ] Keep Stage 0 source-kind behavior as one explicit `match`. Do not add source-kind predicate methods unless the same decision repeats in several files.
- [ ] Do not create Markdown metadata structs for links/images until a consumer exists.
- [ ] Do not add config keys, builder hooks, route policies, or backend branches.

---

## Validation cadence

Every phase ends with an audit, style-guide review, and validation checkpoint.

Preferred full command:

```bash
just validate
```

If the environment cannot run the full command, run the strongest available subset and record the limitation:

```bash
cargo fmt --check
cargo clippy --all-targets --all-features
cargo test
cargo run -- tests
```

---

# Phase 0 — Preflight repo audit

## Summary / reasoning / context

The repository is in active Alpha development. This phase verifies that the planned ownership boundaries still match the current code before an agent starts editing. It prevents implementing against stale extension points.

## Checklist

- [ ] Create or switch to a dedicated branch, for example `feature/html-markdown-imports`.
- [ ] Confirm the working tree is clean.
- [ ] Fetch or pull the current `main` branch.
- [ ] Re-read and compare these files against the repo anchor above:
  - [ ] `Cargo.toml`
  - [ ] `src/compiler_frontend/mod.rs`
  - [ ] `src/libraries/source_file_kind_registry.rs`
  - [ ] `src/libraries/tests/source_file_kind_registry_tests.rs`
  - [ ] `src/compiler_frontend/tokenizer/tokens.rs`
  - [ ] `src/compiler_frontend/pipeline.rs`
  - [ ] `src/compiler_frontend/headers/mod.rs`
  - [ ] `src/compiler_frontend/headers/beandown_prepare.rs`
  - [ ] `src/build_system/create_project_modules/reachable_file_discovery.rs`
  - [ ] `src/compiler_frontend/paths/path_resolution.rs`
  - [ ] `src/compiler_frontend/paths/path_normalization.rs`
  - [ ] `src/projects/html_project/html_project_builder.rs`
  - [ ] `docs/language-overview.md`
  - [ ] `docs/compiler-design-overview.md`
  - [ ] `docs/src/docs/beandown/#page.bst`
- [ ] Run baseline validation before changes.
- [ ] Record baseline results in the implementation notes or PR description.

## Phase closeout — audit / style / validation

- [ ] No feature code has been changed in this phase.
- [ ] Any repo drift has been reflected in the relevant phase checklist.
- [ ] If `.md` import support already exists, stop and convert the remaining work into audit, tests, and docs updates.
- [ ] Baseline validation result is recorded.

---

# Phase 1 — Add the plain Markdown renderer adapter

## Summary / reasoning / context

This phase adds the parser dependency and a narrow rendering adapter without wiring it into imports. The goal is to make Markdown rendering testable in isolation before touching Stage 0 or frontend file preparation.

## Checklist

- [ ] Re-check `pulldown-cmark` version and feature names.
- [ ] Add the dependency to `Cargo.toml` using `default-features = false` and `features = ["html"]`.
- [ ] Update `Cargo.lock` deterministically.
- [ ] Create one small module:

  ```text
  src/compiler_frontend/plain_markdown.rs
  src/compiler_frontend/tests/plain_markdown_tests.rs
  ```

- [ ] Add `pub(crate) mod plain_markdown;` to `src/compiler_frontend/mod.rs`.
- [ ] Add file-level docs to `plain_markdown.rs`:
  - [ ] WHAT: renders raw plain Markdown imports to HTML.
  - [ ] WHY: `.md` files are plain content assets and must not enter Beanstalk tokenization.
  - [ ] MUST NOT: own import resolution, source-kind registration, diagnostics, asset emission, route rewriting, or Beandown behavior.
- [ ] Implement this small API:

  ```rust
  pub(crate) struct RenderedPlainMarkdown {
      pub(crate) html: String,
  }

  pub(crate) fn render_plain_markdown(markdown: &str) -> RenderedPlainMarkdown
  ```

- [ ] Keep all `pulldown_cmark` imports inside `plain_markdown.rs`.
- [ ] Implement a private `commonmark_web_options()` helper with the V1 option set.
- [ ] Allocate output predictably:

  ```rust
  let mut html = String::with_capacity(markdown.len() + markdown.len() / 2);
  ```

- [ ] Render with `Parser::new_ext(markdown, options)` and `pulldown_cmark::html::push_html`.
- [ ] Add focused renderer tests:
  - [ ] heading and paragraph rendering;
  - [ ] fenced code block rendering;
  - [ ] table rendering;
  - [ ] task list rendering;
  - [ ] strikethrough rendering;
  - [ ] footnote rendering if stable enough in output;
  - [ ] raw HTML preservation;
  - [ ] `--` remains `--`, proving smart punctuation is disabled.

## Phase closeout — audit / style / validation

- [ ] The renderer module has exactly one responsibility.
- [ ] `pulldown_cmark` is not imported outside `plain_markdown.rs`.
- [ ] No Stage 0, header, AST, HIR, borrow-checker, or backend behavior is changed.
- [ ] No unused metadata structs or future-facing config types were added.
- [ ] Tests are in a separate test file.
- [ ] Run:

  ```bash
  cargo fmt --check
  cargo test plain_markdown
  cargo clippy --all-targets --all-features
  ```

- [ ] Run `just validate` if available.

---

# Phase 2 — Add source-kind support and raw Markdown header preparation

## Summary / reasoning / context

This phase teaches the compiler about `.md` as a recognized source-file kind and adds the frontend preparation path that turns raw Markdown into the ordinary synthetic `content #String` header. The critical behavior is that Markdown bypasses the tokenizer entirely.

Do not register `.md` in the HTML builder during this phase. After this phase, `.md` should be recognized but unsupported unless a builder registers it.

## Checklist

### 2.1 Extend the source-kind model

- [ ] Update `src/libraries/source_file_kind_registry.rs`.
- [ ] Add a source-kind variant:

  ```rust
  PlainMarkdown
  ```

- [ ] Update `SourceFileKind::from_extension`:

  ```rust
  "md" => Some(Self::PlainMarkdown)
  ```

- [ ] Update `extension()` and `extension_suffix()`.
- [ ] Update `recognized_kinds()` to include `.md` in deterministic extension order.
- [ ] Update `supports_recognized_extension()` match arms if needed.
- [ ] Extend `src/libraries/tests/source_file_kind_registry_tests.rs`:
  - [ ] empty registry recognizes `.md` as compiler-known but unsupported;
  - [ ] registering `.md` makes it supported;
  - [ ] `supported_kinds()` sorts multiple kinds deterministically;
  - [ ] `from_extension("md")`, `extension()`, and `extension_suffix()` round-trip.

### 2.2 Make tokenization optional by source kind

- [ ] Update `TokenizerEntryMode::for_source_file_kind` to return `Option<Self>`:

  ```rust
  pub fn for_source_file_kind(source_kind: SourceFileKind) -> Option<Self>
  ```

- [ ] Return:
  - [ ] `Some(SourceFile)` for `Beanstalk`;
  - [ ] `Some(TemplateBody { ... })` for `Beandown`;
  - [ ] `None` for `PlainMarkdown`.
- [ ] Add a doc comment explaining that `None` means the source kind is compiler-recognized but must not be tokenized.
- [ ] Update production callers explicitly. Do not use `.unwrap()` for the tokenizer mode.

### 2.3 Extract frontend source identity lookup

- [ ] In `src/compiler_frontend/pipeline.rs`, extract the source identity lookup currently inside `tokenize_source` into a small helper.
- [ ] Suggested internal type:

  ```rust
  struct FrontendSourceFileIdentity {
      logical_path: InternedPath,
      file_id: Option<FileId>,
      canonical_os_path: Option<PathBuf>,
  }
  ```

- [ ] Suggested helper:

  ```rust
  fn source_file_identity(
      source_files: &SourceFileTable,
      source_path: &PathBuf,
      string_table: &mut StringTable,
  ) -> FrontendSourceFileIdentity
  ```

- [ ] Use the helper from `tokenize_source` and from Markdown preparation wiring.
- [ ] Keep this helper near `tokenize_source`; do not create a new module unless the identity lookup already has another owner after repo drift.

### 2.4 Add shared generated-content header helper

- [ ] Add:

  ```text
  src/compiler_frontend/headers/synthetic_content_header.rs
  ```

- [ ] Add it to `src/compiler_frontend/headers/mod.rs` as a private module.
- [ ] File-level docs:
  - [ ] WHAT: builds ordinary private `content #String` headers for compiler-generated source assets.
  - [ ] WHY: `.bd` and `.md` both expose a single generated content constant but differ in how their initializer tokens are produced.
  - [ ] MUST NOT: render Markdown, tokenize source, parse imports, or own source-kind decisions.
- [ ] Keep the helper deliberately narrow. Suggested input:

  ```rust
  pub(crate) struct SyntheticContentHeaderInput {
      pub(crate) source_file: InternedPath,
      pub(crate) file_id: Option<FileId>,
      pub(crate) canonical_os_path: Option<PathBuf>,
      pub(crate) location: SourceLocation,
      pub(crate) initializer_tokens: Vec<Token>,
      pub(crate) initializer_references: Vec<InitializerReference>,
  }
  ```

- [ ] Implement:

  ```rust
  pub(crate) fn synthetic_content_header(
      input: SyntheticContentHeaderInput,
      string_table: &mut StringTable,
  ) -> Header
  ```

- [ ] The helper owns:
  - [ ] interning `content`;
  - [ ] creating `source_file.append(content_name)`;
  - [ ] `BindingMode::CompileTimeConstant`;
  - [ ] `ParsedTypeRef::BuiltinString`;
  - [ ] `HeaderKind::Constant`;
  - [ ] `HeaderExportMode::Private`;
  - [ ] `FileRole::Normal`;
  - [ ] empty dependency set;
  - [ ] empty `capacity_references`;
  - [ ] empty `FileTokens` with `canonical_os_path` copied onto it.

### 2.5 Refactor Beandown preparation through the helper

- [ ] Update `beandown_prepare.rs` to use `synthetic_content_header`.
- [ ] Keep Beandown initializer behavior unchanged:
  - [ ] tokenized body;
  - [ ] synthetic `$markdown` directive;
  - [ ] `collect_symbol_references` over the generated template initializer tokens.
- [ ] Keep `prepare_beandown_file` output shape unchanged.
- [ ] Run existing Beandown tests before adding Markdown preparation.

### 2.6 Add Markdown header preparation

- [ ] Add:

  ```text
  src/compiler_frontend/headers/plain_markdown_prepare.rs
  src/compiler_frontend/headers/tests/plain_markdown_prepare_tests.rs
  ```

- [ ] Add `plain_markdown_prepare` to `headers/mod.rs`.
- [ ] File-level docs:
  - [ ] WHAT: turns raw `.md` source into a private synthetic `content #String` declaration.
  - [ ] WHY: later frontend stages should see an ordinary folded constant, not Markdown-specific AST/HIR.
  - [ ] MUST NOT: tokenize Markdown or inspect it as Beanstalk syntax.
- [ ] Define an input struct to avoid a long argument list:

  ```rust
  pub(crate) struct PlainMarkdownPrepareInput<'a> {
      pub(crate) source_code: &'a str,
      pub(crate) source_file: InternedPath,
      pub(crate) file_id: Option<FileId>,
      pub(crate) canonical_os_path: Option<PathBuf>,
  }
  ```

- [ ] Implement:

  ```rust
  pub(crate) fn prepare_plain_markdown_file(
      input: PlainMarkdownPrepareInput<'_>,
      string_table: &mut StringTable,
  ) -> FileFrontendPrepareOutput
  ```

- [ ] Render Markdown with `plain_markdown::render_plain_markdown(input.source_code)`.
- [ ] Intern the rendered HTML string.
- [ ] Build a single initializer token at the Markdown file start location:

  ```rust
  TokenKind::StringSliceLiteral(rendered_html_id)
  ```

- [ ] Before finalizing the token kind, inspect AST constant folding for `StringSliceLiteral` versus `RawStringLiteral`. Use the token kind that preserves already-rendered HTML exactly and folds cleanly to `#String` without serializing or escaping source text.
- [ ] Set `initializer_references` to `Vec::new()`. Do not scan rendered HTML for Beanstalk symbol references.
- [ ] Generate the header through `synthetic_content_header`.
- [ ] Return `FileFrontendPrepareOutput` with:
  - [ ] `file_role: FileRole::Normal`;
  - [ ] `file_imports: Vec::new()`;
  - [ ] `top_level_const_fragments: Vec::new()`;
  - [ ] `const_template_count: 0`;
  - [ ] `runtime_fragment_count: 0`;
  - [ ] `token_count: 0`, because Markdown is not tokenized;
  - [ ] no warnings.
- [ ] Add focused tests:
  - [ ] exactly one header is produced;
  - [ ] the generated header path ends in `content`;
  - [ ] the declaration type is builtin `String`;
  - [ ] initializer is a single literal token containing rendered HTML;
  - [ ] Markdown containing `$100`, `--`, `[not_a_template]`, and `[: body]` creates no initializer references;
  - [ ] rendered HTML containing quotes, backticks, brackets, and newlines is preserved exactly through the chosen token kind.

### 2.7 Wire frontend file preparation

- [ ] Update `CompilerFrontend::prepare_file_frontend_local` to branch by source kind before tokenization.
- [ ] Intended structure:

  ```rust
  match input.source_kind {
      SourceFileKind::PlainMarkdown => prepare_plain_markdown_file(...),
      SourceFileKind::Beanstalk | SourceFileKind::Beandown => tokenize_then_prepare(...),
  }
  ```

- [ ] Keep the tokenized path behavior unchanged for `.bst` and `.bd`.
- [ ] Do not route Markdown tokenization errors through `FileFrontendPrepareError`; Markdown rendering itself should not create source diagnostics in V1.
- [ ] Ensure all exhaustive matches over `SourceFileKind` have explicit `PlainMarkdown` arms.

## Phase closeout — audit / style / validation

- [ ] `.md` has no tokenizer path.
- [ ] Markdown preparation does not create Beanstalk template tokens.
- [ ] Markdown preparation does not call `collect_symbol_references`.
- [ ] Beandown behavior is unchanged and now shares only the generated-content header helper.
- [ ] No AST, HIR, borrow-checker, backend, or HTML builder registration code is touched in this phase except compile-required exhaustive matches.
- [ ] New files have file-level WHAT/WHY docs.
- [ ] Tests live in separate test files.
- [ ] Run:

  ```bash
  cargo fmt --check
  cargo test source_file_kind_registry
  cargo test beandown_prepare
  cargo test plain_markdown_prepare
  cargo clippy --all-targets --all-features
  ```

- [ ] Run `just validate` if available.

---

# Phase 3 — Stage 0 discovery and HTML builder registration

## Summary / reasoning / context

This phase makes `.md` usable in HTML projects. Stage 0 must treat Markdown as an importless content asset, while the HTML builder must opt in declaratively through the existing source-kind registry.

Update Stage 0 before registering `.md` in the HTML builder.

## Checklist

### 3.1 Update reachable-file discovery

- [ ] Update `src/build_system/create_project_modules/reachable_file_discovery.rs`.
- [ ] Replace the Beandown-only early skip with an explicit match:

  ```rust
  match next_file.kind {
      SourceFileKind::Beanstalk => {
          // scan imports below
      }
      SourceFileKind::Beandown => {
          queue_same_directory_facade_for_beandown(&canonical_file, &reachable, &mut queue);
          continue;
      }
      SourceFileKind::PlainMarkdown => {
          continue;
      }
  }
  ```

- [ ] Add a short comment explaining why `.md` differs from `.bd`:
  - [ ] `.bd` is a Beanstalk template body and may need same-directory facade constants.
  - [ ] `.md` is plain Markdown and has no Beanstalk scope.
  - [ ] Facade re-exports still work because the facade file itself is Beanstalk and is scanned normally.
- [ ] Do not scan Markdown files for imports.
- [ ] Do not treat Markdown links/images as Beanstalk imports.
- [ ] Confirm `provider_backed_import_prefix` continues to skip recognized source extensions now that `.md` is recognized.

### 3.2 Register `.md` in the HTML builder

- [ ] Update `src/projects/html_project/html_project_builder.rs`.
- [ ] Register `PlainMarkdown` alongside Beandown in `HtmlProjectBuilder::libraries()`:

  ```rust
  libraries.source_file_kinds.register(
      SourceFileKind::PlainMarkdown.extension(),
      SourceFileKind::PlainMarkdown,
  );
  ```

- [ ] Keep Beandown registration unchanged.
- [ ] Do not add a Markdown config key.
- [ ] Do not touch backend lowering.

### 3.3 Verify path-resolution behavior

Use existing resolver behavior unless it has become misleading after `.md` recognition.

- [ ] Extensionless import candidates include `.bst`, `.bd`, and `.md`.
- [ ] `import @docs/intro.md` is rejected by explicit source-extension diagnostics.
- [ ] `.md` imported under a builder that does not register `PlainMarkdown` reports unsupported source file kind, not missing target.
- [ ] `.md` used as a single-file entry reports invalid source file entry.
- [ ] Collisions among `intro.bst`, `intro.bd`, `intro.md`, and `intro/` report ambiguous import target.
- [ ] No new diagnostic kind is added unless an existing message is actively wrong.

## Phase closeout — audit / style / validation

- [ ] Stage 0 has one clear source-kind match.
- [ ] `.bd` same-directory facade behavior is preserved.
- [ ] `.md` does not queue same-directory facades, scan imports, or inspect Markdown links.
- [ ] HTML builder support remains declarative in `libraries()`.
- [ ] Existing diagnostics are reused.
- [ ] Run:

  ```bash
  cargo fmt --check
  cargo test reachable_file_discovery
  cargo test path_resolution
  cargo clippy --all-targets --all-features
  ```

- [ ] Run `just validate` if available.

---

# Phase 4 — Integration tests and diagnostic fixtures

## Summary / reasoning / context

The user-facing behavior is source import semantics and rendered output. Unit tests protect narrow renderer/header invariants; integration tests should prove real Beanstalk project behavior.

## Checklist

### 4.1 Positive integration cases

Add cases under `tests/cases/` and register each in `tests/cases/manifest.toml`.

- [ ] `html_markdown_import_basic`
  - [ ] `input/#page.bst` imports `@docs/intro` and renders `[intro.content]`.
  - [ ] `input/docs/intro.md` contains a heading, paragraph, emphasis, and list.
  - [ ] `expect.toml` uses `[backends.html]`, success mode, forbidden warnings, and normalized golden output if available.
  - [ ] Golden proves expected heading, emphasis, and list HTML appears.

- [ ] `html_markdown_grouped_import_alias`
  - [ ] Uses grouped import alias:

    ```beanstalk
    import @docs/intro {
        content as intro_html,
    }
    ```

  - [ ] Renders `[intro_html]`.
  - [ ] Golden proves alias import works.

- [ ] `html_markdown_plain_not_beanstalk`
  - [ ] Markdown contains Beanstalk-looking text:

    ```markdown
    This costs $100 -- not a comment.
    Literal template-looking text: [not_a_template]
    Raw Beanstalk-ish block: [: <p>not parsed</p>]
    ```

  - [ ] Golden proves `$100`, `--`, `[not_a_template]`, and `[: ...]` are treated as Markdown/text.
  - [ ] This is the primary regression test for the no-tokenization rule.

- [ ] `html_markdown_tables_tasks_code`
  - [ ] Markdown contains a table, task list, strikethrough, footnote, and fenced code block.
  - [ ] Golden proves the selected parser options are enabled.

- [ ] `html_markdown_raw_html_preserved`
  - [ ] Markdown contains inline HTML and an HTML block.
  - [ ] Golden proves raw HTML is preserved.

- [ ] `html_markdown_facade_reexport`
  - [ ] `input/docs/#mod.bst` re-exports Markdown content:

    ```beanstalk
    export @./intro {
        content as intro,
    }
    ```

  - [ ] `input/#page.bst` imports `@docs { intro }` and renders `[intro]`.
  - [ ] Golden proves normal facade re-export works.

- [ ] `html_markdown_literal_relative_links`
  - [ ] Markdown contains `[Next](./next.md)` and `![Diagram](./diagram.png)`.
  - [ ] Golden proves `href` and `src` are literal relative values.
  - [ ] No tracked-asset output should be expected from this fixture.

### 4.2 Negative integration cases

Use stable diagnostic codes rather than rendered prose.

- [ ] `html_markdown_direct_extension_import_rejected`
  - [ ] `input/#page.bst` uses `import @docs/intro.md`.
  - [ ] `input/docs/intro.md` exists.
  - [ ] Assert:

    ```toml
    diagnostic_codes = ["BST-IMPORT-0024"]
    ```

- [ ] `html_markdown_collision_with_beandown`
  - [ ] Create both `input/docs/intro.md` and `input/docs/intro.bd`.
  - [ ] Import `@docs/intro`.
  - [ ] Assert:

    ```toml
    diagnostic_codes = ["BST-IMPORT-0006"]
    ```

- [ ] `html_markdown_collision_with_bst`
  - [ ] Create both `input/docs/intro.md` and `input/docs/intro.bst`.
  - [ ] Import `@docs/intro`.
  - [ ] Assert `BST-IMPORT-0006`.

- [ ] `html_markdown_collision_with_folder`
  - [ ] Create both `input/docs/intro.md` and `input/docs/intro/` with an importable target or facade.
  - [ ] Import `@docs/intro`.
  - [ ] Assert `BST-IMPORT-0006`.

- [ ] `html_markdown_entry_rejected`
  - [ ] Use a single-file entry pointing at a `.md` file if the runner supports it cleanly.
  - [ ] Assert:

    ```toml
    diagnostic_codes = ["BST-IMPORT-0026"]
    ```

  - [ ] If the integration runner cannot express this cleanly, add a focused build-system test around `compile_single_file_frontend` instead.

- [ ] `html_markdown_unsupported_builder_source_kind`
  - [ ] Add only if the test harness can run a builder that does not register `PlainMarkdown`.
  - [ ] Assert:

    ```toml
    diagnostic_codes = ["BST-IMPORT-0025"]
    ```

### 4.3 Backend matrix

- [ ] Run all positive cases against `[backends.html]`.
- [ ] Add one `[backends.html_wasm]` smoke case only if the fixture avoids unsupported runtime features and the existing test matrix supports it cleanly.
- [ ] Prefer normalized output/goldens when available. Do not overfit tests to incidental `pulldown-cmark` whitespace beyond what Beanstalk output behavior requires.

## Phase closeout — audit / style / validation

- [ ] Tests cover import behavior, selected Markdown options, and source-kind boundaries.
- [ ] Tests do not attempt full CommonMark compliance.
- [ ] Failure cases assert stable diagnostic codes.
- [ ] Fixture names are behavior-specific.
- [ ] `tests/cases/manifest.toml` is updated.
- [ ] Run:

  ```bash
  cargo run -- tests
  ```

- [ ] Run `just validate` if available.

---

# Phase 5 — Documentation updates

## Summary / reasoning / context

Documentation is part of the feature. It must clearly distinguish `.md` from `.bd`, because both import as `content` but have different parser, scope, and syntax rules.

## Checklist

### 5.1 Compiler-facing docs

- [ ] Update `docs/language-overview.md` near the Beandown section.
- [ ] Add a “Plain Markdown `.md` Content Files” section documenting:
  - [ ] `.md` files are plain Markdown/CommonMark-style HTML-project content helpers;
  - [ ] they expose exactly `content #String`;
  - [ ] imports are extensionless;
  - [ ] no Beanstalk imports, declarations, interpolation, templates, frontmatter, or metadata;
  - [ ] no same-directory facade scope;
  - [ ] raw HTML is preserved in V1;
  - [ ] Markdown links/images render literally and are not tracked assets in V1;
  - [ ] `.bd` remains Beanstalk-aware and `$markdown`-based.
- [ ] Update `docs/compiler-design-overview.md`:
  - [ ] Stage 0: `.md` is a builder-supported source asset discovered through the same extensionless candidate path.
  - [ ] Stage 1: `.md` bypasses tokenization.
  - [ ] Stage 2: `headers/plain_markdown_prepare.rs` renders raw Markdown to a synthetic literal `content #String` constant.
  - [ ] Later stages: no Markdown-specific AST/HIR/backend path.

### 5.2 User-facing docs-site source

- [ ] Add either a dedicated page or a section:
  - [ ] preferred page if docs navigation supports it: `docs/src/docs/markdown/#page.bst`;
  - [ ] otherwise add a dedicated section to `docs/src/docs/beandown/#page.bst` and cross-link it clearly.
- [ ] If a docs navigation file lists Beandown, add Markdown imports beside it.
- [ ] Include an example:

  ```text
  src/
  ├── #page.bst
  └── docs/
      └── intro.md
  ```

  ```markdown
  # Welcome

  This is **plain Markdown**.
  ```

  ```beanstalk
  import @docs/intro

  [: [intro.content] ]
  ```

- [ ] Include a comparison table:

  | Feature | `.md` | `.bd` |
  |---|---|---|
  | Parser | CommonMark-style dependency | Beanstalk `$markdown` |
  | Beanstalk syntax | No | Yes, template body rules |
  | Generated member | `content #String` | `content #String` |
  | Same-directory facade constants | No | Yes |
  | Frontmatter/metadata | No | No |
  | Raw HTML | Preserved V1 | Through Beanstalk/template semantics |
  | Links/images | Literal output V1 | Template-rendered output |

### 5.3 Progress/index references

- [ ] Search `docs/src/docs/progress` for content-file/source-kind feature matrices and update only if such a matrix exists.
- [ ] Search docs for Beandown references and add a Markdown-import note where users would expect it.
- [ ] Do not update `README.md` unless the README already lists current content-file features.

## Phase closeout — audit / style / validation

- [ ] Docs never imply `.md` supports Beanstalk interpolation.
- [ ] Docs never imply Markdown links/images are tracked assets in V1.
- [ ] Docs clearly say extension imports are invalid.
- [ ] Docs distinguish `$markdown`, `.bd`, and `.md`.
- [ ] Compiler-facing and user-facing docs are both updated.
- [ ] Run the docs build if the repo has a separate docs command.
- [ ] Run `just validate` if available.

---

# Phase 6 — Final cross-stage audit and validation

## Summary / reasoning / context

This feature crosses Stage 0 discovery, source-kind registration, frontend preparation, header generation, AST constant folding, and HTML builder registration. The final phase verifies that Markdown concerns did not leak into later compiler stages and that the implementation removed duplication rather than adding parallel paths.

## Checklist

### 6.1 Cross-stage ownership audit

- [ ] Stage 0:
  - [ ] `.md` is discovered through extensionless source import candidates.
  - [ ] `.md` is not scanned for imports.
  - [ ] Markdown links/images are not discovered as imports or tracked assets.
- [ ] Frontend preparation:
  - [ ] `.md` bypasses `TokenizerEntryMode` and `tokenize_source`.
  - [ ] `.bst` and `.bd` tokenization behavior is unchanged.
- [ ] Header stage:
  - [ ] `plain_markdown_prepare.rs` generates exactly one private `content #String` constant.
  - [ ] `beandown_prepare.rs` and `plain_markdown_prepare.rs` share only the narrow synthetic-content-header helper.
  - [ ] The generated declaration is private unless re-exported through normal facade rules.
- [ ] AST:
  - [ ] Rendered Markdown appears as an ordinary string literal initializer.
  - [ ] There is no Markdown-specific constant folding path.
- [ ] HIR and borrow validation:
  - [ ] No new HIR node kind.
  - [ ] No borrow-checker changes.
- [ ] Backend:
  - [ ] No Markdown-specific backend branch.
  - [ ] HTML builder receives ordinary compiled modules and constants.

### 6.2 Diagnostics audit

- [ ] Existing diagnostics cover:
  - [ ] direct `.md` extension import — `BST-IMPORT-0024`;
  - [ ] unsupported `.md` under a non-registering builder — `BST-IMPORT-0025`;
  - [ ] `.md` as a single-file entry — `BST-IMPORT-0026`;
  - [ ] `.md` collision with `.bst`, `.bd`, or folder — `BST-IMPORT-0006`.
- [ ] Any renderer text adjustments are made in diagnostic renderers, not by adding Markdown-specific diagnostic payloads.
- [ ] No user-facing Markdown/source-kind error is routed through `CompilerError` unless it is a filesystem/tooling failure.

### 6.3 Style-guide audit

- [ ] New modules have file-level docs.
- [ ] `mod.rs` files remain structural maps.
- [ ] Main functions read as named steps, not dense expression chains.
- [ ] No user-input panics, `todo!`, or user-data `.unwrap()`.
- [ ] No broad generic helpers, macro abstractions, or boolean-heavy APIs.
- [ ] No compatibility wrappers or duplicate old/new paths.
- [ ] No stale comments say Beandown when Markdown is meant.
- [ ] No `#[allow(...)]` additions unless justified by an existing project pattern.

### 6.4 Dependency audit

- [ ] Run:

  ```bash
  cargo tree -i pulldown-cmark
  ```

- [ ] Confirm `pulldown-cmark` is a direct dependency only for `plain_markdown.rs`.
- [ ] Confirm `getopts` is not pulled when using `default-features = false`.
- [ ] Confirm no unexpected parser features are enabled.

### 6.5 Final validation

- [ ] Run:

  ```bash
  cargo fmt --check
  cargo clippy --all-targets --all-features
  cargo test
  cargo run -- tests
  just validate
  ```

- [ ] If any command cannot run in the environment, record exactly which command failed to run and why.
- [ ] Manually inspect generated HTML for at least one Markdown import fixture.
- [ ] Record final changed files.
- [ ] Record the final commit hash or relevant file SHAs.
- [ ] Add a PR summary covering:
  - [ ] user-facing behavior;
  - [ ] implementation boundaries;
  - [ ] tests added;
  - [ ] docs updated;
  - [ ] validation commands run.

---

## Expected touched files

Production files:

```text
Cargo.toml
Cargo.lock
src/compiler_frontend/mod.rs
src/compiler_frontend/plain_markdown.rs
src/compiler_frontend/headers/mod.rs
src/compiler_frontend/headers/synthetic_content_header.rs
src/compiler_frontend/headers/beandown_prepare.rs
src/compiler_frontend/headers/plain_markdown_prepare.rs
src/compiler_frontend/pipeline.rs
src/compiler_frontend/tokenizer/tokens.rs
src/libraries/source_file_kind_registry.rs
src/build_system/create_project_modules/reachable_file_discovery.rs
src/projects/html_project/html_project_builder.rs
```

Unit test files:

```text
src/compiler_frontend/tests/plain_markdown_tests.rs
src/compiler_frontend/headers/tests/plain_markdown_prepare_tests.rs
src/compiler_frontend/headers/tests/beandown_prepare_tests.rs      # update only if helper refactor requires expected-shape assertions
src/libraries/tests/source_file_kind_registry_tests.rs
```

Integration test files:

```text
tests/cases/manifest.toml
tests/cases/html_markdown_import_basic/**
tests/cases/html_markdown_grouped_import_alias/**
tests/cases/html_markdown_plain_not_beanstalk/**
tests/cases/html_markdown_tables_tasks_code/**
tests/cases/html_markdown_raw_html_preserved/**
tests/cases/html_markdown_facade_reexport/**
tests/cases/html_markdown_literal_relative_links/**
tests/cases/html_markdown_direct_extension_import_rejected/**
tests/cases/html_markdown_collision_with_beandown/**
tests/cases/html_markdown_collision_with_bst/**
tests/cases/html_markdown_collision_with_folder/**
tests/cases/html_markdown_entry_rejected/**                # or focused build-system test if runner support is poor
tests/cases/html_markdown_unsupported_builder_source_kind/** # only if test harness can express this cleanly
```

Docs files:

```text
docs/language-overview.md
docs/compiler-design-overview.md
docs/src/docs/markdown/#page.bst       # preferred if docs nav supports it
docs/src/docs/beandown/#page.bst       # update or cross-link
docs/src/docs/progress/#page.bst       # only if the feature matrix tracks this surface
```

---

## Acceptance criteria

- [ ] HTML projects can import `intro.md` with `import @docs/intro`.
- [ ] The imported namespace exposes `content` as a usable string.
- [ ] Grouped import aliases work for `content`.
- [ ] Markdown renders to HTML with the selected CommonMark/GFM-like options.
- [ ] `.md` content containing Beanstalk-looking syntax is not parsed as Beanstalk.
- [ ] Direct `.md` extension imports are rejected with `BST-IMPORT-0024`.
- [ ] `.md` collisions with `.bst`, `.bd`, or folders are rejected with `BST-IMPORT-0006`.
- [ ] `.md` cannot be used as a build entry.
- [ ] `.md` is unsupported under builders that do not register it.
- [ ] Markdown links and images render literally and do not become tracked assets.
- [ ] No HIR, borrow-checker, or backend-specific Markdown path exists.
- [ ] `.bd` behavior is unchanged after synthetic-header helper consolidation.
- [ ] Docs describe `.md` and distinguish it from `.bd`.
- [ ] `just validate` passes, or any environment limitation is recorded.
