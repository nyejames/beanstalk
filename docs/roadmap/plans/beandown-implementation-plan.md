# Beandown `.bd` Implementation Plan

## Goal

Implement **Beandown** `.bd` files as HTML-builder-supported Beanstalk content assets.

A `.bd` file is authored as a template body and is compiled as if it were the body of this generated compile-time template:

```beanstalk
content #String = [$markdown:
    ...entire .bd file body...
]
```

The compiler must build this structurally. Do not prepend/append wrapper source text.

`.bd` files are never page entries, module roots, config files, or standalone Beanstalk modules. They are simple content helpers for `.bst` files under builders that explicitly support Beandown. V1 support is owned by the existing HTML project builder.

## Current repo anchors

This plan is anchored to the current `main` branch shape and assumes the explicit `export` module-facade refactor as the baseline.

Key current-state anchors:

- `BackendBuilder` lives in `src/build_system/build.rs` and exposes `build_backend`, `validate_project_config`, `frontend_style_directives`, and `libraries`.
- `LibrarySet` in `src/libraries/library_set.rs` already carries builder-declared frontend-visible surface: source libraries, external packages, config keys, external import providers, provider cache/table state, and builder runtime packages.
- `src/build_system/create_project_modules/mod.rs` owns Stage 0 orchestration and delegates to focused modules: `reachable_file_discovery`, `import_scanning`, `module_inventory`, `source_loading`, `project_roots`, and diagnostics.
- `src/build_system/create_project_modules/reachable_file_discovery.rs` currently walks `.bst` import graphs, seeds source-library facades, extracts imports from each reachable file, and resolves imports through `ProjectPathResolver`.
- `src/build_system/create_project_modules/import_scanning.rs` tokenizes source files in normal mode and extracts import paths. `.bd` files must not use this path because they have no import syntax.
- `ProjectPathResolver` in `src/compiler_frontend/paths/path_resolution.rs` currently resolves extensionless imports by checking `.bst` candidates through `candidate_import_files` in `path_normalization.rs`.
- `candidate_import_files` currently produces `.bst` candidates only. This is the correct central place to generalize candidate collection instead of scattering `.bd` checks.
- `CompilerFrontend::prepare_file_frontend_local` tokenizes every input as `TokenizeMode::Normal` before header parsing. It must become source-kind-aware.
- `TokenizeMode` currently models lexical state only: `Normal`, `TemplateBody`, and `TemplateHead`.
- `TokenStream::pop_template_mode` already keeps the initial template frame from escaping back to normal mode, but an outer-depth `]` can still emit `TemplateClose`. Beandown needs a structured diagnostic there.
- `tokenize_template_body` already treats `\` as an escape introducer, so `\]` becomes a literal `]`. Keep this behavior and add tests.
- Header data already includes explicit `HeaderExportMode` and per-file import outputs from the facade-export work. Do not duplicate that refactor.
- `DeclarationSyntax` stores initializer tokens and initializer reference hints. Synthetic Beandown constants should reuse this shape.
- `src/projects/html_project/mod.rs` is the HTML builder module map. Add the public/internal Beandown API under this tree.
- `src/projects/html_project/html_project_builder.rs` already registers `@html`, exposes HTML core libraries, registers web/canvas runtime packages, and returns HTML style directives. Reuse these existing registration paths.

Primary files and areas:

| Area | Current paths |
|---|---|
| Builder/library capability surface | `src/libraries/library_set.rs`, `src/build_system/build.rs` |
| Stage 0 discovery | `src/build_system/create_project_modules/**` |
| Import candidate/path resolution | `src/compiler_frontend/paths/path_resolution.rs`, `src/compiler_frontend/paths/path_normalization.rs`, `src/compiler_frontend/paths/import_resolution.rs` |
| Tokenizer | `src/compiler_frontend/tokenizer/tokens.rs`, `lexer.rs`, `text_modes.rs` |
| Header data/preparation | `src/compiler_frontend/headers/types.rs`, `file_state.rs`, `file_parser.rs`, `file_imports.rs` |
| Module symbols/visibility | `src/compiler_frontend/headers/module_symbols.rs`, `symbol_collection.rs`, `facade_data.rs`, `import_environment/**` |
| Dependency sorting | `src/compiler_frontend/module_dependencies.rs` |
| AST constants/templates | `src/compiler_frontend/ast/**`, `src/compiler_frontend/optimizers/constant_folding.rs` |
| HTML builder | `src/projects/html_project/**` |
| HTML source library | `libraries/html/#mod.bst`, `libraries/html/**` |
| Docs/matrix | `docs/language-overview.md`, `docs/compiler-design-overview.md`, `docs/src/docs/**`, `docs/src/docs/progress/#page.bst`, `docs/roadmap/roadmap.md` |
| Tests | `src/compiler_tests/**`, `tests/cases/**`, module-local `tests/` directories |

## Design contract

### File semantics

- Extension: `.bd`.
- Name: Beandown.
- A `.bd` file starts inside an implicit template body.
- The implicit outer template always applies `$markdown`.
- There is no outer opt-out from `$markdown`.
- `.bd` has no import syntax, declarations, config, frontmatter, metadata block, or document-level directives.
- Nested explicit Beanstalk templates are supported and use normal template syntax.
- `--` is plain template body text, not a comment. Use `$doc`, `$note`, and `$todo` directives for template comments.
- Empty `.bd` files compile successfully to `content == ""`.
- More complex composition belongs in `.bst`.

### Import surface

Every `.bd` file exposes one generated constant on its normal import surface:

```beanstalk
content #String
```

Rules:

- `content` is a normal `String` constant.
- `content` is the rendered post-`$markdown` folded string.
- The generated string is exactly the fragment content. No HTML document wrapper, file writing, output folder mirroring, or generated `.html` artifact.
- Raw `.bd` source is not preserved in output metadata or import surfaces.
- `content` is not reserved. It exists only as a field on the `.bd` import record, for example `intro.content`.
- The generated `content` for a `.bd` file is not injected into that file’s own body scope.

### Visibility inside `.bd`

A `.bd` body sees a restricted compile-time constant scope:

1. every exported compile-time top-level constant/const record from `@html`;
2. every exported compile-time constant/const record from the same-directory `#mod.bst`, if that file exists and has been supplied by the compiler/caller.

Rules:

- The `@html` constants are visible flat: `[p: ...]`, not `[html.p: ...]`.
- The `@html` import is dumb/general. Do not maintain a curated Beandown constant list.
- The same filtering rule applies to `@html` and the same-directory facade: constants/const records only.
- Functions, structs, choices, type aliases, traits, methods, runtime bindings, external functions, and JS-backed runtime APIs are not visible.
- Same-directory facade constants override `@html` constants on name collision.
- Same-directory means the exact directory containing the `.bd` file. Do not search ancestors or child folders.
- If there is no same-directory `#mod.bst`, only `@html` constants are visible.
- Standalone/public API use never searches for project/module context. Any extra constants must be explicitly supplied by the caller.
- Caller-supplied constants must already be folded.
- If a same-directory facade re-exports the same `.bd` file, that self-originating export is excluded from that `.bd` file’s body scope.

### Imports from `.bst`

Use regular extensionless source import syntax:

```beanstalk
import @docs/intro

[page:
    [intro.content]
]
```

Grouped imports also work:

```beanstalk
import @docs/intro {
    content as intro_content,
}
```

Rules:

- Direct extension imports are rejected: `import @docs/intro.bd` is invalid.
- `.bd` imports are accepted only when the active builder supports Beandown.
- HTML builder supports Beandown in v1.
- Other builders should report “recognized but unsupported by this builder” when `intro.bd` exists but the builder does not support `.bd`.
- `.bd` files do not create module roots.
- `.bd` files are not compiled merely because they exist under `entry_root`.
- `.bd` files are never valid HTML page entries.
- Facades can re-export Beandown content with explicit facade export syntax:

```beanstalk
-- docs/#mod.bst
export @./intro {
    content as intro,
}
```

### Path and collision rules

- `import @docs/intro` may resolve to `docs/intro.bd` only when the active builder supports `.bd`.
- `intro.bst` + `intro.bd` is ambiguous.
- `intro.bd` + `intro/` is ambiguous.
- `intro.bst` + `intro/` remains ambiguous as today.
- Do not use extension priority.
- Duplicate paths passed to the Beandown public API are diagnostics, not silent deduplication.

### Public/internal API

Beandown’s callable API lives under the HTML project tree, for example:

```text
src/projects/html_project/beandown/
```

V1 exposes a narrow, readable API for internal compiler/tooling use. Keep the implementation mostly `pub(crate)`, with a concise public wrapper only where it matches current crate API patterns.

Suggested public-facing shape:

```rust
pub fn compile_beandown(
    request: BeandownCompileRequest,
    string_table: &mut StringTable,
) -> Result<BeandownCompileOutput, CompilerMessages>;

pub struct BeandownCompileRequest {
    pub input: BeandownInput,
    pub default_module_constants: Vec<BeandownScopeConstant>,
    pub module_constants_by_path: Vec<BeandownPathScope>,
}

pub enum BeandownInput {
    File(PathBuf),
    Directory { path: PathBuf, recursive: bool },
    Files(Vec<PathBuf>),
    Sources(Vec<BeandownSource>),
}

pub struct BeandownSource {
    pub display_path: PathBuf,
    pub source_text: String,
}

pub struct BeandownPathScope {
    pub source_path: PathBuf,
    pub constants: Vec<BeandownScopeConstant>,
}

pub struct BeandownCompileOutput {
    pub documents: Vec<CompiledBeandownDocument>,
    pub warnings: Vec<CompilerDiagnostic>,
}

pub struct CompiledBeandownDocument {
    pub source_path: PathBuf,
    pub relative_path: Option<PathBuf>,
    pub content: String,
}
```

Use existing folded-value/compiler structures internally. Do not expose AST constants, synthetic headers, `StringId`s, or folded-value internals from the public output.

### Deferred / non-goals

Document these in docs, roadmap, and the progress matrix:

- CLI command such as `bean beandown`.
- `project = "beandown"`.
- Generated `.html` files, output folders, or artifact cleanup behavior.
- Direct `.bd` page entries.
- Import syntax inside `.bd`.
- `.bd` composition from within `.bd`.
- Frontmatter, metadata, raw source retention, heading extraction, TOC extraction, summaries.
- Curated Beandown-only HTML constants.
- Regular Markdown/CSS/JSON source handlers. V1 adds the scaffold only; `.bd` is the only implemented non-`.bst` source kind.
- Wildcard imports/exports.
- Namespace facade exports.
- Direct extension imports such as `import @docs/intro.bd`.

## Implementation phases

Each phase should be completed as one coherent coding-agent chunk and must end with audit/style/validation.

---

## Phase 0 — Baseline audit and current-shape confirmation

### Context

The explicit `export` facade refactor is the baseline. Beandown must build on that shape and avoid reintroducing old automatic facade export behavior.

### Checklist

- [ ] Record `git status --short` and current HEAD.
- [ ] Confirm explicit facade export pieces are present and passing:
  - [ ] `TokenKind::Export` and keyword reservation.
  - [ ] `HeaderExportMode` on headers/imports.
  - [ ] per-file imports stored once in `FileFrontendPrepareOutput`.
  - [ ] facade export maps built only from explicit `export` headers/imports.
  - [ ] `export @path { ... }` and `export import @path { ... }` behavior.
- [ ] Inspect current source discovery and path resolution:
  - [ ] `src/build_system/create_project_modules/reachable_file_discovery.rs`
  - [ ] `src/build_system/create_project_modules/import_scanning.rs`
  - [ ] `src/compiler_frontend/paths/path_resolution.rs`
  - [ ] `src/compiler_frontend/paths/path_normalization.rs`
- [ ] Inspect tokenizer entry behavior and template body escapes:
  - [ ] `src/compiler_frontend/tokenizer/tokens.rs`
  - [ ] `src/compiler_frontend/tokenizer/lexer.rs`
  - [ ] `src/compiler_frontend/tokenizer/text_modes.rs`
- [ ] Inspect header/AST constant paths:
  - [ ] `src/compiler_frontend/declaration_syntax/declaration_shell.rs`
  - [ ] `src/compiler_frontend/headers/types.rs`
  - [ ] `src/compiler_frontend/pipeline.rs`
  - [ ] relevant AST constant/template folding modules.
- [ ] Inspect HTML builder surface:
  - [ ] `src/projects/html_project/mod.rs`
  - [ ] `src/projects/html_project/html_project_builder.rs`
  - [ ] `src/projects/html_project/style_directives.rs`
  - [ ] `libraries/html/#mod.bst`
- [ ] Inventory integration tests involving:
  - [ ] source-library imports;
  - [ ] module facades;
  - [ ] grouped imports;
  - [ ] template folding;
  - [ ] path/import collision diagnostics;
  - [ ] HTML JS / HTML-Wasm builder paths.

### Phase-end audit / validation

- [ ] No production code changes unless they are needed to update stale test comments.
- [ ] Record existing failures separately from Beandown work.
- [ ] Run:
  - [ ] `cargo fmt --check`
  - [ ] `cargo test`
  - [ ] `cargo run -- tests`
  - [ ] `just validate` if available.

---

## Phase 1 — Builder-supported source-kind registry

### Context

Do not add a new `BackendBuilder` method unless the audit proves it is necessary. The lower-churn path is to extend `LibrarySet`, because builders already return it to declare frontend-visible library/package/provider/config surface.

### Checklist

- [ ] Add a small source-kind registry under `src/libraries/`, for example:
  - [ ] `src/libraries/source_file_kinds.rs`, or
  - [ ] `src/libraries/source_file_kind_registry.rs`.
- [ ] Suggested types:

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SourceFileKind {
    Beanstalk,
    Beandown,
}

pub struct SupportedSourceFileKind {
    pub extension: &'static str,
    pub kind: SourceFileKind,
}

#[derive(Clone, Debug)]
pub struct SourceFileKindRegistry { ... }
```

- [ ] Keep `.bst` as compiler-owned built-in source kind; builders should not need to register it.
- [ ] Add `source_file_kinds: SourceFileKindRegistry` to `LibrarySet`.
- [ ] Initialize it empty in `LibrarySet::with_mandatory_core()`.
- [ ] Register `.bd` / `SourceFileKind::Beandown` in `HtmlProjectBuilder::libraries()`.
- [ ] Do not register Beandown in other builders/test builders unless the test explicitly needs support.
- [ ] Thread the registry into Stage 0 and `ProjectPathResolver` creation.
- [ ] Keep the registry extension-driven, but keep behavior dispatch typed through `SourceFileKind`.
- [ ] Do not add a broad `SourceFileHandler` trait in v1 unless implementation proves a registry-only model is insufficient.
- [ ] Add focused tests for registry registration and lookup.

### Phase-end audit / validation

- [ ] Confirm `.bd` support is builder-declared through `LibrarySet`, not hard-coded globally.
- [ ] Confirm no regular Markdown/CSS/JSON behavior was implemented.
- [ ] Confirm no noisy trait/object abstraction was introduced for one file type.
- [ ] Run:
  - [ ] focused library/registry tests
  - [ ] `cargo fmt`
  - [ ] `cargo test`

---

## Phase 2 — Source-kind-aware path resolution and Stage 0 discovery

### Context

Import resolution currently centralizes `.bst` extension fallback in `candidate_import_files`. Extend that path rather than adding ad hoc `.bd` checks in discovery or AST.

### Checklist

- [ ] Replace or extend `candidate_import_files` so it can build typed source candidates:

```rust
pub struct ImportCandidate {
    pub path: PathBuf,
    pub kind: SourceFileKind,
}
```

- [ ] Candidate collection should check:
  - [ ] `name.bst`;
  - [ ] builder-supported `name.<extension>` candidates such as `name.bd`;
  - [ ] `name/` folder where existing facade fallback/module-root behavior applies.
- [ ] Detect unsupported recognized candidates:
  - [ ] if `name.bd` exists but active builder does not support `.bd`, return a structured unsupported-source-kind diagnostic;
  - [ ] do not fall through to missing import target.
- [ ] Reject direct source extension imports:
  - [ ] existing `.bst` extension diagnostic remains;
  - [ ] add equivalent `.bd` direct-extension diagnostic;
  - [ ] ensure `.js` provider-backed imports still use provider logic.
- [ ] Preserve provider-backed import behavior:
  - [ ] when an explicit extension is provider-owned, route to external import provider;
  - [ ] when an explicit extension is source-kind-owned (`.bd`), reject direct extension import;
  - [ ] when extension is unknown and no provider supports it, keep unsupported external extension diagnostics.
- [ ] Add source kind to Stage 0 loaded inputs. Prefer replacing `InputFile` with or extending it to include `source_kind`:

```rust
pub struct InputFile {
    pub source_code: String,
    pub source_path: PathBuf,
    pub source_kind: SourceFileKind,
}
```

- [ ] Update reachable-file BFS to track `(PathBuf, SourceFileKind)` rather than bare paths.
- [ ] Skip import scanning for `.bd` files. They contain no import syntax.
- [ ] When a `.bd` file is reachable, queue its same-directory `#mod.bst` if present so facade constants can be available later.
- [ ] Ensure `.bd` files do not create module roots. Module root discovery should remain based on `#*.bst` files.
- [ ] Ensure unimported `.bd` files under `entry_root` are not discovered or compiled.
- [ ] Ensure source libraries follow the same source-kind candidate rules.
- [ ] Add tests:
  - [ ] `import @docs/intro` resolves `intro.bd` under HTML builder;
  - [ ] `intro.bd` found under non-HTML builder reports unsupported source kind;
  - [ ] direct `import @docs/intro.bd` rejected;
  - [ ] `.bd` + `.bst` same stem ambiguous;
  - [ ] `.bd` + folder same stem ambiguous;
  - [ ] `.js` provider imports still work;
  - [ ] unimported `.bd` is ignored.

### Phase-end audit / validation

- [ ] Confirm import resolution still has one candidate collection path.
- [ ] Confirm there is no extension-priority ordering.
- [ ] Confirm provider-backed imports did not regress.
- [ ] Run:
  - [ ] focused path/import tests
  - [ ] `cargo test`
  - [ ] `cargo run -- tests`

---

## Phase 3 — Tokenizer entry policy for Beandown bodies

### Context

`TokenizeMode` should remain the current lexical state. Beandown also needs an entry policy: start in body mode, but reject closing the implicit outer body.

### Checklist

- [ ] Add `TokenizerEntryMode` or equivalent near `TokenizeMode`:

```rust
pub enum TokenizerEntryMode {
    SourceFile,
    TemplateBody {
        initial_close_policy: InitialTemplateClosePolicy,
    },
}

pub enum InitialTemplateClosePolicy {
    Allow,
    RejectOuterClose { source_kind: SourceFileKind },
}
```

- [ ] Update `tokenize`, `TokenStream::new`, `CompilerFrontend::tokenize_source`, and test helpers to use the entry policy.
- [ ] Avoid compatibility shims that hide the source entry mode. Update call sites explicitly.
- [ ] Add initial-frame close-policy metadata to `TokenStream` or the initial `TemplateModeFrame`.
- [ ] In `lexer.rs`, when `]` appears at the initial Beandown body frame, emit a `CompilerDiagnostic` instead of `TokenKind::TemplateClose`.
- [ ] Diagnostic requirements:
  - [ ] source kind/path context;
  - [ ] explain that `.bd` starts inside an implicit template body;
  - [ ] suggest `\]` and `[']']`.
- [ ] Keep nested explicit templates unchanged: nested `[` enters `TemplateHead`; its matching `]` closes normally.
- [ ] Audit existing template-body escapes and add tests:
  - [ ] `\]` -> literal `]` in normal templates;
  - [ ] `\[` -> literal `[` in normal templates;
  - [ ] escaped backslash behavior;
  - [ ] same cases in Beandown entry mode.
- [ ] If any escape behavior is missing, fix `tokenize_template_body` generally, not a Beandown-only path.
- [ ] Add test that `--` remains text in Beandown body mode.

### Phase-end audit / validation

- [ ] Confirm tokenizer policy does not encode parser/AST semantics beyond initial close handling.
- [ ] Confirm source locations for diagnostics point at the original `.bd` source.
- [ ] Confirm normal `.bst` tokenization is unchanged except for improved escape coverage.
- [ ] Run:
  - [ ] focused tokenizer tests
  - [ ] template tests
  - [ ] `cargo test`

---

## Phase 4 — Synthetic Beandown file preparation

### Context

A `.bd` file should enter the module as a normal source file that contributes one synthetic constant declaration. It should not be parsed as `.bst`, and it should not create a Beandown-only AST node.

### Checklist

- [ ] Make `CompilerFrontend::prepare_file_frontend_local` branch by `input.source_kind`.
- [ ] For `SourceFileKind::Beanstalk`, keep the current normal tokenization/header parser path.
- [ ] For `SourceFileKind::Beandown`, call a focused preparer owned by the header/frontend stage, for example:
  - [ ] `src/compiler_frontend/headers/beandown_prepare.rs`, or
  - [ ] `src/compiler_frontend/special_sources/beandown.rs` if that module shape is cleaner.
- [ ] The preparer should:
  - [ ] tokenize source text with Beandown template-body entry mode;
  - [ ] create a synthetic `DeclarationSyntax` for `content #String = <template>`;
  - [ ] set initializer tokens to a structurally generated template expression with `$markdown` and the original body tokens;
  - [ ] use original `.bd` locations for body tokens;
  - [ ] use a stable file-start/synthetic location for generated `content` when needed;
  - [ ] collect initializer references through the existing `collect_initializer_references` helper;
  - [ ] produce no `file_imports`;
  - [ ] produce no start function;
  - [ ] produce no top-level const fragments;
  - [ ] preserve `canonical_os_path` and file identity.
- [ ] Reuse `HeaderKind::Constant` if possible.
- [ ] Do not set `HeaderExportMode::Public` merely because this is `.bd`. `HeaderExportMode::Public` is for explicit `export` entries in `#mod.bst`; ordinary `.bd` `content` should follow normal ordinary-file importability and facade re-export rules.
- [ ] Ensure the generated `content` constant is not visible in the `.bd` file’s own body scope.
- [ ] Ensure `#mod.bst` can import/re-export `.bd` content through normal explicit facade export syntax.
- [ ] Add tests:
  - [ ] empty `.bd` produces an empty folded string;
  - [ ] simple markdown heading matches equivalent `[$markdown:]` template;
  - [ ] nested templates parse;
  - [ ] unescaped outer `]` diagnostic;
  - [ ] escaped `\]` works;
  - [ ] `--` remains body text;
  - [ ] text that looks like imports/declarations remains body text or template syntax according to normal template rules.

### Phase-end audit / validation

- [ ] Confirm header parsing remains the owner of top-level declaration discovery.
- [ ] Confirm no text wrapper concatenation was used.
- [ ] Confirm no Beandown-specific AST node was introduced.
- [ ] Confirm `.bd` files do not generate runtime/start/HIR content.
- [ ] Run:
  - [ ] focused header/preparation tests
  - [ ] `cargo test`
  - [ ] `cargo run -- tests`

---

## Phase 5 — Restricted Beandown const scope

### Context

Beandown needs a small implicit constant scope, not a second import system. Build it by reusing existing facade/export and folded constant data.

### Checklist

- [ ] Identify the existing folded constant representation used after AST constant folding.
- [ ] Add the smallest public wrapper only if existing internal types are too noisy for the Beandown API.
- [ ] Implement a Beandown scope builder with two layers:
  - [ ] HTML layer: exported folded constants/const records from `@html`;
  - [ ] module layer: caller/same-directory facade exported folded constants/const records.
- [ ] Merge order:
  - [ ] HTML constants first;
  - [ ] module/facade constants second;
  - [ ] module/facade overrides HTML on name collision.
- [ ] Do not curate the HTML list. The filter is “exported folded constants/const records only”.
- [ ] Validate supplied scope items:
  - [ ] constants/const records only;
  - [ ] already folded;
  - [ ] no functions/types/traits/methods/runtime values/external functions;
  - [ ] duplicate names inside one supplied layer rejected as a boundary invariant violation.
- [ ] For compiler-integrated `.bd` imports:
  - [ ] locate only the same-directory `#mod.bst`;
  - [ ] use explicit `export` facade data;
  - [ ] include exported constants/const records only;
  - [ ] exclude exports that originate from the same `.bd` file being compiled;
  - [ ] do not search ancestor/child facades.
- [ ] Represent the scope as file-local implicit constant visibility for synthetic `.bd` files after facade data is available. Do not synthesize wildcard imports or user-visible import records.
- [ ] Ensure const-record field access uses existing folded-template expression behavior.
- [ ] Add tests:
  - [ ] flat `@html` constants visible;
  - [ ] same-directory facade constants visible;
  - [ ] no same-directory facade means only `@html`;
  - [ ] facade constants override `@html` names;
  - [ ] functions/types/methods not visible;
  - [ ] const-record field access works;
  - [ ] facade-supplied `content` can be referenced normally;
  - [ ] generated self `content` is not in scope;
  - [ ] self-originating facade re-export excluded;
  - [ ] duplicate caller-provided scope rejected by API.

### Phase-end audit / validation

- [ ] Confirm this is not a general wildcard-import system.
- [ ] Confirm Beandown does not resolve imports/facades independently of existing facade data.
- [ ] Confirm ordinary duplicate declaration checks in `.bst`/facades remain intact.
- [ ] Run:
  - [ ] focused visibility/scope tests
  - [ ] facade export tests
  - [ ] `cargo test`
  - [ ] `cargo run -- tests`

---

## Phase 6 — AST folding and no-HIR Beandown extraction

### Context

All Beandown content must fold to one compile-time string. Imported `.bd` constants flow through normal module compilation; the direct Beandown API should stop after AST/folding.

### Checklist

- [ ] Reuse AST constant/template folding for synthetic `content`.
- [ ] Do not duplicate markdown/template rendering in `html_project/beandown`.
- [ ] Add or extract a narrow helper for direct API use that can:
  - [ ] compile one synthetic `.bd` file through tokenization/header/dependency/AST;
  - [ ] apply a prepared Beandown const scope;
  - [ ] extract the folded `content` string;
  - [ ] stop before HIR generation and borrow validation.
- [ ] Ensure imported `.bd` files in normal modules expose `content` as normal compile-time metadata and do not introduce runtime HIR nodes.
- [ ] Reject anything that cannot fold:
  - [ ] runtime bindings;
  - [ ] runtime calls;
  - [ ] external calls;
  - [ ] dynamic template control flow;
  - [ ] unknown/non-foldable values.
- [ ] Reuse normal const-template/folding diagnostics wherever possible.
- [ ] Add Beandown-specific diagnostics only for feature boundaries:
  - [ ] unescaped outer `]`;
  - [ ] unsupported builder/source kind;
  - [ ] direct `.bd` extension import;
  - [ ] `.bd` as entry/page;
  - [ ] invalid public API scope item/input.
- [ ] Add tests:
  - [ ] compile-time `if`/`loop` folds;
  - [ ] runtime/unknown condition rejected;
  - [ ] runtime function call rejected;
  - [ ] external/runtime HTML API not visible;
  - [ ] folded `.bd` content usable as a normal `String` constant in `.bst`.

### Phase-end audit / validation

- [ ] Confirm AST remains the owner of constant/template folding.
- [ ] Confirm HIR remains Beandown-free.
- [ ] Confirm direct Beandown API does not run borrow validation.
- [ ] Run:
  - [ ] focused AST/folding tests
  - [ ] `cargo test`
  - [ ] `cargo run -- tests`

---

## Phase 7 — HTML-project Beandown API

### Context

Expose a small callable Beandown API under the HTML project builder tree. This API consumes files/sources and returns strings; it writes no artifacts.

### Checklist

- [ ] Add `src/projects/html_project/beandown/`.
- [ ] Suggested module layout:
  - [ ] `mod.rs` — module docs, public wrapper exports, ownership/non-goals;
  - [ ] `input.rs` — `BeandownInput`, `BeandownSource`, path collection;
  - [ ] `scope.rs` — public scope wrapper and conversion to internal folded constants;
  - [ ] `compile.rs` — orchestration;
  - [ ] `output.rs` — output types;
  - [ ] `tests/` — tests outside production files.
- [ ] Wire `pub(crate) mod beandown;` into `src/projects/html_project/mod.rs`.
- [ ] Expose a narrow public wrapper if consistent with current crate API policy.
- [ ] Mark the public wrapper experimental/internal-use-first in Rust docs.
- [ ] Input behavior:
  - [ ] `File(PathBuf)` compiles one `.bd` file;
  - [ ] `Directory { recursive: false }` compiles direct child `.bd` files;
  - [ ] `Directory { recursive: true }` compiles descendant `.bd` files;
  - [ ] `Files(Vec<PathBuf>)` preserves caller order;
  - [ ] `Sources(Vec<BeandownSource>)` compiles in-memory source text.
- [ ] Duplicate source paths/display paths are diagnostics.
- [ ] Directory inputs return documents sorted by normalized relative path.
- [ ] `relative_path` is relative to the requested directory for directory inputs; use `None` where no meaningful root exists.
- [ ] Output contains path metadata, compiled content, and warnings only.
- [ ] On failure, return `CompilerMessages`; do not return partial output.
- [ ] Do not write files, create output folders, emit `.html`, or use cleanup policy.
- [ ] Add tests:
  - [ ] file input;
  - [ ] direct directory input;
  - [ ] recursive directory input;
  - [ ] explicit file list order;
  - [ ] in-memory sources;
  - [ ] duplicate input diagnostic;
  - [ ] no artifact side effects.

### Phase-end audit / validation

- [ ] Confirm `mod.rs` is a structural map with clear ownership docs.
- [ ] Confirm there is no `src/projects/beandown/`.
- [ ] Confirm there is no `project = "beandown"`.
- [ ] Confirm no CLI behavior was added.
- [ ] Run:
  - [ ] focused Beandown API tests
  - [ ] `cargo test`
  - [ ] `cargo run -- tests`

---

## Phase 8 — HTML builder integration for `.bst` imports

### Context

This phase wires Beandown into real HTML project builds. `.bst` files should import `.bd` content through normal import records and use `intro.content` anywhere a compile-time string constant is valid.

### Checklist

- [ ] Ensure `HtmlProjectBuilder::libraries()` registers `.bd` support through `LibrarySet` source-file kinds.
- [ ] Ensure HTML style directives are available for Beandown nested templates through the existing `frontend_style_directives()` path.
- [ ] Ensure `@html` scope is derived through existing HTML source-library/facade compilation, not a hard-coded Beandown list.
- [ ] Wire `.bd` generated `content` into module symbols/import environment so these work:

```beanstalk
import @docs/intro
intro.content
```

```beanstalk
import @docs/intro {
    content as intro_content,
}
```

- [ ] Wire explicit facade re-export support:

```beanstalk
export @./intro {
    content as intro,
}
```

- [ ] Ensure `.bd` imports work from project source folders and source-library roots only when the active builder supports Beandown.
- [ ] Ensure `.bd` is rejected as an HTML page/entry file.
- [ ] Ensure `.bd` files do not affect runtime asset tracking, JS glue, external runtime imports, output path planning, or tracked asset emission.
- [ ] Ensure HTML JS and HTML-Wasm paths see the same folded string constants.
- [ ] Add integration tests/goldens:
  - [ ] page imports `.bd` namespace and renders `content`;
  - [ ] grouped import aliases `content`;
  - [ ] facade re-exports `.bd` content;
  - [ ] `.bd` in source library imported from app code;
  - [ ] `.bd` direct entry rejected;
  - [ ] unimported `.bd` ignored;
  - [ ] `.bd` content produces no extra output artifact;
  - [ ] HTML JS and HTML-Wasm parity where applicable.

### Phase-end audit / validation

- [ ] Confirm Beandown support is HTML-builder-owned, not globally enabled.
- [ ] Confirm `.bd` contributes constants only, not runtime code/assets.
- [ ] Run:
  - [ ] HTML builder integration tests
  - [ ] `cargo run -- tests`
  - [ ] `just validate`

---

## Phase 9 — Diagnostics and negative coverage

### Context

Beandown is deliberately small. Negative tests should protect that boundary.

### Checklist

- [ ] Add or reuse typed diagnostics with stable codes for:
  - [ ] unsupported source kind for active builder;
  - [ ] direct `.bd` extension import;
  - [ ] ambiguous import involving `.bd`;
  - [ ] `.bd` used as direct entry/page;
  - [ ] unescaped Beandown outer `]`;
  - [ ] invalid Beandown API scope item;
  - [ ] duplicate Beandown API input path;
  - [ ] Beandown body cannot fully fold.
- [ ] Negative fixtures:
  - [ ] `import @docs/intro.bd` rejected;
  - [ ] `.bd` under non-HTML builder rejected as recognized unsupported source kind;
  - [ ] `.bd` + `.bst` same stem ambiguous;
  - [ ] `.bd` + folder same stem ambiguous;
  - [ ] runtime function call in `.bd` rejected;
  - [ ] function/type exported by facade is not visible in `.bd`;
  - [ ] external/runtime HTML API not visible in `.bd`;
  - [ ] unescaped `]` diagnostic includes suggestions;
  - [ ] `.bd` as entry rejected with design-constraint explanation.
- [ ] Prefer `diagnostic_codes` in `expect.toml`.
- [ ] Use rendered text assertions only where the wording itself is the behavior under test.

### Phase-end audit / validation

- [ ] Confirm user-facing mistakes use `CompilerDiagnostic`.
- [ ] Confirm infrastructure failures stay on `CompilerError`.
- [ ] Confirm no `panic!`, `todo!`, or user-input `.unwrap()` paths were added.
- [ ] Run:
  - [ ] failure fixtures
  - [ ] `cargo test`
  - [ ] `cargo run -- tests`
  - [ ] `just validate`

---

## Phase 10 — Documentation, roadmap, and progress matrix

### Context

Docs must describe Beandown as a simple content-helper format for HTML projects, not a second page system.

### Checklist

- [ ] Update `docs/language-overview.md`:
  - [ ] add a concise Beandown subsection near templates/imports;
  - [ ] document `.bd` as an implicit `$markdown` template body;
  - [ ] document generated `content #String`;
  - [ ] document extensionless import syntax;
  - [ ] document flat implicit `@html` compile-time constants;
  - [ ] document same-directory explicit facade constants;
  - [ ] document compile-time-only/foldability;
  - [ ] document `.bd` is never an entry/page;
  - [ ] document direct `.bd` extension imports are invalid;
  - [ ] document no imports/frontmatter/metadata/raw-source preservation.
- [ ] Update `docs/compiler-design-overview.md`:
  - [ ] document builder-supported source-file-kind registry in `LibrarySet`;
  - [ ] document Stage 0 discovery of builder-supported source assets;
  - [ ] document `TokenizerEntryMode` vs `TokenizeMode`;
  - [ ] document synthetic Beandown `content` constant preparation;
  - [ ] document AST-folding/no-HIR direct API boundary.
- [ ] Update docs-site source under `docs/src/docs/**`:
  - [ ] add/update a Beandown user-facing page after inspecting the docs tree;
  - [ ] show `.bd` examples;
  - [ ] show `.bst` import examples;
  - [ ] show facade re-export example;
  - [ ] explicitly say `.bst` is for pages/composition.
- [ ] Update `docs/src/docs/progress/#page.bst`:
  - [ ] add Beandown `.bd` support status;
  - [ ] add builder-supported source-kind scaffold status;
  - [ ] add deferred CLI/project-type/direct-entry/frontmatter/future-source-handler rows.
- [ ] Update `docs/roadmap/roadmap.md` or relevant roadmap/plans page:
  - [ ] list deferred CLI;
  - [ ] list deferred generic Markdown/CSS/JSON handlers;
  - [ ] list Beandown non-goals.
- [ ] Update README only if there is a short, natural place. Do not expand README into a tutorial.

### Phase-end audit / validation

- [ ] Confirm docs do not call Beandown a project type.
- [ ] Confirm docs do not imply `.bd` can be an entry/page.
- [ ] Confirm docs do not describe wildcard imports.
- [ ] Confirm docs match explicit `export` facade rules.
- [ ] Run:
  - [ ] docs checks if available;
  - [ ] `cargo run -- tests` if docs fixtures compile;
  - [ ] `just validate`.

---

## Phase 11 — Final consolidation and redundancy cleanup

### Context

Finish by removing temporary paths, duplicate extension checks, and accidental abstractions.

### Checklist

- [ ] Search for hard-coded `.bd` extension checks. Centralize them behind source-kind registry/candidate dispatch except where diagnostics need explicit display text.
- [ ] Search for Beandown-specific import resolution outside typed source-kind handling. Remove ad hoc branches.
- [ ] Search for hard-coded Beandown-visible `@html` names. There should be none.
- [ ] Search for source handler abstractions added “for future file types” but unused in v1. Remove or narrow them.
- [ ] Remove compatibility shims added during migration.
- [ ] Confirm `mod.rs` files are structural maps, not implementation dumps.
- [ ] Confirm tests live in test files/directories, not production files.
- [ ] Confirm all new public/internal APIs have concise ownership/non-goal docs.
- [ ] Confirm all new diagnostics have source locations and stable codes.
- [ ] Confirm no extra artifacts are produced for `.bd` imports.

### Phase-end audit / validation

- [ ] Run full validation:
  - [ ] `cargo fmt`
  - [ ] `cargo clippy`
  - [ ] `cargo test`
  - [ ] `cargo run -- tests`
  - [ ] `just validate`
- [ ] Manual frontend boundary review:
  - [ ] `LibrarySet` declares builder source-kind support;
  - [ ] Stage 0 owns source discovery and source-kind candidate resolution;
  - [ ] tokenizer owns entry body mode and delimiter diagnostics;
  - [ ] headers own synthetic declaration preparation;
  - [ ] dependency sorting owns Beandown constant dependency ordering;
  - [ ] AST owns folding;
  - [ ] HIR remains runtime-only and Beandown-free;
  - [ ] HTML builder owns Beandown API and support registration.

## Test matrix summary

### Positive

- [ ] `.bd` import renders markdown content in an HTML page.
- [ ] Namespace import uses `intro.content`.
- [ ] Grouped import aliases `content`.
- [ ] Facade re-export exposes `.bd` content.
- [ ] `.bd` in source library works under HTML builder.
- [ ] Empty `.bd` compiles to empty string.
- [ ] Nested templates work.
- [ ] `@html` constants are visible flat.
- [ ] Same-directory facade constants are visible.
- [ ] Facade constants override `@html` constants.
- [ ] Const-record field access works.
- [ ] Escaped `\]` and `\[` work.
- [ ] Public API handles file, directory, recursive directory, explicit file list, and in-memory sources.
- [ ] HTML JS and HTML-Wasm paths consume the same folded content.

### Negative

- [ ] `.bd` unsupported by non-HTML builder.
- [ ] Direct `import @x/file.bd` rejected.
- [ ] `.bd` used as page/entry rejected.
- [ ] `.bd` + `.bst` same stem ambiguous.
- [ ] `.bd` + folder same stem ambiguous.
- [ ] Unescaped outer `]` rejected with suggestions.
- [ ] Runtime function call in `.bd` rejected.
- [ ] Non-foldable template control flow in `.bd` rejected.
- [ ] Functions/types/methods from `@html` or facade are not visible.
- [ ] Duplicate Beandown API source paths rejected.
- [ ] Unimported `.bd` file under `entry_root` has no build effect.

## Agent constraints

- Do not implement Beandown by textual wrapper insertion.
- Do not introduce a Beandown-only template engine.
- Do not add a second import/visibility system.
- Do not make `.bd` globally valid. Builder capability controls it.
- Do not expose all `@html` symbols. Expose only folded compile-time constants/const records using the general filter.
- Do not curate a Beandown-specific HTML constant list.
- Do not make `content` reserved.
- Do not let generated `.bd` `content` self-reference from its own body.
- Do not add CLI, project config, or artifact output behavior in v1.
- Keep code readable and stage-owned: no user-input panics, no broad future-proof abstractions, no compatibility shims, and no tests in production files.
