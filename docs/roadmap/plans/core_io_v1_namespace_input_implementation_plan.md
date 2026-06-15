# Core IO V1 and Nested Namespace Implementation Plan

## Goal

Move Beanstalk to a single lowercase core IO namespace and use that work to land the first practical IO expansion:

```beanstalk
io.line("hello")
io.print("loading...")

input ~= io.input.new()!
io.input.update(~input)

if io.input.key_pressed(input, "d"):
    io.line("pressed d")
;
```

This replaces callable `io(...)` with `io.line(...)`, removes public `IO`, implements nested namespace traversal needed by `io.input.*`, and adds HTML-JS keyboard/pointer polling without adding callbacks, promises, channels, or a frame/tick API.

The phases are sized for coding agents to complete one phase at a time. Each phase ends with an audit, style-guide review, and validation gate.

---

## Design contract

### Final V1 surface

| Area | Public shape | Notes |
|---|---|---|
| Prelude namespace | `io` | Lowercase namespace alias to `@core/io`. |
| Explicit import | `import @core/io` | Binds the namespace as `io`. |
| Explicit alias | `import @core/io as output` | `output.line(...)` works through ordinary namespace import logic. |
| Console output | `io.print`, `io.line`, `io.debug`, `io.warn`, `io.error` | Infallible. String-compatible input only. |
| Input handle | `io.input.Input` | Opaque external handle type. |
| Input creation | `io.input.new() -> io.input.Input, Error!` | Fallible because backend/runtime support may fail. |
| Input state update | `io.input.update(~input)` | Infallible after creation. Requires mutable access. |
| Input cleanup | `io.input.close(~input)` | Infallible. Requires mutable access. |
| Polling reads | `key_down`, `key_pressed`, `pointer_x`, etc. | Infallible. Shared input access. |

### Console functions

```beanstalk
io.print("text")
io.line("text")
io.debug("text")
io.warn("text")
io.error("text")
```

- Console output accepts escaped string slices and owned/template strings.
- Console output rejects non-string values such as `Int`, `Float`, `Bool`, structs, choices, collections, and options.
- Use template interpolation for debugging values:

```beanstalk
io.line([: count = [count]])
```

- HTML-JS maps V1 console output to the browser console.
- `io.print` and `io.line` keep separate terminal-backend intent even if HTML-JS initially maps both to console output.

### Input functions

```beanstalk
io.input.new() -> io.input.Input, Error!
io.input.update(input ~io.input.Input)
io.input.close(input ~io.input.Input)

io.input.key_down(input io.input.Input, key String) -> Bool
io.input.key_pressed(input io.input.Input, key String) -> Bool
io.input.key_released(input io.input.Input, key String) -> Bool

io.input.pointer_x(input io.input.Input) -> Float
io.input.pointer_y(input io.input.Input) -> Float
io.input.pointer_down(input io.input.Input, button String) -> Bool
io.input.pointer_pressed(input io.input.Input, button String) -> Bool
io.input.pointer_released(input io.input.Input, button String) -> Bool

io.input.last_key_pressed(input io.input.Input) -> String?
io.input.last_key_released(input io.input.Input) -> String?
io.input.last_pointer_pressed(input io.input.Input) -> String?
io.input.last_pointer_released(input io.input.Input) -> String?
```

### Input behavior

- V1 supports keyboard and pointer/mouse polling only.
- `io.input.new()` creates the backend-default input source.
- For HTML-JS, the backend-default source is window/document-level input.
- No target/canvas/element argument exists in V1.
- Key strings use normalized logical keys:
  - single alphabetic keys normalize to lowercase;
  - space normalizes to `"Space"`;
  - special keys use names such as `"ArrowLeft"`, `"Enter"`, `"Escape"`, `"Backspace"`, `"Shift"`;
  - exact text entry is deferred.
- Pointer button strings are `"left"`, `"middle"`, and `"right"`.
- Unknown key/button strings return `false`.
- `key_down` / `pointer_down` mean currently held.
- `key_pressed` / `key_released` / `pointer_pressed` / `pointer_released` mean edge state from the latest `io.input.update(~input)`.
- Key auto-repeat does not retrigger `key_pressed` while the key is already held.
- `last_*` helpers return the most recent matching edge string from the latest update, or `none`.
- Pointer coordinates are `Float`, use backend logical coordinates, default to `0.0`, and reset to `0.0` on close.
- HTML-JS uses Pointer Events internally.
- HTML-JS does not call `preventDefault()` by default.
- HTML-JS clears held key/button state on blur, visibility loss, and pointer cancel.
- No frame/tick API is added in V1.

### Deliberately deferred

Document these in the roadmap and progress matrix:

- filesystem/path IO;
- fetch/network IO until async/channels exist;
- timers, sleep, intervals, frame/tick, and animation-frame APIs;
- targeted input sources such as canvas, DOM elements, native windows, or backend-specific surfaces;
- physical key-code APIs;
- typed `Button` / `KeyCode` choices;
- text entry and IME/composition events;
- touch gestures;
- gamepads;
- drag/drop;
- clipboard;
- wheel scrolling;
- file picker;
- full ordered event queues;
- pressed/released key/button collections;
- canvas-local, element-relative, DPI-scaled, and world/screen coordinates;
- configurable event capture/default suppression;
- Wasm/native lowerings unless added in a later backend-specific phase.

---

## Current repo anchor

Verify this again at the start of Phase 0. As of the planning pass, the relevant current shape is:

| Area | File | Current shape to replace or extend |
|---|---|---|
| External package docs | `src/compiler_frontend/external_packages/mod.rs` | Mentions prelude bare-name exception as `io`, `IO`. |
| External IDs | `src/compiler_frontend/external_packages/ids.rs` | `IO_FUNC_NAME = "io"`, `IO_TYPE_NAME = "IO"`, `ExternalFunctionId::Io`. |
| External ABI | `src/compiler_frontend/external_packages/abi.rs` | Scalars, handles, inferred values, external types. No optional signature type. |
| External definitions | `src/compiler_frontend/external_packages/definitions.rs` | `ExternalPackage` stores flat cloned definition maps by symbol name. |
| Registry | `src/compiler_frontend/external_packages/registry.rs` | Flat package-scoped symbol lookup plus prelude symbol map. |
| Core IO package | `src/libraries/core/io.rs` | Registers flat callable `io`, flat type `IO`, and `__bs_io`. |
| Prelude | `src/libraries/core/prelude.rs` | Registers `io` as prelude function and `IO` as prelude type. |
| Import visibility | `src/compiler_frontend/headers/import_environment/bindings.rs` | `NamespaceRecord` has value/type members only. |
| Namespace construction | `src/compiler_frontend/headers/import_environment/namespace_imports.rs` | Builds shallow source/facade/external namespace records. |
| Expression namespace access | `src/compiler_frontend/ast/expressions/namespace_member_access.rs` | Parses `namespace.member` and rejects nested traversal. |
| Identifier dispatch | `src/compiler_frontend/ast/expressions/parse_expression_identifiers.rs` | Detects visible namespace records and delegates to shallow member access. |
| Type refs | `src/compiler_frontend/datatypes/parsed.rs` | `ParsedTypeRef::Namespaced { namespace, name }` supports two segments. |
| Backend validation | `src/backends/external_package_validation.rs` | Reachable external calls need target lowering metadata; Wasm special-cases old `Io`. |
| Progress matrix | `docs/src/docs/progress/#page.bst` | Paths/imports row describes shallow namespace records; IO/string row describes old IO boundary. |
| Roadmap | `docs/roadmap/roadmap.md` | Has TODOs for nested namespace traversal and core IO expansion. |

---

## Implementation principles

- Keep namespaces compile-time only. Do not introduce runtime namespace objects.
- Resolve `io.input.key_pressed` to a stable `ExternalFunctionId` before HIR.
- HIR/backend lowering must not depend on source namespace spelling.
- Header import preparation owns namespace visibility. AST consumes it through `ScopeContext`.
- Remove obsolete shallow namespace and old `io(...)` paths instead of layering wrappers around them.
- Reuse existing source/external call validation and lowering wherever possible.
- Add reusable string-content and optional external signature support instead of keeping IO-specific type hacks.
- Prefer compact IO spec tables/helpers over repeated `ExternalFunctionDef` literals.
- Keep `@core/io` backend-agnostic at the language surface. Backend support is expressed through lowerings and backend validation.
- Unsupported reachable IO calls are compile-time backend validation errors.
- Runtime `Error!` is only for failures on supported backends.
- Do not expose JS DOM events, JS event objects, callbacks, promises, or listener handles through Beanstalk V1.

---

## Complexity-reduction targets

These are part of the implementation, not optional cleanup.

- [ ] Replace flat external package member maps with path-aware package surface indexes.
  - Prefer package maps that store symbol path → stable ID.
  - Keep full definitions in the existing ID-indexed maps.
  - Avoid storing cloned function/type/constant definitions both in package maps and global ID maps after the path refactor.
- [ ] Replace shallow `namespace_member_access.rs` with one traversal-oriented namespace access module.
  - Delete the old nested-traversal rejection path.
  - Reuse existing source-call and external-call parsers for leaf dispatch.
  - Consolidate `external_namespace_members.rs` into the new namespace access module if it becomes a thin forwarding layer.
- [ ] Move old IO string-boundary behavior into a reusable external signature concept.
  - Do not keep a special case for the old `ExternalFunctionId::Io`.
  - Console functions should use the same reusable string-content validation path as future string-boundary host APIs.
- [ ] Register IO functions from a small spec table.
  - Avoid repeated long metadata literals.
  - Use named helper constructors for common parameter/return shapes.
- [ ] Share JS runtime helper internals.
  - Console helpers can be tiny wrappers around one internal console-write helper.
  - Input helpers should share key/button normalization and neutral-state helpers.

---

## Phase 0 — Baseline audit and plan landing

### Context

Land the plan, verify the current repo shape, and build a callsite inventory before touching compiler behavior.

### Checklist

- [ ] Add this plan to `docs/roadmap/plans/core_io_v1_namespace_input_plan.md`.
- [ ] Update `docs/roadmap/roadmap.md`:
  - [ ] link this plan from the active TODO list;
  - [ ] keep nested namespace traversal and core IO expansion tied to this plan;
  - [ ] add the deferred IO follow-up list from this document.
- [ ] Run repo searches and save the results in the implementation notes or PR description:
  - [ ] `rg "\bio\s*\("`
  - [ ] `rg "\bIO\b"`
  - [ ] `rg "@core/io"`
  - [ ] `rg "Standard Output|String coercion and IO|io\)" docs README.md tests src`
  - [ ] `rg "nested_traversal|nested traversal|shallow" src docs tests`
  - [ ] `rg "ExternalFunctionId::Io|IO_FUNC_NAME|IO_TYPE_NAME" src`
- [ ] Confirm whether the expression parser still uses `ExpressionRpnItem`; keep new namespace code consistent with the current expression representation.
- [ ] Confirm the JS backend helper-emission location before adding runtime helpers.

### Phase 0 gate

- [ ] No production compiler behavior changed.
- [ ] `cargo fmt` if any Rust files changed.
- [ ] Run docs/static-site check if available.
- [ ] Run `just validate` if docs changes are included in the repo’s normal validation path.
- [ ] Manual review: the plan does not add compatibility shims or transitional public APIs.

---

## Phase 1 — Nested namespace infrastructure

### Context

The final IO API requires nested value and type traversal: `io.input.new()` and `io.input.Input`. Implement the generic import/namespace machinery first, while leaving old `io(...)` behavior in place until Phase 2.

### 1.1 External package symbol paths

#### Checklist

- [ ] Add an `ExternalSymbolPath` type.
  - [ ] Store path components as owned strings.
  - [ ] Provide helpers for one-component paths, display text, and leaf name.
  - [ ] Do not use a dot-joined string as the canonical representation.
- [ ] Refactor package surface storage.
  - [ ] Store function path → `ExternalFunctionId` on the package surface.
  - [ ] Store type path → `ExternalTypeId` on the package surface.
  - [ ] Store constant path → `ExternalConstantId` on the package surface.
  - [ ] Keep full definitions in the existing `functions_by_id`, `types_by_id`, and `constants_by_id` maps.
  - [ ] Remove cloned definition maps from `ExternalPackage` once callers use path → ID maps.
- [ ] Replace flat package-symbol keys with path-aware keys.
- [ ] Add registration APIs:
  - [ ] `register_function_at_path(package_id, path, id, definition)`;
  - [ ] `register_type_at_path(package_id, path, id, definition)`;
  - [ ] `register_constant_at_path(package_id, path, id, definition)`;
  - [ ] dynamic/spec variants for builder-created symbols.
- [ ] Keep one-component registration helpers as convenience wrappers only.
- [ ] Update existing core/builder/test package registrations to use the new APIs or wrappers.
- [ ] Update external import provider package registration code.
- [ ] Update package lookup helpers:
  - [ ] resolve function/type/constant by path;
  - [ ] resolve any symbol by path;
  - [ ] iterate package symbol paths for namespace construction.

#### Tests

- [ ] Duplicate function path in one package is rejected.
- [ ] Same leaf under different child namespaces is allowed.
- [ ] Function/type/constant collisions at the same namespace slot are rejected where they would create ambiguous namespace records.
- [ ] Existing one-component external imports still resolve.
- [ ] Virtual package prefix matching is unchanged.

### 1.2 Recursive namespace records

#### Checklist

- [ ] Extend `NamespaceRecord` with child namespace members.
- [ ] Add a small traversal/path helper type for diagnostics and lookup.
- [ ] Update duplicate-member validation:
  - [ ] value + type same name rejects;
  - [ ] namespace + value same name rejects;
  - [ ] namespace + type same name rejects;
  - [ ] same leaf under different namespaces is allowed.
- [ ] Build external package namespace records from path-aware package surfaces.
- [ ] Keep source and facade namespace records shallow unless recursive source namespace exports are implemented in a later plan.
- [ ] Preserve receiver-method behavior: receiver methods are not namespace members.

#### Tests

- [ ] External nested namespace tree builds correctly.
- [ ] Duplicate namespace/value/type slots produce structured diagnostics.
- [ ] Source receiver methods remain absent from namespace records.

### 1.3 Value-position traversal

#### Checklist

- [ ] Replace shallow namespace parsing with `ast/expressions/namespace_access/`.
- [ ] Suggested structure:
  - [ ] `mod.rs` for orchestration and public parser entry;
  - [ ] `traversal.rs` for record walking;
  - [ ] `leaf_resolution.rs` for source/external terminal dispatch;
  - [ ] `diagnostics.rs` if diagnostic construction needs local helpers.
- [ ] Walk `root.member.member...` while each intermediate member is a namespace.
- [ ] Terminal source function/value handling reuses existing source callable/reference code.
- [ ] Terminal external function/constant handling reuses existing external call/constant code.
- [ ] Terminal type member in value position produces type/value misuse.
- [ ] Namespace used as value remains invalid.
- [ ] Value used as namespace produces a structured diagnostic.
- [ ] Ordinary field/member access after a real expression remains owned by the normal postfix/field-access path.
- [ ] Delete or shrink old `namespace_member_access.rs` after the new parser path is wired.

#### Tests

- [ ] Nested external function call succeeds.
- [ ] Nested external constant read succeeds.
- [ ] Namespace used as value fails.
- [ ] Missing nested member fails.
- [ ] Type used in value position fails.
- [ ] Value/function used as namespace fails.

### 1.4 Type-position traversal

#### Checklist

- [ ] Replace or extend `ParsedTypeRef::Namespaced { namespace, name }` with a qualified path shape.
  - Recommended:
    ```rust
    ParsedTypeRef::Qualified {
        path: Vec<StringId>,
        location: SourceLocation,
    }
    ```
- [ ] Update type parsing to collect arbitrary dotted type paths.
- [ ] Preserve bare named type parsing.
- [ ] Update `remap_string_ids`.
- [ ] Replace two-segment namespaced type lookup with qualified namespace lookup.
- [ ] Walk namespace children until the final segment.
- [ ] Final segment must be a type member.
- [ ] Value/function member in type position produces namespace type/value misuse.
- [ ] Missing final segment produces unknown type diagnostics.
- [ ] If generic application on qualified bases becomes large, reject it deliberately and add a roadmap follow-up.

#### Tests

- [ ] `io.input.Input`-style nested external opaque type resolves in a parameter.
- [ ] Nested function used as type fails.
- [ ] Missing nested type fails.
- [ ] Namespace used as type fails.

### Phase 1 gate

- [ ] Run focused registry/import/type-resolution tests.
- [ ] Run namespace integration cases.
- [ ] Run `cargo fmt`.
- [ ] Run `cargo clippy`.
- [ ] Run `just validate`.
- [ ] Manual stage-boundary audit:
  - [ ] header/import preparation owns namespace records;
  - [ ] AST consumes `ScopeContext` visibility;
  - [ ] HIR contains stable paths/IDs only;
  - [ ] no runtime namespace value exists;
  - [ ] receiver methods are still receiver-call-only.
- [ ] Style review:
  - [ ] new files have file-level docs;
  - [ ] helpers use named input structs where needed;
  - [ ] diagnostics use `CompilerDiagnostic`;
  - [ ] no user-input panics or `todo!` paths.

---

## Phase 2 — Convert `@core/io` to lowercase namespace + console API

### Context

Make the public API break in one slice. Remove callable `io(...)`, remove public `IO`, and expose console output through `io.*`.

### 2.1 External IDs and package constants

#### Checklist

- [ ] Replace old IO constants:
  - [ ] remove or stop using `IO_FUNC_NAME`;
  - [ ] remove or stop using `IO_TYPE_NAME`;
  - [ ] add `CORE_IO_PACKAGE_PATH: &str = "@core/io"`;
  - [ ] add `IO_NAMESPACE_NAME: &str = "io"`.
- [ ] Replace `ExternalFunctionId::Io` with:
  - [ ] `IoPrint`;
  - [ ] `IoLine`;
  - [ ] `IoDebug`;
  - [ ] `IoWarn`;
  - [ ] `IoError`.
- [ ] Update `ExternalFunctionId::name()`.
- [ ] Remove the Wasm validation special-case for old `ExternalFunctionId::Io`.
- [ ] Let all old `ExternalFunctionId::Io` uses fail compilation until updated.

### 2.2 Reusable string-content external parameter

#### Checklist

- [ ] Audit the current old `io(...)` string-boundary validation.
- [ ] Extract reusable string-content validation if it is currently special-cased.
- [ ] Add a clear external signature shape if needed, for example:
  ```rust
  ExternalSignatureType::StringContent
  ```
  or an equivalent ABI/parameter policy.
- [ ] Ensure the signature accepts string slices and owned/template strings.
- [ ] Ensure it rejects non-string values with existing string-boundary diagnostics where possible.
- [ ] Use this for all console functions.
- [ ] Delete old `io(...)`-specific string validation after console functions are wired.

### 2.3 Prelude namespace alias

#### Checklist

- [ ] Add registry support for prelude namespace aliases.
  - Preferred shape:
    ```rust
    enum PreludeBinding {
        ExternalSymbol(ExternalSymbolId),
        NamespacePackage { package_path: &'static str },
    }
    ```
    or equivalent separate maps if that is clearer.
- [ ] Add `register_prelude_namespace(public_name, package_path)`.
- [ ] Update import-environment builder:
  - [ ] reserve prelude namespace names for collision detection;
  - [ ] inject unshadowed prelude namespace records into `visible_namespace_records`;
  - [ ] use the same namespace-record construction path as explicit imports.
- [ ] Update `register_core_prelude`:
  - [ ] register `io` as namespace alias to `@core/io`;
  - [ ] remove prelude function `io`;
  - [ ] remove prelude type `IO`.

### 2.4 Core IO package registration

#### Checklist

- [ ] Convert `src/libraries/core/io.rs` to a directory if useful:
  ```text
  src/libraries/core/io/
  ├── mod.rs
  ├── functions.rs
  ├── signatures.rs
  └── ids.rs          # only if local constants/spec helpers need separation
  ```
- [ ] Register `@core/io` as a builtin package.
- [ ] Register console functions at paths:
  - [ ] `print`;
  - [ ] `line`;
  - [ ] `debug`;
  - [ ] `warn`;
  - [ ] `error`.
- [ ] Use a compact `IoFunctionSpec` table and helper registration function.
- [ ] Console functions return `Void` and have no `Error!` slot.
- [ ] Remove `IO` external type from `@core/io`.

### 2.5 JS console lowerings

#### Checklist

- [ ] Add JS runtime functions:
  - [ ] `__bs_io_print`;
  - [ ] `__bs_io_line`;
  - [ ] `__bs_io_debug`;
  - [ ] `__bs_io_warn`;
  - [ ] `__bs_io_error`.
- [ ] Share implementation through one internal console helper where practical.
- [ ] Map HTML-JS output to browser console.
- [ ] Keep helper emission demand-driven/reachability-driven.
- [ ] Do not add promises, async, or channels.

### 2.6 Hard removal of callable `io(...)`

#### Checklist

- [ ] Remove old `io` external function registration.
- [ ] Remove old prelude function registration.
- [ ] Do not add a compatibility wrapper.
- [ ] Do not add a special migration diagnostic.
- [ ] Let normal namespace/call diagnostics handle `io(...)`.
- [ ] Replace all valid source uses:
  - [ ] `io("...")` -> `io.line("...")`;
  - [ ] `io([: ...])` -> `io.line([: ...])`.
- [ ] Update README, docs, docs-site examples, tests, fixtures, and goldens.

#### Tests

- [ ] `io.line("hello")` succeeds.
- [ ] `io.print("hello")` succeeds.
- [ ] `io.debug("hello")` succeeds.
- [ ] `io.warn("hello")` succeeds.
- [ ] `io.error("hello")` succeeds.
- [ ] `import @core/io` then `io.line("hello")` succeeds.
- [ ] `import @core/io as output` then `output.line("hello")` succeeds.
- [ ] `io.line([: hello])` succeeds.
- [ ] `io.line(1)` fails.
- [ ] `io("hello")` fails without a compatibility path.
- [ ] `IO` is not visible.

### Phase 2 gate

- [ ] `rg "\bio\s*\("` has no stale user-facing old API matches.
- [ ] `rg "\bIO\b"` has no stale public IO matches.
- [ ] Console integration tests pass.
- [ ] JS artifact/golden tests pass.
- [ ] Run `cargo fmt`.
- [ ] Run `cargo clippy`.
- [ ] Run `just validate`.
- [ ] Manual audit:
  - [ ] `io` is a namespace record, not a function;
  - [ ] old callable `io` is not visible;
  - [ ] HIR console calls use new external IDs;
  - [ ] old IO special cases are deleted or generalized.

---

## Phase 3 — External optional returns and IO input metadata

### Context

The agreed debug helpers return `String?`, and `io.input.Input` is a nested opaque external type. Add the minimum reusable signature support and register the full input surface.

### 3.1 Optional external return support

#### Checklist

- [ ] Add optional external signature support.
  - Preferred general shape:
    ```rust
    ExternalSignatureType::Optional(Box<ExternalSignatureType>)
    ```
  - A narrower V1 shape is acceptable only if it still uses the canonical option representation.
- [ ] Update `to_datatype()`.
- [ ] Update `to_type_id()`.
- [ ] Update parameter conversion only if optional parameters are needed.
  - V1 does not need optional external parameters.
- [ ] Update external call validation and result typing.
- [ ] Update HIR external call result typing if it assumes only scalar/external return IDs.
- [ ] Update JS lowering to materialize the existing Beanstalk optional representation.
- [ ] Do not introduce an IO-only `null`/empty-string convention.
- [ ] Add a test package function returning `String?`.

#### Tests

- [ ] External `String?` return can be captured and pattern-checked with `if maybe is |value|`.
- [ ] External absent optional return behaves as `none`.
- [ ] Optional external return uses the same type identity as ordinary `String?`.

### 3.2 Input IDs and type

#### Checklist

- [ ] Add stable function IDs:
  - [ ] `IoInputNew`;
  - [ ] `IoInputUpdate`;
  - [ ] `IoInputClose`;
  - [ ] `IoInputKeyDown`;
  - [ ] `IoInputKeyPressed`;
  - [ ] `IoInputKeyReleased`;
  - [ ] `IoInputPointerX`;
  - [ ] `IoInputPointerY`;
  - [ ] `IoInputPointerDown`;
  - [ ] `IoInputPointerPressed`;
  - [ ] `IoInputPointerReleased`;
  - [ ] `IoInputLastKeyPressed`;
  - [ ] `IoInputLastKeyReleased`;
  - [ ] `IoInputLastPointerPressed`;
  - [ ] `IoInputLastPointerReleased`.
- [ ] Add a named `ExternalTypeId` constant for `io.input.Input`.
  - Do not scatter raw numeric `ExternalTypeId(...)` values.
- [ ] Register type path `input.Input`.
- [ ] Register function paths under `input.*`.

### 3.3 Input signatures

#### Checklist

- [ ] `io.input.new() -> io.input.Input, Error!`
  - [ ] no parameters;
  - [ ] success return `External(INPUT_TYPE_ID)`;
  - [ ] error return `BuiltinError`;
  - [ ] JS lowering `__bs_io_input_new`.
- [ ] `io.input.update(~input)`
  - [ ] mutable `Input` parameter;
  - [ ] return `Void`;
  - [ ] JS lowering `__bs_io_input_update`.
- [ ] `io.input.close(~input)`
  - [ ] mutable `Input` parameter;
  - [ ] return `Void`;
  - [ ] JS lowering `__bs_io_input_close`.
- [ ] Key reads:
  - [ ] shared `Input`;
  - [ ] string-content or strict string parameter for `key`;
  - [ ] return `Bool`.
- [ ] Pointer button reads:
  - [ ] shared `Input`;
  - [ ] string parameter for `button`;
  - [ ] return `Bool`.
- [ ] Pointer coordinate reads:
  - [ ] shared `Input`;
  - [ ] return `Float`.
- [ ] Last-edge helpers:
  - [ ] shared `Input`;
  - [ ] return `String?`.

#### Tests

- [ ] `input ~= io.input.new()!` succeeds.
- [ ] `io.input.update(~input)` succeeds.
- [ ] `io.input.close(~input)` succeeds.
- [ ] Key and pointer reads type-check in `if` conditions.
- [ ] `io.input.Input` works in function signatures.
- [ ] `last_key_pressed` optional capture works.
- [ ] Missing `~` on `update` and `close` fails.
- [ ] Non-string key/button arguments fail.
- [ ] `io.input.Input` used as a value fails.

### Phase 3 gate

- [ ] Optional external signature tests pass.
- [ ] Input frontend/signature tests pass.
- [ ] Run `cargo fmt`.
- [ ] Run `cargo clippy`.
- [ ] Run `just validate`.
- [ ] Manual audit:
  - [ ] optional returns use canonical option typing;
  - [ ] input handle identity is opaque;
  - [ ] no IO-specific option representation exists;
  - [ ] raw external type IDs are not duplicated.

---

## Phase 4 — HTML-JS input runtime

### Context

Metadata and frontend validation exist. Implement the actual HTML-JS handle and polling behavior through the current JS helper-emission system.

### 4.1 Runtime handle shape

#### Checklist

- [ ] Define one JS input handle object shape with:
  - [ ] `closed`;
  - [ ] `controller`;
  - [ ] pending event queue;
  - [ ] held key set;
  - [ ] pressed key set;
  - [ ] released key set;
  - [ ] held pointer button set;
  - [ ] pressed pointer button set;
  - [ ] released pointer button set;
  - [ ] `pointerX`;
  - [ ] `pointerY`;
  - [ ] `lastKeyPressed`;
  - [ ] `lastKeyReleased`;
  - [ ] `lastPointerPressed`;
  - [ ] `lastPointerReleased`.
- [ ] Keep pending raw events separate from edge state.
- [ ] Do not expose DOM events through Beanstalk.
- [ ] Avoid global hidden input state; state belongs to the handle.

### 4.2 `io.input.new()`

#### Checklist

- [ ] Implement `__bs_io_input_new`.
- [ ] Feature-detect browser APIs required by HTML-JS.
- [ ] Return `Error!` if input support is unavailable.
- [ ] Create `AbortController`.
- [ ] Register listeners with `signal`.
- [ ] Use passive listeners where safe.
- [ ] Register:
  - [ ] `keydown`;
  - [ ] `keyup`;
  - [ ] `pointermove`;
  - [ ] `pointerdown`;
  - [ ] `pointerup`;
  - [ ] `pointercancel`;
  - [ ] `blur`;
  - [ ] `visibilitychange`.
- [ ] Do not call `preventDefault()`.

### 4.3 Keyboard handling

#### Checklist

- [ ] Implement shared key normalization helper.
- [ ] Normalize `" "` to `"Space"`.
- [ ] Normalize single alphabetic keys to lowercase.
- [ ] Preserve special-key names.
- [ ] Preserve `"Unidentified"` for `last_*`; polling it returns true only if the user asks for `"Unidentified"`.
- [ ] Ignore auto-repeat for `key_pressed` when the key is already held.

### 4.4 Pointer handling

#### Checklist

- [ ] Implement shared pointer button normalization helper.
- [ ] Map browser button values:
  - [ ] `0` -> `"left"`;
  - [ ] `1` -> `"middle"`;
  - [ ] `2` -> `"right"`;
  - [ ] all others ignored in V1.
- [ ] Update coordinates from `clientX` / `clientY`.
- [ ] Return coordinates as `Float`.
- [ ] On `pointercancel`, clear held pointer buttons and produce release edges where practical.
- [ ] On blur or visibility loss, clear held keys/buttons and produce release edges where practical.

### 4.5 Update, polling, and close

#### Checklist

- [ ] Implement `__bs_io_input_update`.
  - [ ] Clear previous edge sets and `last_*` values.
  - [ ] Drain pending events.
  - [ ] Apply events in backend delivery order.
  - [ ] Permit press and release edges for the same key/button in one update if both events happened.
- [ ] Implement `__bs_io_input_close`.
  - [ ] abort listeners;
  - [ ] clear pending events;
  - [ ] clear held/edge sets;
  - [ ] reset pointer coordinates to `0.0`;
  - [ ] mark closed.
- [ ] Implement polling helpers.
  - [ ] closed handles return neutral values;
  - [ ] unknown strings return `false`;
  - [ ] missing `last_*` returns `none`.
- [ ] Ordinary closed-handle polling must not throw.
- [ ] Invalid handle shapes can remain internal runtime errors.

### 4.6 Helper emission

#### Checklist

- [ ] Add helpers to the existing demand-driven JS runtime/helper emission system.
- [ ] Emit shared dependencies when any input helper is reachable:
  - [ ] key normalization;
  - [ ] button normalization;
  - [ ] neutral-state handling;
  - [ ] optional construction helpers if required;
  - [ ] fallible carrier helpers for `new()` if required.
- [ ] Add artifact/golden coverage for helper presence/absence.

### Phase 4 gate

- [ ] JS backend tests pass.
- [ ] HTML-JS input integration tests pass.
- [ ] Artifact/golden tests pass.
- [ ] Run `cargo fmt`.
- [ ] Run `cargo clippy`.
- [ ] Run `just validate`.
- [ ] Runtime audit:
  - [ ] no `preventDefault()` by default;
  - [ ] no Beanstalk callbacks;
  - [ ] no promises/async dependency;
  - [ ] listener cleanup uses abortable listeners;
  - [ ] closed handles are neutral;
  - [ ] stuck key/button prevention exists.

---

## Phase 5 — Backend validation and unsupported targets

### Context

Make target support explicit. HTML-JS supports this V1 slice. Other backends reject reachable unsupported calls before lowering unless they implement equivalent lowerings.

### Checklist

- [ ] Update `src/backends/external_package_validation.rs`.
- [ ] Remove the old `ExternalFunctionId::Io` Wasm special-case.
- [ ] Confirm every new console/input function has JS lowering metadata.
- [ ] Confirm Wasm/native behavior is deliberate:
  - [ ] either implement specific lowerings;
  - [ ] or reject reachable calls with structured unsupported-backend diagnostics.
- [ ] Add negative HTML-Wasm tests for reachable unsupported input calls.
- [ ] Add negative HTML-Wasm tests for reachable console calls if console no longer has Wasm lowering.
- [ ] Ensure unreachable wrappers using IO do not fail backend validation.
- [ ] Ensure diagnostics name the external function and package path clearly.

### Phase 5 gate

- [ ] Backend validation tests pass.
- [ ] Run `cargo fmt`.
- [ ] Run `cargo clippy`.
- [ ] Run `just validate`.
- [ ] Manual audit:
  - [ ] unsupported target errors happen before lowering;
  - [ ] unreachable helper bodies are ignored by reachability validation;
  - [ ] runtime `Error!` is not used for unsupported backend capability.

---

## Phase 6 — Docs, progress matrix, roadmap, and examples

### Context

Make the repo documentation match the implemented language. This phase must remove stale `io(...)` and `IO` references from public examples.

### 6.1 Language docs

#### Checklist

- [ ] Update `docs/language-overview.md`.
- [ ] Replace “Standard Output” with “Core IO” or equivalent.
- [ ] Document:
  - [ ] `io.line` as replacement for old output form;
  - [ ] console functions and string-only boundary;
  - [ ] `io.input.Input` opaque handle;
  - [ ] `new`, `update`, `close`;
  - [ ] key/pointer polling semantics;
  - [ ] normalized key strings;
  - [ ] pointer coordinates;
  - [ ] HTML-JS limitations around synchronous browser loops;
  - [ ] deferred IO surfaces.
- [ ] Remove public `io(...)` examples.
- [ ] Remove public `IO` wording unless explicitly describing removed historical behavior.

### 6.2 Progress matrix

#### Checklist

Update `docs/src/docs/progress/#page.bst`.

- [ ] Update “String coercion and IO/template boundaries”:
  - [ ] replace old `io` wording with console function names;
  - [ ] state string-compatible values only.
- [ ] Update “Paths and imports”:
  - [ ] nested namespace traversal is supported;
  - [ ] namespace records are compile-time records, not values;
  - [ ] type-position qualified namespace support exists;
  - [ ] any intentionally deferred namespace/facade recursion is explicit.
- [ ] Add or update “Core IO” row:
  - [ ] console output support;
  - [ ] input polling support;
  - [ ] runtime target is HTML-JS for V1;
  - [ ] deferred filesystem/fetch/frame/targeted-input/event-queue surfaces.
- [ ] Update Wasm notes for reachable unsupported IO calls.
- [ ] Update coverage summaries to match added tests.

### 6.3 Roadmap

#### Checklist

- [ ] Update `docs/roadmap/roadmap.md`.
- [ ] Mark nested namespace traversal and core IO V1 status accurately.
- [ ] Add follow-up bullets for all deferred IO surfaces.
- [ ] Add namespace re-export as a separate follow-up if not implemented.

### 6.4 README and docs-site examples

#### Checklist

- [ ] Update `README.md` examples from `io(...)` to `io.line(...)`.
- [ ] Update `docs/src/docs/**` examples.
- [ ] Update generated docs/goldens if the docs build requires it.
- [ ] Run:
  - [ ] `rg "\bio\s*\(" docs README.md tests src`
  - [ ] `rg "\bIO\b" docs README.md tests src`
- [ ] Inspect every remaining match.

### Phase 6 gate

- [ ] Docs/static-site check passes if available.
- [ ] All integration tests pass.
- [ ] Run `cargo fmt`.
- [ ] Run `cargo clippy`.
- [ ] Run `just validate`.
- [ ] Documentation audit:
  - [ ] no stale `io(...)` public examples;
  - [ ] no stale public `IO` surface;
  - [ ] deferred features are clearly labeled;
  - [ ] progress matrix describes implemented behavior, not aspirational behavior.

---

## Optional follow-up — Namespace re-export support

### Context

This is not required for `io.input.*`, because the prelude can alias `@core/io` directly. It is still important for future source-library and facade design.

### Checklist

- [ ] Add namespace export target metadata if needed.
- [ ] Support a finalized public namespace re-export syntax, for example:
  ```beanstalk
  export import @core/io as io
  ```
- [ ] Build recursive facade namespace records from exported namespace targets.
- [ ] Preserve source-library and module-root privacy.
- [ ] Add integration tests:
  - [ ] facade re-exports a nested external namespace;
  - [ ] importing the facade exposes the nested namespace;
  - [ ] private source declarations do not leak;
  - [ ] receiver methods still do not become namespace fields.
- [ ] Update docs/progress matrix.
- [ ] Run `just validate`.
- [ ] Complete a manual facade privacy audit.

---

## Acceptance examples

### Console

```beanstalk
name = "Ada"

io.print("Hello, ")
io.line(name)

io.debug([: debugging [name]])
io.warn("This is a warning")
io.error("This is an error")
```

### Input polling

```beanstalk
input ~= io.input.new()!

io.input.update(~input)

if io.input.key_pressed(input, "d"):
    io.line("pressed d")
;

if io.input.pointer_down(input, "left"):
    x = io.input.pointer_x(input)
    y = io.input.pointer_y(input)

    io.line([: pointer [x], [y]])
;

last_key = io.input.last_key_pressed(input)

if last_key is |key|:
    io.line([: last key: [key]])
;

io.input.close(~input)
```

### Function signature

```beanstalk
poll_input |input ~io.input.Input|:
    io.input.update(~input)

    if io.input.key_released(input, "Escape"):
        io.line("escape released")
    ;
;
```

---

## Risk register

### Optional external returns can become an IO-specific hack

`last_* -> String?` must use the canonical option representation.

Mitigation:

- implement optional external signatures before `last_*`;
- test a non-IO external `String?` return;
- reject any IO-only `null` or empty-string sentinel design.

### String-only console output depends on the correct boundary type

`ExternalAbiType::Utf8Str` may be too narrow if it only accepts string slices and not owned/template strings.

Mitigation:

- audit existing old `io(...)` string-boundary logic;
- extract a reusable `StringContent`-style external parameter if needed;
- add tests for both `"text"` and `[: text]`.

### Nested namespace traversal can accidentally create runtime namespaces

Namespaces must resolve before HIR.

Mitigation:

- keep namespace records in header/import visibility;
- ensure namespace used as value is invalid;
- inspect HIR to confirm external calls carry only IDs.

### Recursive namespace records can leak facade-private symbols

IO does not require recursive source/facade namespace exports.

Mitigation:

- keep source/facade namespace records shallow unless privacy tests are added;
- put namespace re-export in the optional follow-up if it expands scope.

### Browser polling without a frame API can be misunderstood

A synchronous infinite `loop:` can block browser event delivery.

Mitigation:

- document V1 input as a primitive for future scheduling/runtime surfaces;
- do not add `io.frame.*` in this plan;
- avoid examples with infinite synchronous browser loops.

---

## Final completion checklist

- [ ] `io(...)` is no longer valid public API.
- [ ] `io.line(...)` replaces all old examples/tests.
- [ ] Public `IO` is removed.
- [ ] `io` is a preluded namespace alias to `@core/io`.
- [ ] `import @core/io` binds `io`.
- [ ] `import @core/io as alias` works.
- [ ] Nested value namespace traversal works.
- [ ] Nested type namespace traversal works.
- [ ] External package symbol paths are structured.
- [ ] Redundant flat cloned external package definition maps are removed or deliberately minimized.
- [ ] Old IO-specific string-boundary logic is removed or generalized.
- [ ] `@core/io` registers console and input symbols.
- [ ] Console output works in HTML-JS.
- [ ] Input polling type-checks.
- [ ] Input polling works in HTML-JS.
- [ ] Unsupported backend validation is structured.
- [ ] README updated.
- [ ] Language docs updated.
- [ ] Docs-site examples updated.
- [ ] Progress matrix updated.
- [ ] Roadmap updated.
- [ ] `rg "\bio\s*\("` has no stale old API matches.
- [ ] `rg "\bIO\b"` has no stale public IO matches.
- [ ] `just validate` passes.
- [ ] Manual stage-boundary review completed.
- [ ] Final style-guide review completed.

---

## Validation command set

Use focused tests during each phase, then the normal validation path at the phase gate:

```bash
cargo fmt
cargo clippy
cargo test
cargo run tests
just validate
```

For frontend boundary changes, also complete the manual stage-boundary review:

- header/import preparation owns visibility;
- AST consumes visibility and resolves semantic use;
- HIR contains stable semantic IDs, not import syntax;
- backend validation owns unsupported target errors;
- user-facing errors use `CompilerDiagnostic`;
- infrastructure/backend failures use `CompilerError`.

