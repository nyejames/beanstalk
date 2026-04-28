# Core and builder library system implementation plan

## Purpose

This plan defines the implementation path for Beanstalk's library system.

The goal is to separate:

- compiler-owned library metadata
- backend implementations of external libraries
- builder-provided source libraries
- project-local user libraries
- module facade/export rules

This is not a package manager plan. It is the foundation the package manager will later build on.

## Design decisions locked for this plan

### Naming

`@std` is removed.

There is no compatibility alias, no deprecation window, and no documentation of old behavior.

The new root is `@core`.

### Core prelude

Every project builder must provide `@core/prelude`.

`@core/prelude` defines the bare prelude surface available across all normal projects.

Initial prelude contents:

- `io`
- `IO`
- `Error`
- `ErrorKind`
- `ErrorLocation`
- `StackFrame`

The actual IO implementation lives in `@core/io`.

`@core/prelude` re-exports the subset that becomes bare prelude symbols.

Future IO extensions can live in `@core/io` without becoming prelude symbols.

### Core libraries

Normal core libraries are optional builder-provided libraries.

Initial core libraries:

- `@core/math`
- `@core/text`
- `@core/random`
- `@core/time`

A project builder may provide only `@core/prelude`, or it may provide any subset of the other core libraries.

If a user imports a core library the builder does not provide, the compiler should produce an "unsupported by builder" diagnostic.

### Builder libraries

HTML is not core.

HTML libraries are builder-provided libraries. They should live in a shared library location that any builder can opt into, not embedded uniquely in the docs project.

A project builder can expose builder libraries such as:

- `@html`
- `@web/dom`
- `@web/canvas`

The HTML builder should provide `@html` by default.

### Source libraries

Beanstalk source libraries are normal `.bst` files exposed through library roots.

Provided source libraries are compiled from source each build for now.

No precompiled HIR cache is part of this plan.

### `#mod.bst`

`#mod.bst` becomes the required facade file for source library modules.

A library directory is importable only through its `#mod.bst` export surface.

Example:

```text
lib/
  html/
    #mod.bst
    elements.bst
    layout.bst
```

Users import from the library facade:

```beanstalk
import @html {page, div}
```

Those symbols must be exported by:

```text
lib/html/#mod.bst
```

Direct import from implementation files outside that module is not part of the public library surface unless the facade exports it.

### `#mod.bst` visibility rule

`#mod.bst` is strictly for external module visibility.

Top-level exported declarations in `#mod.bst` become part of the module export surface, but are not visible internally to sibling files or other implementation files in that same library module.

Normal code in `#mod.bst` is private to the `#mod.bst` file.

Runtime top-level statements in `#mod.bst` are an error.

This exists to prevent `#mod.bst` from becoming a shared implementation file. Shared internal code should live in regular files inside the module and be imported normally by other files in that module.

### Export syntax

Add `export` only for re-exporting imports from `#mod.bst`.

Supported initial forms:

```beanstalk
#import @path/to/symbol
#import @path/to/symbol as exported_symbol
#import @path/to/module {thing, aliased_thing as exported_symbol}
```

Grouped re-export entries may contain nested paths:

```beanstalk
#import @core/time {date, clock/seconds}
```

`#import` does not create a local binding in the exporting file.

`#import` is restricted to `#mod.bst`.

Using `#import` elsewhere is a structured error.

Using `#import` for declaration visibility is a structured error. The diagnostic should explain that `#` exports declarations, while `#import` only re-exports imported symbols from a `#mod.bst` facade.

External package symbols may be re-exported through `#import`.

Export aliases should use the same case-convention warnings as import aliases.

### Project `/lib` behavior

Provided libraries can physically live under `/lib`, but import paths do not need to include `/lib`.

If `/lib/html` is listed as a provided library root, it can be imported as:

```beanstalk
import @html {page}
```

Builder-provided libraries are compiled through virtual roots.

When creating a new project, libraries may be copied into `/lib` so the user can inspect or vendor them.

During build/dev, builder-provided library roots should be available virtually.

Project-local libraries and builder-provided libraries with the same exposed prefix are a hard error.

## Current repo anchors

The current architecture already gives the right footholds:

- `BackendBuilder::external_packages()` exists and lets project builders provide external package metadata.
- The frontend already treats virtual package imports differently from filesystem source imports.
- AST import binding already gates external symbol visibility through file-local `visible_external_symbols`.
- The JS backend already accepts external package registry metadata during lowering.
- The progress matrix and roadmap are the places where completed/deferred surfaces must be tracked.

The implementation should extend these seams rather than adding a parallel module system.

## Non-goals

Do not implement:

- package manager
- package versions
- dependency resolution from remote registries
- source-library HIR caching
- user-authored external binding files
- dependency lockfiles
- library override/shadowing rules
- wildcard imports
- namespace imports
- Wasm implementations for `@core/text`, `@core/random`, or `@core/time`
- full datetime/date/timezone design
- seeded random design
- `@html` package manager distribution

If a syntax path is reserved or not supported, reject it with a structured diagnostic.

## Target structure

The exact file names can evolve, but the final structure should roughly become:

```text
src/
  compiler_frontend/
    external_packages/
      mod.rs
      abi.rs
      definitions.rs
      ids.rs
      registry.rs

    source_libraries/
      mod.rs
      library_roots.rs
      mod_file.rs
      export_parsing.rs
      export_resolution.rs

  libraries/
    mod.rs
    library_set.rs

    core/
      mod.rs
      prelude.rs
      io.rs
      math.rs
      text.rs
      random.rs
      time.rs

    builder/
      mod.rs
      html/
        mod.rs
        source.rs

  backends/
    js/
      libraries/
        mod.rs
        core/
          mod.rs
          math.rs
          text.rs
          random.rs
          time.rs

      runtime/
        mod.rs
        result.rs
        strings.rs
        collections.rs
        errors.rs

  projects/
    html_project/
      libraries.rs
      template_libraries/
        lib/
          html/
            #mod.bst
            elements.bst
            layout.bst
```

Key rule:

- `src/libraries/**` defines library identity and shared package metadata.
- `src/backends/<backend>/libraries/**` implements backend-specific external lowering.
- `src/projects/<builder>/libraries.rs` selects which libraries the builder exposes.
- `src/compiler_frontend/**` owns parsing, visibility, diagnostics, and source-library resolution.

## Desired API shape

Introduce a broader library surface instead of exposing only external packages.

```rust
pub struct LibrarySet {
    pub external_packages: ExternalPackageRegistry,
    pub source_libraries: SourceLibraryRegistry,
}
```

Then evolve builder APIs toward:

```rust
pub trait BackendBuilder {
    fn libraries(&self) -> LibrarySet;

    fn frontend_style_directives(&self) -> Vec<StyleDirectiveSpec>;

    fn build_backend(
        &self,
        modules: Vec<Module>,
        config: &Config,
        flags: &[Flag],
        string_table: &mut StringTable,
    ) -> Result<Project, CompilerMessages>;

    fn validate_project_config(
        &self,
        config: &Config,
        string_table: &mut StringTable,
    ) -> Result<(), CompilerError>;
}
```

During the transition, `external_packages()` may be replaced directly rather than kept as a compatibility wrapper. Beanstalk is pre-release, so prefer one current API shape.

## Phase 1 — Rename and reorganise external core packages

### Context

The first cleanup is to remove `@std` completely and establish `@core` package identity before adding source-library facades. This avoids building new library plumbing around old names.

### Implementation steps

1. Move `src/compiler_frontend/external_packages.rs` into a directory module if it is not already split:

```text
src/compiler_frontend/external_packages/
  mod.rs
  abi.rs
  definitions.rs
  ids.rs
  registry.rs
```

2. Separate package definitions from registry mechanics.

Move builtin package registration into shared library definition modules:

```text
src/libraries/core/prelude.rs
src/libraries/core/io.rs
src/libraries/core/math.rs
```

3. Rename package paths:

```text
@std/io      -> @core/io
@std/math    -> @core/math
@std/error   -> @core/prelude or compiler-owned core prelude metadata
```

4. Add `@core/prelude`.

This package should re-export or register the bare prelude surface.

The prelude registry should still populate bare names into each file's external visibility map.

5. Remove all `@std` names from:

- compiler code
- tests
- comments
- docs
- diagnostics
- progress matrix
- roadmap

Do not mention the old namespace.

6. Update integration fixtures from `@std/math` to `@core/math`.

7. Update JS lowering metadata to resolve `@core/math`.

8. Keep the current `@core/math` API set from the existing math implementation:

```text
Constants:
- PI
- TAU
- E

Functions:
- sin(Float) -> Float
- cos(Float) -> Float
- tan(Float) -> Float
- atan2(Float, Float) -> Float
- log(Float) -> Float
- log2(Float) -> Float
- log10(Float) -> Float
- exp(Float) -> Float
- pow(Float, Float) -> Float
- sqrt(Float) -> Float
- abs(Float) -> Float
- floor(Float) -> Float
- ceil(Float) -> Float
- round(Float) -> Float
- trunc(Float) -> Float
- min(Float, Float) -> Float
- max(Float, Float) -> Float
- clamp(Float, Float, Float) -> Float
```

### Tests

Add or update integration tests for:

- `import @core/math {PI, sin}`
- grouped import
- alias import
- constants in const context
- missing symbol
- non-imported symbol rejection
- old `@std/math` path fails as a missing/unsupported package, with no special compatibility diagnostic
- prelude symbols still available bare
- `@core/io` can be explicitly imported where appropriate
- prelude collision rejection still works

### Documentation updates

Update:

- `docs/language-overview.md`
- `docs/compiler-design-overview.md`
- `docs/src/docs/progress/#page.bst`
- `docs/roadmap/roadmap.md`
- any generated/docs source page that mentions external packages or math

Document only the new behavior.

### Roadmap / matrix updates

Progress matrix:

- Replace "Standard math package" with "Core math package".
- Add "Core prelude libraries" row.
- Keep "External platform packages" but update it to refer to `@core`.
- Remove `@std` from every row.

Roadmap:

- Add a linked plan entry for this library system work.
- Remove any old standalone math-library TODO if present.

### Phase-end audit commit

Commit boundary suggestion:

```text
refactor: rename std packages to core packages
```

Audit checklist:

- no `@std` references remain
- no compatibility aliases remain
- comments are grammatical and current
- external package registry files have single responsibilities
- no new user-input panics
- imports are clean and not inline-heavy
- `mod.rs` files explain module structure

Validation:

```bash
cargo clippy
cargo test
cargo run tests
```

Run `just validate` if practical.

## Phase 2 — Add `LibrarySet` and source library root plumbing

### Context

External packages are not enough. Builders must also provide Beanstalk source libraries such as `@html`. This phase adds the builder/compiler plumbing without yet changing `#mod.bst` visibility rules.

### Implementation steps

1. Add:

```rust
pub struct LibrarySet {
    pub external_packages: ExternalPackageRegistry,
    pub source_libraries: SourceLibraryRegistry,
}
```

2. Add `SourceLibraryRegistry`.

It should track exposed import prefixes and physical/embedded source roots.

Possible shape:

```rust
pub struct SourceLibraryRegistry {
    roots: Vec<SourceLibraryRoot>,
}

pub struct SourceLibraryRoot {
    pub import_prefix: LibraryImportPrefix,
    pub root: ProvidedSourceRoot,
}

pub enum ProvidedSourceRoot {
    Filesystem(PathBuf),
    Embedded {
        display_name: &'static str,
        files: Vec<EmbeddedSourceFile>,
    },
}
```

3. Replace `BackendBuilder::external_packages()` with `BackendBuilder::libraries()`.

4. Update build-system module discovery so library roots are included in import resolution.

Important behavior:

- import paths use the library prefix, not the physical `/lib` folder name
- `@html` can resolve to project `/lib/html` or builder-provided HTML library root
- local project roots and builder-provided roots cannot expose the same prefix
- collisions produce a hard structured config/build error

5. Teach reachable-file discovery to include source library files reachable through imports.

6. Keep virtual package handling distinct from source-library handling.

External package imports are still registry metadata.

Source library imports are `.bst` file resolution.

7. Ensure path rendering remains stable and readable.

Diagnostics should refer to the user-facing library prefix, not hidden embedded paths, where possible.

### Tests

Add integration tests for:

- builder-provided source library root can be imported as `@html`
- project-local library root can be imported without spelling `/lib`
- source root collision with builder-provided library is a hard error
- unresolved provided source library gives a clear missing-library diagnostic
- external `@core/math` and source `@html` imports coexist
- source libraries compile from `.bst` source each build

### Documentation updates

Update docs to describe:

- project libraries
- builder-provided libraries
- library roots
- why `/lib` does not appear in import paths when configured/provided
- distinction between source libraries and external packages

### Progress matrix updates

Add rows:

- Source library roots — Partial
- Builder-provided source libraries — Partial
- Project-local libraries — Partial

Deferred rows:

- source-library HIR caching
- package manager
- dependency versioning
- library override/shadowing

### Phase-end audit commit

Commit boundary suggestion:

```text
feat: add library set and source library roots
```

Audit checklist:

- builder API has one current shape
- no compatibility wrapper remains unless absolutely necessary
- library roots and external packages stay distinct
- file/path diagnostics use shared string table conventions
- source-root collision paths are deterministic
- no broad helper function mixes config, discovery, and import resolution

Validation:

```bash
cargo clippy
cargo test
cargo run tests
```

Run `just validate` if practical.

## Phase 3 — Implement `#mod.bst` library facades

### Context

Libraries need a deliberate public surface. A directory-based library should not leak every file or internal helper. `#mod.bst` becomes the facade that defines what external modules can see.

### Semantics

A directory containing `#mod.bst` is a module facade root.

External modules may import only symbols exported by that facade.

Inside the same library module, normal files still follow normal internal visibility/import rules. `#mod.bst` is not a shared implementation file.

Rules:

- `#mod.bst` can contain top-level exported declarations using `#`.
- Those declarations become part of the facade export surface.
- Those declarations are not visible to sibling files or other files in the same module just because they are in `#mod.bst`.
- `#mod.bst` can contain private helper declarations visible only inside `#mod.bst`.
- `#mod.bst` cannot contain runtime top-level statements.
- `#mod.bst` cannot contain top-level runtime templates.
- `#mod.bst` is the only file where `#import @...` is valid.

### Implementation steps

1. Add a header/parser concept for module facade files.

Possible naming:

```rust
FileRole::ModuleFacade
```

or:

```rust
ModuleFileKind::ModFile
```

2. During project structure / module discovery, identify `#mod.bst` roots.

3. Record each facade's exported symbol surface separately from regular file exports.

4. Change import resolution for library module paths:

```beanstalk
import @html {page}
```

should resolve through:

```text
@html/#mod.bst
```

5. Enforce that importers outside a module cannot bypass the facade.

If a user imports:

```beanstalk
import @html/elements/div
```

and `div` is not exported by `@html/#mod.bst`, produce a structured error explaining that library modules expose symbols only through their `#mod.bst` facade.

6. Decide path identity for exported symbols.

Recommended:

- canonical source declaration path remains the real declaration file
- import permission is granted through the facade export map
- diagnostics show facade path when rejecting public access

7. Ensure `#mod.bst` private code remains private.

8. Ensure `#mod.bst` exported declarations do not become implicit shared declarations inside sibling files.

9. Add targeted diagnostics for obvious mistakes:

- runtime statement in `#mod.bst`
- runtime top-level template in `#mod.bst`
- trying to import a non-exported internal library file symbol
- missing `#mod.bst` in a source library root
- using `#mod.bst` as a normal module entry page/file

### Tests

Add integration tests for:

- directory import resolves through `#mod.bst`
- grouped import from facade
- single-symbol import from facade
- direct internal implementation import rejected
- non-exported symbol in `#mod.bst` not visible externally
- `#` declaration in `#mod.bst` exported externally
- `#` declaration in `#mod.bst` not visible to sibling implementation files without normal import
- runtime top-level statement in `#mod.bst` rejected
- top-level runtime template in `#mod.bst` rejected
- source library missing `#mod.bst` rejected with clear diagnostic

### Documentation updates

Update language/project docs to clearly define:

- module roots
- source library roots
- facade files
- public exports
- internal module visibility
- why `#mod.bst` is not shared implementation code
- how to structure libraries with internal files and public exports

### Progress matrix updates

Add/update rows:

- `#mod.bst` library facades — Partial or Supported depending test coverage
- Source library visibility — Partial
- Import visibility — update watch points for facade-gated libraries

Deferred rows:

- library module wildcard exports
- namespace imports
- source-library precompilation
- facade-generated docs/API metadata if not implemented

### Phase-end audit commit

Commit boundary suggestion:

```text
feat: add mod file source library facades
```

Audit checklist:

- facade resolution does not bypass existing import collision rules
- path diagnostics are clear and stable
- no runtime code from `#mod.bst` reaches entry/start lowering
- file role checks are centralized
- import binding remains readable and stage-owned

Validation:

```bash
cargo clippy
cargo test
cargo run tests
```

Run `just validate` if practical.

## Phase 4 — Add `#import @...` re-export syntax

### Context

`#mod.bst` needs to expose symbols from internal files and external packages without forcing wrapper declarations. `#import @...` is a facade-only re-export syntax.

### Syntax

Supported forms:

```beanstalk
#import @path/to/symbol
#import @path/to/symbol as exported_symbol
#import @path/to/module {thing, aliased_thing as exported_symbol}
```

Grouped entries can contain nested paths:

```beanstalk
#import @core/time {date, clock/seconds}
```

External package re-exports are allowed:

```beanstalk
#import @core/math {sin, PI}
```

### Semantics

`#import @...`:

- is valid only in `#mod.bst`
- resolves its target like an import
- adds the resolved symbol to the facade export map
- does not create a local binding
- does not make the target visible to code in `#mod.bst`
- does not make the target visible to sibling files
- supports aliases with the same case-convention warnings as import aliases
- rejects collisions with other facade exports

### Implementation steps

1. Add `#import` re-export support in the parser. `#import` reuses the existing `import` keyword preceded by `#`.

2. Reuse import parsing where possible.

Avoid duplicating grouped path/alias parsing logic.

3. Add header representation.

Possible shape:

```rust
HeaderKind::ReExport {
    export: FileExport,
}
```

or a separate `FileReExport` collected alongside imports.

4. Restrict parsing/validation to `#mod.bst`.

If `#import` appears outside a facade file, emit a syntax/rule error:

> `#import` can only be used in `#mod.bst` to re-export imported symbols from a library facade.

5. Reject invalid `#import` forms:

```beanstalk
#import thing = 1
#import function_name |x Int| -> Int:
#import Struct = |...|
```

Diagnostics should explain:

- `#import` only accepts import-style paths
- declaration visibility uses `#`
- `#import` is facade-only

6. Resolve re-export targets after imports/source visibility is available.

7. Add facade export collision checks.

Collisions should include:

- duplicate re-export names
- re-export colliding with `#` exported declaration in `#mod.bst`
- alias collisions
- prelude/builtin collision if exported into a namespace where that matters

8. Allow re-export of external package symbols into source library facades.

Example:

```beanstalk
#import @core/math {sin as sine}
```

9. Preserve file-local import alias semantics.

A re-export alias does not create a local import alias.

### Tests

Add tests for:

- single re-export
- single re-export alias
- grouped re-export
- grouped nested-path re-export
- re-export external function
- re-export external constant
- re-export external opaque type if available
- alias case warning
- duplicate re-export rejection
- `#import` outside `#mod.bst` rejection
- `#import` with declaration syntax rejection
- `#import` does not create local binding
- exported symbol visible to importer
- non-exported imported helper remains private

### Documentation updates

Update language/project docs with:

- `#import @...` syntax
- difference between `#` and `#import`
- examples of facade re-export
- examples of aliases
- clear restrictions

### Progress matrix updates

Add row:

- Import re-exports — Supported or Partial

Update deferred row:

- namespace/wildcard imports remain Deferred

### Phase-end audit commit

Commit boundary suggestion:

```text
feat: add mod file import reexports
```

Audit checklist:

- import/re-export parsing shares code cleanly
- re-export errors are specific and not vague undefined-variable errors
- re-export does not mutate local visibility
- grouped re-export alias metadata is preserved
- no new compatibility syntax is added

Validation:

```bash
cargo clippy
cargo test
cargo run tests
```

Run `just validate` if practical.

## Phase 5 — Move HTML source library into shared builder-provided libraries

### Context

The docs-local `lib/html.bst` should become a shared HTML builder source library. The docs project should consume it like any other HTML project.

HTML remains builder-provided, not core.

### Implementation steps

1. Create shared HTML source library:

```text
src/libraries/builder/html/source/lib/html/
  #mod.bst
  elements.bst
  layout.bst
  formatting.bst
```

The exact split can be smaller for the first pass. Moving to `html/#mod.bst` is the key.

2. Move current docs `lib/html.bst` symbols into this library.

3. Make `html/#mod.bst` export the public symbols.

Example:

```beanstalk
#import @./elements {format, style}
```

or declare top-level `#` exports directly in `#mod.bst` if simpler for the first pass.

4. Update `HtmlProjectBuilder::libraries()` to include the HTML source library root.

5. Update docs imports.

Current shape:

```beanstalk
import @lib/html {format, style}
```

New shape:

```beanstalk
import @html {format, style}
```

6. Ensure project-local libraries and builder-provided `@html` collisions are hard errors.

7. Ensure docs build still compiles.

### Tests

Add integration tests for:

- HTML builder provides `@html`
- docs-style page can import `@html`
- `@html` unavailable in a builder that does not expose it
- project-local `@html` collision with builder-provided `@html` rejected
- direct internal import from `@html/elements/...` rejected if not exported by `#mod.bst`

### Documentation updates

Update:

- docs project examples
- language overview import examples if they mention `@lib/html`
- builder docs to say HTML projects provide `@html`

### Progress matrix updates

Add/update rows:

- HTML builder source library — Supported/Partial
- Builder-provided source libraries — Partial

### Phase-end audit commit

Commit boundary suggestion:

```text
feat: provide html as a builder source library
```

Audit checklist:

- no docs-only library code remains that should be shared
- public HTML helper surface is explicitly exported by `#mod.bst`
- builder ownership of `@html` is clear
- docs imports use the public library prefix
- no `@lib/html` examples remain unless showing project-local libraries specifically

Validation:

```bash
cargo clippy
cargo test
cargo run tests
cargo run --features "detailed_timers" docs
```

Run `just validate` if practical.

## Phase 6 — Add core text, random, and time skeletons

### Context

The goal is not to fully design these libraries. The goal is to create minimal tested skeletons that prove the new core-library structure scales beyond math.

### `@core/text`

Initial external JS-backed skeleton:

```text
length(text String) -> Int
is_empty(text String) -> Bool
contains(text String, pattern String) -> Bool
starts_with(text String, prefix String) -> Bool
ends_with(text String, suffix String) -> Bool
```

JS implementation:

- `length` -> `String(value).length` or strict string read equivalent
- `is_empty` -> length equals `0`
- `contains` -> `.includes`
- `starts_with` -> `.startsWith`
- `ends_with` -> `.endsWith`

Use the compiler's actual `String` / `StringSlice` ABI conventions. Do not invent a new string type in this phase.

### `@core/random`

Initial external JS-backed skeleton:

```text
random_float() -> Float
random_int(min Int, max Int) -> Int
```

JS implementation:

- `random_float` -> `Math.random()`
- `random_int(min, max)` -> integer in a clear documented range

Recommended range:

```text
min <= value <= max
```

Add a runtime guard for `min > max` only if the existing external-helper error/result machinery makes this clean. If not, document the current strict behavior and add a deferred follow-up.

Do not add seedable random in this phase.

### `@core/time`

Initial external JS-backed skeleton:

```text
now_millis() -> Int
now_seconds() -> Float
```

JS implementation:

- `now_millis` -> `Date.now()`
- `now_seconds` -> `Date.now() / 1000.0`

Do not add date objects, timezone types, formatting, durations, monotonic clocks, or calendars in this phase.

### Implementation steps

1. Add core package definitions:

```text
src/libraries/core/text.rs
src/libraries/core/random.rs
src/libraries/core/time.rs
```

2. Add JS lowering implementations:

```text
src/backends/js/libraries/core/text.rs
src/backends/js/libraries/core/random.rs
src/backends/js/libraries/core/time.rs
```

3. Register them only when the builder opts in.

For HTML builder, opt into:

- `@core/math`
- `@core/text`
- `@core/random`
- `@core/time`

This is practical for the default web builder but should not become a global compiler assumption.

4. Ensure Wasm path fails cleanly if these packages are used under Wasm mode and not implemented.

5. Keep functions positional-only like other external calls.

### Tests

Add integration tests for:

`@core/text`:

- `length("abc")`
- `is_empty("")`
- `contains("beanstalk", "stalk")`
- `starts_with("beanstalk", "bean")`
- `ends_with("beanstalk", "stalk")`
- type error for non-string arguments

`@core/random`:

- imports and calls compile
- output is numeric
- random int uses min/max arguments
- arity/type errors

Avoid deterministic golden values for randomness.

`@core/time`:

- imports and calls compile
- output is numeric
- arity/type errors

Avoid exact timestamp goldens.

Builder/backend errors:

- builder without core text rejects `@core/text`
- Wasm unsupported backend path produces clean diagnostic

### Documentation updates

Document the skeleton APIs as minimal initial surfaces.

Do not over-specify future random/time behavior.

### Progress matrix updates

Add rows:

- Core text package — Partial
- Core random package — Partial
- Core time package — Partial

Deferred rows:

- seeded random
- full datetime/date/timezone API
- monotonic clock API
- Wasm implementations for core text/random/time if not implemented
- user-authored external binding files

### Phase-end audit commit

Commit boundary suggestion:

```text
feat: add core text random and time skeleton libraries
```

Audit checklist:

- skeleton APIs are intentionally small
- random/time tests do not depend on exact values
- JS helpers live under backend library implementation, not generic runtime dumping grounds
- builder opt-in is explicit
- unsupported builder/backend diagnostics are clear

Validation:

```bash
cargo clippy
cargo test
cargo run tests
```

Run `just validate` if practical.

## Phase 7 — Documentation and implementation matrix sweep

### Context

The final phase makes the new system understandable. The docs should describe the new behavior directly, not as a migration from the old behavior.

### Documentation targets

Update at least:

```text
docs/language-overview.md
docs/compiler-design-overview.md
docs/src/docs/progress/#page.bst
docs/roadmap/roadmap.md
README.md if examples mention library imports
docs/src docs pages that describe imports/project structure
```

### Required documentation content

Document:

1. Library categories

- core prelude libraries
- core libraries
- builder libraries
- project libraries
- source libraries
- external packages

2. Import roots

Explain that configured/provided library roots expose prefixes directly.

Example:

```text
/lib/html -> @html
```

3. Core prelude

Explain:

- every builder must provide `@core/prelude`
- prelude exports become bare names
- `io`, `IO`, and builtin error types are part of this prelude surface
- implementation may live in more specific core modules such as `@core/io`

4. Core libraries

Explain:

- `@core/math`, `@core/text`, `@core/random`, `@core/time`
- optional by builder
- explicit import required
- unsupported-by-builder diagnostic if absent

5. Builder libraries

Explain:

- `@html` belongs to HTML builder
- other builders may provide their own libraries
- builder libraries are not core

6. Source libraries and `#mod.bst`

Document clearly:

- every source library module exposes public surface through `#mod.bst`
- directory imports resolve through `#mod.bst`
- internal files are not public unless exported by `#mod.bst`
- `#mod.bst` is not a shared implementation file
- runtime top-level code in `#mod.bst` is invalid

7. Re-export syntax

Document:

```beanstalk
#import @path/to/symbol
#import @path/to/symbol as exported_symbol
#import @path/to/module {thing, aliased_thing as exported_symbol}
```

Explain:

- only valid in `#mod.bst`
- only re-exports import-style paths
- does not create local bindings
- use `#` for declaration exports

8. Deferred features

Document explicitly as deferred:

- package manager
- package versions
- remote dependency fetching
- source-library HIR caching
- user-authored external binding files
- dependency lockfiles
- library override/shadowing
- wildcard imports
- namespace imports
- seeded random
- full date/time/timezone API
- Wasm implementations for unsupported core packages
- automatic docs/API extraction from `#mod.bst`

### Progress matrix row plan

Split the matrix into clear rows rather than one overloaded external-package row.

Recommended rows:

#### Core prelude libraries

Status: Supported once implemented.

Coverage: Targeted/Broad depending tests.

Watch points:

- every builder must provide `@core/prelude`
- bare prelude symbols are populated from prelude exports
- no old namespace references

#### Core external libraries

Status: Partial.

Coverage: Targeted.

Watch points:

- `@core/math` has broadest support
- text/random/time are skeletons
- builders opt in

#### Builder-provided source libraries

Status: Partial.

Coverage: Targeted.

Watch points:

- HTML builder provides `@html`
- other builders may provide their own libraries
- prefix collision is hard error

#### Project-local source libraries

Status: Partial.

Coverage: Targeted.

Watch points:

- `/lib` path does not appear in imports when exposed as a library root
- collision with builder roots is hard error

#### `#mod.bst` facades

Status: Partial/Supported.

Coverage: Targeted.

Watch points:

- only facade exports are visible externally
- no runtime top-level code
- `#mod.bst` is not internal shared code

#### Import re-exports

Status: Supported if phase 4 test coverage is complete.

Coverage: Targeted.

Watch points:

- facade-only
- import-style paths only
- no local binding creation
- aliases warn consistently

#### External platform packages

Status: Partial.

Coverage: Broad/Targeted.

Watch points:

- registry metadata remains Rust-side
- user-authored binding files deferred
- backend support must be explicit

### Roadmap updates

Add linked roadmap entry:

```markdown
- [Core and builder library system](plans/core-and-builder-library-system.md)
```

Add deferred list items or plan notes for:

- package manager
- library versioning
- source-library HIR caching
- user-authored external binding files
- seeded random
- full datetime/timezone API
- Wasm support for non-math core packages

Remove old math-library plan references.

Do not mention old namespace migrations.

### Phase-end audit commit

Commit boundary suggestion:

```text
docs: document core and builder library system
```

Audit checklist:

- docs describe new behavior directly
- no old namespace remains
- language overview and compiler overview agree
- progress matrix separates current support from deferred features
- roadmap links the full plan
- examples use `@core` and `@html`

Validation:

```bash
cargo clippy
cargo test
cargo run tests
cargo run --features "detailed_timers" docs
```

Run `just validate` if practical.

## Phase 8 — Final whole-system audit

### Context

After the functional phases, do a final cleanup pass. This should be a separate commit so implementation changes and style cleanup are easy to review.

### Audit targets

Review:

- external package registry split
- library set plumbing
- source library root resolution
- `#mod.bst` facade handling
- export parsing/resolution
- JS library implementation files
- HTML library migration
- tests and fixtures
- docs/progress/roadmap

### Specific checks

1. Clone/churn audit

Look for avoidable clones around:

- `ExternalPackageRegistry`
- `LibrarySet`
- `SourceLibraryRegistry`
- visible import/export maps
- backend compile input
- JS emitter config
- package definitions copied into lookup maps

Keep clones where they preserve readability or avoid lifetime complexity.

2. Module responsibility audit

Check that:

- registry mechanics do not contain package definitions
- package definitions do not contain backend JS bodies
- JS runtime helpers are not mixed with library implementations
- project builder library selection is not mixed into backend lowering

3. Diagnostic audit

Check that errors are clear for:

- unsupported builder library
- unsupported backend library lowering
- missing `#mod.bst`
- direct import of internal library file
- `export` outside `#mod.bst`
- `export` used for declarations
- runtime top-level code in `#mod.bst`
- library prefix collisions
- old removed namespace imports

4. Style guide audit

Check:

- `mod.rs` files are structural maps
- no large mixed-responsibility files
- comments explain WHAT/WHY
- no stale comments
- no user-input panics
- no compatibility wrappers
- imports are clean
- tests are not duplicated unnecessarily

5. Test audit

Check for:

- success and failure coverage per new feature
- no randomness exact-golden tests
- no timestamp exact-golden tests
- builder/backend unsupported diagnostics
- docs-style HTML build coverage

### Final validation

Run:

```bash
just validate
```

If not possible, run and record:

```bash
cargo clippy
cargo test
cargo run tests
cargo run --features "detailed_timers" docs
cargo run --release --features "detailed_timers" speed-test.bst
cargo fmt --check
```

### Phase-end audit commit

Commit boundary suggestion:

```text
chore: audit library system organisation
```

## Implementation risk notes

### Risk: library modules complicate import resolution

The biggest risk is bolting facade visibility onto the existing import system without clearly separating:

- source file resolution
- source symbol visibility
- external package visibility
- facade export permission

Mitigation:

- add explicit data structures for facade export maps
- keep canonical declaration identity separate from public import permission
- do not hide facade logic inside string path matching

### Risk: `#mod.bst` becomes a special-case entry file

Do not treat `#mod.bst` as a runtime entry file.

It should be closer to a facade/header file with optional private compile-time declarations.

Runtime top-level statements are invalid.

### Risk: `#import` accidentally behaves like `import`

`#import @...` must not create local bindings.

Mitigation:

- separate `FileImport` and `FileReExport` in data model even if parsing is shared
- test that exported names are not visible inside `#mod.bst`

### Risk: core libraries become globally assumed

Only `@core/prelude` is mandatory.

Everything else is builder-provided.

Mitigation:

- builder library set must explicitly include each optional core package
- add unsupported-builder tests

### Risk: backend implementation leaks into registry metadata

Package definitions should own signatures and stable IDs.

Backend files should own lowering bodies/host mappings.

Mitigation:

- use backend-neutral lowering keys where practical
- keep JS helper bodies out of shared package definitions

## Suggested commit sequence

1. `refactor: rename std packages to core packages`
2. `feat: add library set and source library roots`
3. `feat: add mod file source library facades`
4. `feat: add mod file import reexports`
5. `feat: provide html as a builder source library`
6. `feat: add core text random and time skeleton libraries`
7. `docs: document core and builder library system`
8. `chore: audit library system organisation`

Each phase should include its own audit/style/validation checklist before committing.

## Final acceptance criteria

This plan is complete when:

- no `@std` references remain
- every builder provides `@core/prelude`
- `@core/io` contains IO implementation metadata and `@core/prelude` exposes prelude IO
- `@core/math` replaces the existing math package with the current API set
- `@core/text`, `@core/random`, and `@core/time` exist as minimal skeleton libraries
- optional core libraries produce unsupported-by-builder diagnostics when absent
- JS backend implements the initial core libraries using native JS APIs
- unsupported Wasm library lowering fails cleanly
- source library roots exist
- builder-provided `@html` source library exists
- `/lib/html` can be exposed as `@html` without spelling `/lib` in imports
- source library prefix collisions are hard errors
- `#mod.bst` is enforced as the library facade surface
- `#import @...` works only in `#mod.bst`
- re-export syntax supports single, aliased, grouped, and nested grouped re-exports
- re-export aliases warn like import aliases
- `#import` does not create local bindings
- docs explain libraries, projects, modules, facades, and visibility boundaries
- roadmap links this plan
- progress matrix separates implemented, partial, experimental, and deferred surfaces
- all validation commands pass or skipped commands are recorded with reasons
