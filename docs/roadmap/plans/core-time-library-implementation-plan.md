# Beanstalk `@core/time` implementation plan

## Goal

Implement the first stable JS-lowering slice of `@core/time` around three core concepts:

- `Duration`: signed elapsed time, represented as milliseconds internally.
- `TimeMark`: monotonic clock mark for deltas, profiling, animations, and games.
- `Timestamp`: UTC wall-clock instant for logs, storage, and external/system boundaries.

This plan deliberately avoids the old ambiguous `now_millis()` / `now_seconds()` API and replaces it with explicit monotonic and wall-clock names. Beanstalk is still pre-release, so do not add compatibility wrappers unless a later product decision explicitly overrides this plan.

## Source anchors in the current repo

Use these files as the main implementation anchors:

- `src/libraries/core/time.rs` — current `@core/time` package registration.
- `src/libraries/core/mod.rs` — core package module map and optional package path list.
- `src/libraries/library_set.rs` — HTML builder opt-in for optional core libraries.
- `src/compiler_frontend/external_packages/{abi.rs,definitions.rs,registry.rs}` — external ABI/type/function metadata.
- `src/backends/js/js_statement.rs` — external call lowering through `RuntimeFunction`, `InlineExpression`, and `ExternalModuleExport`.
- `src/backends/js/libraries/core/{mod.rs,time.rs}` — JS helper emission for optional core libraries.
- `src/backends/external_package_validation.rs` — reachable-call backend support validation.
- `tests/cases/manifest.toml` and `tests/cases/*` — integration fixtures.
- `docs/language-overview.md` — compiler-facing language facts and deferred-surface notes.
- `docs/src/docs/libraries/#page.bst` — existing user-facing libraries docs page.
- `docs/src/docs/progress/#page.bst` — implementation matrix.
- `docs/roadmap/roadmap.md` — deliberately deferred follow-up tracking.

## Final public API for this implementation

```beanstalk
-- Types
Duration
TimeMark
Timestamp

-- Monotonic elapsed time
mark_now || -> TimeMark
elapsed_since |start TimeMark| -> Duration
duration_between |start TimeMark, end TimeMark| -> Duration

-- Wall-clock timestamp
timestamp_now || -> Timestamp

-- Duration construction
duration_from_seconds |seconds Float| -> Duration
duration_from_milliseconds |milliseconds Float| -> Duration

-- Duration receiver methods
as_seconds |this Duration| -> Float
as_milliseconds |this Duration| -> Float
is_negative |this Duration| -> Bool
abs |this Duration| -> Duration
clamp |this Duration, min Duration, max Duration| -> Duration

-- Timestamp construction/conversion
timestamp_from_unix_seconds |seconds Float| -> Timestamp
timestamp_from_unix_milliseconds |milliseconds Float| -> Timestamp
timestamp_from_iso_string |text String| -> Timestamp, Error!

unix_seconds |this Timestamp| -> Float
unix_milliseconds |this Timestamp| -> Float
to_iso_string |this Timestamp| -> String
```

## JS representation contract

For this JS-only first implementation, keep the runtime representation intentionally simple:

| Beanstalk type | JS representation | Source |
|---|---:|---|
| `Duration` | `number`, signed milliseconds | arithmetic / constructors |
| `TimeMark` | `number`, monotonic milliseconds | `globalThis.performance.now()` |
| `Timestamp` | `number`, Unix epoch milliseconds | `Date.now()`, `Date.parse(...)` |

These are still external opaque types in the Beanstalk type system. Users can pass and return them through `@core/time` functions, but cannot construct them as structs or access fields.

## Deliberately deferred features

Track these explicitly in the roadmap and implementation matrix:

- Civil/calendar types: `Date`, `TimeOfDay`, `DateTime`, `TimeZone`, `ZonedDateTime`, `Period`.
- Temporal-backed implementation of calendar/time-zone behavior.
- Local time zone lookup and conversions.
- Locale-aware formatting and parsing.
- Timers, `sleep`, intervals, animation callbacks, and `requestAnimationFrame` integration.
- Web-specific animation/game-loop scheduling packages such as future `@web/animation`.
- Wasm/native time lowerings.
- Higher-precision integer/nanosecond timestamp ABI work, if/when Beanstalk adds wider numeric ABI types.
- Validation policy for non-finite `Float` values passed into numeric timestamp/duration constructors.

---

# Phase 0 — Baseline audit and replacement inventory

## Context

The current library has only `now_millis()` and `now_seconds()`. Before changing code, identify every place that mentions those names so this implementation can replace the old surface cleanly instead of carrying stale docs or fixtures.

## Checklist

- [ ] Run repository search for old API names:
  - [ ] `rg "now_millis|now_seconds" src docs tests libraries`
  - [ ] Record every source, docs, progress-matrix, and test hit.
- [ ] Inspect current `@core/time` registration in `src/libraries/core/time.rs`.
- [ ] Inspect current JS helper emission in `src/backends/js/libraries/core/time.rs`.
- [ ] Inspect existing core package test naming conventions in `tests/cases/manifest.toml` and nearby `core_*` fixtures.
- [ ] Confirm no external stable docs or examples need a transitional alias. Default answer should be no: remove the old names.
- [ ] Confirm that the HTML builder still exposes optional core packages through `LibrarySet::expose_html_core_libraries()` and that no change is required there unless tests reveal otherwise.

## Audit / style / validation gate

- [ ] Confirm this phase made no behavior changes.
- [ ] Write down any newly discovered files that must be edited in later phases.
- [ ] If the discovered dependency graph differs from this plan, update the plan notes before implementing.

---

# Phase 1 — Register the new typed `@core/time` API

## Context

This phase is frontend/package metadata only. It defines the Beanstalk-visible package surface: opaque types, free functions, receiver methods, signatures, fallibility, access modes, and JS lowering metadata. No backend helper logic beyond inline expression strings is implemented here.

## Files to edit

- `src/libraries/core/time.rs`

Potentially inspect but do not expect to change:

- `src/libraries/core/mod.rs`
- `src/libraries/library_set.rs`
- `src/compiler_frontend/external_packages/definitions.rs`
- `src/compiler_frontend/external_packages/registry.rs`

## Checklist

- [ ] Replace the existing `now_millis` / `now_seconds` registrations with the new API. Do not leave compatibility wrappers.
- [ ] Update the file-level docs in `src/libraries/core/time.rs` to describe the new split:
  - [ ] `Duration` for elapsed amounts.
  - [ ] `TimeMark` for monotonic deltas.
  - [ ] `Timestamp` for UTC wall-clock instants.
- [ ] Register external opaque types in this order for readability:
  - [ ] `Duration`
  - [ ] `TimeMark`
  - [ ] `Timestamp`
- [ ] Use `ExternalTypeSpec { abi_type: ExternalAbiType::Handle }` for all three types.
- [ ] Store each returned `ExternalTypeId` and build `ExternalSignatureType::External(type_id)` values for signatures.
- [ ] Add small local helpers to keep the registration readable:
  - [ ] `shared_param(signature_type) -> ExternalParameter`
  - [ ] `register_external_time_function(...)`
  - [ ] Optional helpers for `ExternalReturnSlot::fresh(...)`
- [ ] Register free functions:
  - [ ] `mark_now || -> TimeMark`
    - JS lowering: `InlineExpression("globalThis.performance.now()")`
  - [ ] `elapsed_since |start TimeMark| -> Duration`
    - JS lowering: `InlineExpression("(globalThis.performance.now() - #0)")`
  - [ ] `duration_between |start TimeMark, end TimeMark| -> Duration`
    - JS lowering: `InlineExpression("(#1 - #0)")`
  - [ ] `timestamp_now || -> Timestamp`
    - JS lowering: `InlineExpression("Date.now()")`
  - [ ] `duration_from_seconds |seconds Float| -> Duration`
    - JS lowering: `InlineExpression("(#0 * 1000.0)")`
  - [ ] `duration_from_milliseconds |milliseconds Float| -> Duration`
    - JS lowering: `InlineExpression("#0")`
  - [ ] `timestamp_from_unix_seconds |seconds Float| -> Timestamp`
    - JS lowering: `InlineExpression("(#0 * 1000.0)")`
  - [ ] `timestamp_from_unix_milliseconds |milliseconds Float| -> Timestamp`
    - JS lowering: `InlineExpression("#0")`
  - [ ] `timestamp_from_iso_string |text String| -> Timestamp, Error!`
    - JS lowering: `RuntimeFunction("__bs_time_timestamp_from_iso_string")`
    - `returns`: one success slot of `Timestamp`.
    - `error_return_type`: `Some(ExternalSignatureType::BuiltinError)`.
- [ ] Register `Duration` receiver methods:
  - [ ] `as_seconds |this Duration| -> Float`
    - receiver type: `Duration`, shared access.
    - parameters include the receiver as parameter `0`.
    - JS lowering: `InlineExpression("(#0 / 1000.0)")`
  - [ ] `as_milliseconds |this Duration| -> Float`
    - JS lowering: `InlineExpression("#0")`
  - [ ] `is_negative |this Duration| -> Bool`
    - JS lowering: `InlineExpression("(#0 < 0)")`
  - [ ] `abs |this Duration| -> Duration`
    - JS lowering: `InlineExpression("Math.abs(#0)")`
  - [ ] `clamp |this Duration, min Duration, max Duration| -> Duration`
    - JS lowering: `InlineExpression("Math.min(Math.max(#0, #1), #2)")`
- [ ] Register `Timestamp` receiver methods:
  - [ ] `unix_seconds |this Timestamp| -> Float`
    - JS lowering: `InlineExpression("(#0 / 1000.0)")`
  - [ ] `unix_milliseconds |this Timestamp| -> Float`
    - JS lowering: `InlineExpression("#0")`
  - [ ] `to_iso_string |this Timestamp| -> String`
    - JS lowering: `InlineExpression("(new Date(#0)).toISOString()")`
- [ ] Keep all `wasm` lowerings as `None` for this phase.
- [ ] Confirm all external functions use `ExternalReturnAlias::Fresh`.
- [ ] Confirm all parameters use `ExternalAccessKind::Shared`; no mutable receiver methods are needed.
- [ ] Confirm `src/libraries/core/mod.rs` already lists `@core/time` in `OPTIONAL_CORE_PACKAGE_PATHS`; if not, add it.
- [ ] Confirm `LibrarySet::expose_html_core_libraries()` still registers `register_core_time_package`.

## Audit / style / validation gate

- [ ] Check the file follows the codebase style guide:
  - [ ] file-level docs explain WHAT/WHY.
  - [ ] registration code reads as named steps, not a long wall of literals.
  - [ ] no stale comments mentioning only wall-clock helpers.
  - [ ] no compatibility wrappers for old names.
  - [ ] no unnecessary clever macros.
- [ ] Run targeted checks:
  - [ ] `cargo fmt --all --check`
  - [ ] `cargo test external_packages`
  - [ ] `cargo test libraries`
- [ ] If targeted test names differ, run the nearest relevant test modules and record the command used.

---

# Phase 2 — Implement JS helper behavior

## Context

Most functions can lower inline because they are pure expressions. `timestamp_from_iso_string` should be a runtime helper because it needs validation and must return the internal fallible carrier shape.

The JS backend already records referenced external functions while lowering calls and emits core helpers only for referenced runtime functions. Keep that behavior intact.

## Files to edit

- `src/backends/js/libraries/core/time.rs`

Potentially inspect but do not expect to change:

- `src/backends/js/libraries/core/mod.rs`
- `src/backends/js/js_statement.rs`
- `src/backends/js/runtime/errors.rs`
- `src/backends/js/runtime/results.rs`

## Checklist

- [ ] Replace the old `__bs_time_now_seconds` helper with `__bs_time_timestamp_from_iso_string`.
- [ ] Implement the helper as a one-line helper or a readable multi-line emission block consistent with nearby core helper files.
- [ ] Helper behavior:

```js
function __bs_time_timestamp_from_iso_string(text) {
  const millis = Date.parse(text);
  if (Number.isNaN(millis)) {
    return {
      tag: "err",
      value: __bs_make_error("Invalid ISO timestamp", 400, null, null)
    };
  }
  return { tag: "ok", value: millis };
}
```

- [ ] Use the existing `__bs_make_error(...)` runtime helper rather than constructing public error records manually.
- [ ] Keep the helper emitted only when `referenced_external_runtime_function("__bs_time_timestamp_from_iso_string")` is true.
- [ ] Remove stale docs/comments that say only `now_seconds` uses a helper.
- [ ] Confirm no helper is required for inline-only functions:
  - [ ] `mark_now`
  - [ ] `elapsed_since`
  - [ ] `duration_between`
  - [ ] `timestamp_now`
  - [ ] duration constructors/methods
  - [ ] timestamp numeric constructors/methods
- [ ] Confirm the emitted JS can reference `globalThis.performance.now()` from inline expressions in browser and Node-style test runtimes.
- [ ] Confirm the emitted JS can reference `Date`, `Date.now`, `Date.parse`, and `new Date(...).toISOString()` without imports.

## Audit / style / validation gate

- [ ] Confirm helper emission remains reachability/reference driven.
- [ ] Confirm no general runtime prelude pollution was added for optional time helpers.
- [ ] Confirm fallible helper returns the same `{ tag: "ok" | "err", value }` shape used by generated fallible lowering.
- [ ] Run targeted checks:
  - [ ] `cargo fmt --all --check`
  - [ ] `cargo test js`
  - [ ] `cargo test backends`
- [ ] If targeted test names differ, run the nearest relevant JS backend test modules and record the command used.

---

# Phase 3 — Add integration and regression coverage

## Context

The API is mostly external package metadata plus JS lowering, so integration tests should prove real Beanstalk imports, type checking, receiver method visibility, fallible parsing, runtime output, and unsupported Wasm behavior. Avoid exact assertions on live wall-clock values; assert stable derived facts instead.

## Files to edit

- `tests/cases/manifest.toml`
- New `tests/cases/core_time_*` fixture directories.
- Existing expected/golden files as required by the test runner.

Optional unit-test files, if the current structure already has a suitable owner:

- `src/compiler_frontend/external_packages/tests/*`
- `src/backends/js/tests/*`
- `src/libraries/tests/*`

## Checklist

### Positive runtime fixtures

- [ ] Add `core_time_duration_conversions_success`:
  - [ ] Imports `Duration`, `duration_from_seconds`, `duration_from_milliseconds`.
  - [ ] Exercises `as_seconds`, `as_milliseconds`, `is_negative`, `abs`, and `clamp`.
  - [ ] Uses deterministic values such as `1.5` seconds and `-250.0` milliseconds.
  - [ ] Asserts stable output, not current time.
- [ ] Add `core_time_mark_delta_success`:
  - [ ] Imports `TimeMark`, `Duration`, `mark_now`, `elapsed_since`, and `duration_between`.
  - [ ] Creates two marks and asserts the delta is non-negative with a stable `ok` / `bad` output.
  - [ ] Avoids exact elapsed output.
- [ ] Add `core_time_timestamp_conversions_success`:
  - [ ] Imports `Timestamp`, timestamp constructors, `to_iso_string`, `unix_seconds`, `unix_milliseconds`.
  - [ ] Uses epoch values such as `0.0` and `1000.0`.
  - [ ] Asserts ISO output such as `1970-01-01T00:00:00.000Z`.
- [ ] Add `core_time_timestamp_parse_success`:
  - [ ] Parses a valid ISO string with `timestamp_from_iso_string(...)!`.
  - [ ] Converts back to Unix milliseconds and/or ISO string.
  - [ ] Asserts stable output.
- [ ] Add `core_time_timestamp_parse_invalid_catch_success`:
  - [ ] Calls `timestamp_from_iso_string("not-a-date") catch:`.
  - [ ] Recovers to a fallback `Timestamp` or fallback string.
  - [ ] Asserts the catch path runs.

### Import and receiver-method fixtures

- [ ] Add `core_time_namespace_import_success`:
  - [ ] Uses `import @core/time`.
  - [ ] Calls `time.duration_from_seconds(1.0)` and type annotations such as `time.Duration` if current namespace type-position syntax supports this.
- [ ] Add `core_time_grouped_alias_success`:
  - [ ] Uses grouped aliases such as `duration_from_seconds as seconds`.
  - [ ] Confirms aliased free functions work.
- [ ] Add `core_time_receiver_visibility_success`:
  - [ ] Imports `Duration` and a constructor.
  - [ ] Calls `duration.as_seconds()` without importing `as_seconds` directly, if receiver auto-import through visible receiver type is expected.
- [ ] Add `core_time_receiver_method_without_visible_type_rejected` only if current diagnostics make this a useful stable case.

### Negative diagnostics

- [ ] Add `core_time_old_api_rejected`:
  - [ ] Attempts to import or call `now_millis` / `now_seconds`.
  - [ ] Asserts missing-symbol diagnostics.
- [ ] Add `core_time_wrong_argument_type_rejected`:
  - [ ] Passes `String` where `TimeMark`, `Duration`, or `Timestamp` is required.
  - [ ] Asserts stable diagnostic codes, not fragile prose.
- [ ] Add `core_time_wrong_arity_rejected`:
  - [ ] Calls `duration_between(mark)` or `duration_from_seconds()` with invalid arity.
- [ ] Add `core_time_opaque_field_access_rejected`:
  - [ ] Attempts to access a field on `Duration` or `Timestamp`.
  - [ ] Asserts the external opaque type remains opaque.

### Backend support fixtures

- [ ] Add or update `core_time_wasm_unsupported`:
  - [ ] A reachable `@core/time` call should fail HTML-Wasm with structured unsupported external-function diagnostics.
  - [ ] JS/HTML should succeed for the same fixture if the test harness supports backend-specific assertions.
- [ ] Add or update a reachability fixture if one already exists:
  - [ ] An unused wrapper around a `@core/time` call should not fail HTML-Wasm if the call is not reachable from entry `start`, matching reachable-call validation policy.

### Manifest and expectations

- [ ] Add every new case to `tests/cases/manifest.toml`.
- [ ] Use tags consistently, for example:
  - [ ] `core`
  - [ ] `time`
  - [ ] `external-packages`
  - [ ] `js-backend`
  - [ ] `diagnostics`
- [ ] Prefer behavior assertions over large exact JS goldens unless exact output is contractual.
- [ ] Do not use nondeterministic wall-clock output directly in golden files.

## Audit / style / validation gate

- [ ] Review fixtures for clear, focused names.
- [ ] Confirm success fixtures use real Beanstalk code rather than implementation-only shortcuts.
- [ ] Confirm failure fixtures assert stable diagnostic codes where practical.
- [ ] Confirm nondeterministic time values are reduced to stable booleans or deterministic conversions.
- [ ] Run targeted integration tests for the new cases.
- [ ] Run full integration tests:
  - [ ] `cargo run -- tests`
- [ ] Run full style/validation if phase size allows:
  - [ ] `just validate`

---

# Phase 4 — Update compiler-facing language docs

## Context

`docs/language-overview.md` should stay focused on compiler-facing facts: import rules, external package rules, supported/deferred surface, and invariants. The friendly user guide belongs in `docs/src/docs/**`.

## Files to edit

- `docs/language-overview.md`

## Checklist

- [ ] In the external platform package section, replace the current `@core/time` bullet:
  - Old meaning: `now_millis`, `now_seconds`; richer date/time APIs deferred.
  - New meaning: `Duration`, `TimeMark`, `Timestamp`, monotonic mark helpers, duration conversion helpers, timestamp conversion helpers, ISO timestamp parsing/formatting.
- [ ] Add one short rule explaining the semantic split:
  - [ ] Use `TimeMark` for elapsed time and frame deltas.
  - [ ] Use `Timestamp` for real-world UTC instants.
  - [ ] Use `Duration` for elapsed amounts.
- [ ] Add a concise note that `timestamp_from_iso_string` is fallible and must be handled with `!` or `catch`.
- [ ] Update deferred library-system features:
  - [ ] Remove “durations” and “monotonic clocks” from deferred wording.
  - [ ] Keep full date/time/time-zone/calendar APIs deferred.
  - [ ] Add locale formatting, async timers/sleep, animation scheduling, Temporal-backed calendar types, and non-JS lowerings as deferred.
- [ ] Ensure language docs still say optional core libraries require explicit imports unless prelude-imported.
- [ ] Do not add a long tutorial here; link the user-facing docs page if linking conventions support it.

## Audit / style / validation gate

- [ ] Confirm `docs/language-overview.md` remains compiler-facing, not a tutorial.
- [ ] Confirm every deferred feature listed here also appears in either the progress matrix or roadmap notes.
- [ ] Run docs/source formatting checks used by the repo, if any.
- [ ] Run `just validate` if this phase is combined with code changes.

---

# Phase 5 — Add user-facing docs-site pages for core libraries

## Context

The docs site already has `docs/src/docs/libraries/#page.bst`. Keep it as the high-level library-system guide, but add a new core-library docs section under `docs/src/docs/libraries/core/`. The time page should be the most complete new page because it documents the new API.

## Files to add/edit

Edit existing index pages:

- `docs/src/docs/#page.bst`
- `docs/src/docs/libraries/#page.bst`

Add new core-library docs pages:

- `docs/src/docs/libraries/core/#page.bst`
- `docs/src/docs/libraries/core/prelude/#page.bst`
- `docs/src/docs/libraries/core/io/#page.bst`
- `docs/src/docs/libraries/core/collections/#page.bst`
- `docs/src/docs/libraries/core/math/#page.bst`
- `docs/src/docs/libraries/core/text/#page.bst`
- `docs/src/docs/libraries/core/random/#page.bst`
- `docs/src/docs/libraries/core/time/#page.bst`

If the docs router expects file pages rather than folder pages, adapt the exact page paths to match current routing conventions, but keep the public route shape equivalent to `/docs/libraries/core/time`.

## Checklist

### Navigation/index updates

- [ ] Update `docs/src/docs/#page.bst`:
  - [ ] Keep the existing Libraries link.
  - [ ] Add a nested or nearby link to core libraries if the page layout supports it.
- [ ] Update `docs/src/docs/libraries/#page.bst`:
  - [ ] Add a concise friendly introduction to core libraries.
  - [ ] Add links to all new core-library pages.
  - [ ] Link directly to the new time page.
  - [ ] Avoid duplicating the full time API; point users to `@./core/time`.

### Core library index page

- [ ] Create `docs/src/docs/libraries/core/#page.bst`.
- [ ] Include a short overview:
  - [ ] Core libraries are explicit-import packages unless they are prelude symbols.
  - [ ] Builder support can vary; the implementation matrix is the source of truth.
  - [ ] JS/HTML is the current supported target for optional core packages.
- [ ] Add a small table or bullet list of current pages:
  - [ ] Prelude
  - [ ] IO
  - [ ] Collections
  - [ ] Math
  - [ ] Text
  - [ ] Random
  - [ ] Time

### Minimal pages for existing core libraries

Each page should be concise and user-friendly. Do not turn these into full specs yet.

- [ ] `prelude/#page.bst`:
  - [ ] Explain bare `io` / `IO` availability.
  - [ ] Explain that most core libraries still require imports.
- [ ] `io/#page.bst`:
  - [ ] Explain standard output with `io(...)`.
  - [ ] Mention string/template boundary behavior at a high level.
- [ ] `collections/#page.bst`:
  - [ ] Explain collection literals and common methods at a user-guide level.
  - [ ] Link to the main collections language page if appropriate.
- [ ] `math/#page.bst`:
  - [ ] List constants and common functions.
  - [ ] Show a short `sin` / `clamp` example.
- [ ] `text/#page.bst`:
  - [ ] List `length`, `is_empty`, `contains`, `starts_with`, `ends_with`.
  - [ ] Mention JS string-length semantics if already documented elsewhere.
- [ ] `random/#page.bst`:
  - [ ] Explain `random_float` and `random_int`.
  - [ ] Mention seeded random is deferred.

### Time page requirements

- [ ] Create `docs/src/docs/libraries/core/time/#page.bst`.
- [ ] Include a friendly intro:
  - [ ] `Duration` is for elapsed amounts.
  - [ ] `TimeMark` is for measuring elapsed time and frame deltas.
  - [ ] `Timestamp` is for real-world UTC time.
- [ ] Include import examples:

```beanstalk
import @core/time {
    Duration,
    TimeMark,
    mark_now,
    duration_between,
}
```

- [ ] Include a delta-time example that does not require a completed animation scheduler:

```beanstalk
previous ~= mark_now()

current = mark_now()
delta = duration_between(previous, current)
previous = current

seconds = delta.as_seconds()
```

- [ ] Include a timestamp example:

```beanstalk
import @core/time {Timestamp, timestamp_from_iso_string}

created = timestamp_from_iso_string("1970-01-01T00:00:00.000Z")!
io(created.to_iso_string())
```

- [ ] Include an error-handling example using `catch` for invalid ISO strings.
- [ ] Include a small API table for the first implementation.
- [ ] Include a “What is deferred?” section:
  - [ ] calendar dates
  - [ ] time zones
  - [ ] locale formatting
  - [ ] timers/sleep/animation callbacks
  - [ ] Wasm/non-JS support
- [ ] Keep tone friendly and concise.

## Audit / style / validation gate

- [ ] Confirm every new docs page has `page_title`, `page_description`, `page_head`, `navbar`, and `title` consistent with existing docs pages.
- [ ] Confirm links are relative and route-stable.
- [ ] Confirm code examples are valid current Beanstalk syntax.
- [ ] Confirm the time page does not promise `requestAnimationFrame`, `sleep`, local time zones, or Temporal-backed calendar types.
- [ ] Run the docs build or normal validation path used by the repository.
- [ ] Run `just validate` if feasible after docs additions.

---

# Phase 6 — Update progress matrix and roadmap deferred-feature tracking

## Context

The progress matrix is the source of truth for what is currently supported. The roadmap is where deliberately deferred follow-ups should be discoverable. This phase must update both so the new time library is not documented as deferred after it exists.

## Files to edit

- `docs/src/docs/progress/#page.bst`
- `docs/roadmap/roadmap.md`

## Checklist

### Progress matrix

- [ ] Update the `Core time package` row in `docs/src/docs/progress/#page.bst`.
- [ ] Change the exposed API from `now_millis()` / `now_seconds()` to:
  - [ ] `Duration`
  - [ ] `TimeMark`
  - [ ] `Timestamp`
  - [ ] `mark_now`
  - [ ] `elapsed_since`
  - [ ] `duration_between`
  - [ ] `timestamp_now`
  - [ ] `duration_from_seconds`
  - [ ] `duration_from_milliseconds`
  - [ ] `timestamp_from_unix_seconds`
  - [ ] `timestamp_from_unix_milliseconds`
  - [ ] `timestamp_from_iso_string`
  - [ ] `Duration` receiver methods
  - [ ] `Timestamp` receiver methods
- [ ] Update coverage text after tests are added:
  - [ ] Include import, grouped alias, namespace, receiver method, runtime smoke, fallible parse/catch, arity/type diagnostics, old-name rejection, and Wasm-unsupported coverage if those cases are implemented.
- [ ] Keep status as `Partial` unless the project wants to call this first slice `Supported` despite deferred calendar/time-zone features.
- [ ] Runtime target should stay `JS / HTML`.
- [ ] Watch points should say:
  - [ ] Use `TimeMark`, not `Timestamp`, for frame deltas.
  - [ ] `Timestamp` is UTC wall-clock time and can be affected by system clock changes.
  - [ ] Current JS representation is opaque externally but numeric internally.
  - [ ] Full date/time/time-zone/calendar APIs are deferred.
  - [ ] Wasm support remains deferred and should fail with structured unsupported-backend diagnostics.

### Roadmap

- [ ] Add a “Time library follow-ups” note in `docs/roadmap/roadmap.md`.
- [ ] Explicitly list deferred follow-ups:
  - [ ] `Date`, `TimeOfDay`, `DateTime`, `TimeZone`, `ZonedDateTime`, `Period`.
  - [ ] Temporal-backed JS calendar implementation when runtime availability is good enough or a polyfill policy exists.
  - [ ] Locale-aware formatting/parsing.
  - [ ] Local time zone lookup.
  - [ ] Async timers/sleep and intervals after async/task design exists.
  - [ ] Browser animation-frame integration in a web-specific package, not `@core/time`.
  - [ ] Wasm/native lowerings.
  - [ ] Higher-precision/nanosecond representation if wider numeric ABI work lands.
- [ ] Remove or narrow any older roadmap note that says durations and monotonic clocks are deferred.

## Audit / style / validation gate

- [ ] Confirm matrix, roadmap, language overview, and docs-site pages agree.
- [ ] Confirm no old `now_millis` / `now_seconds` references remain unless they are in migration/negative-test context.
- [ ] Run docs validation/build if available.
- [ ] Run `just validate` if feasible.

---

# Phase 7 — Final cross-stage review and full validation

## Context

This phase is not new feature work. It is the closeout audit that checks the implementation remains clean across frontend metadata, HIR/external call lowering, JS backend helper emission, tests, docs, and roadmap.

## Checklist

### Cross-stage review

- [ ] External package metadata:
  - [ ] Opaque types are package-scoped and registered before functions use them.
  - [ ] Receiver methods have the receiver parameter in `parameters` and set `receiver_type`.
  - [ ] All receiver methods use shared access.
  - [ ] `timestamp_from_iso_string` has exactly one success return and one builtin error slot.
- [ ] JS lowering:
  - [ ] Inline expression placeholders match parameter indices.
  - [ ] No inline expression references missing arguments.
  - [ ] Runtime helper is emitted only when referenced.
  - [ ] Runtime helper returns Beanstalk’s internal fallible carrier shape.
- [ ] Backend support:
  - [ ] JS/HTML lowerings are present for every new function.
  - [ ] Wasm lowerings remain absent by design.
  - [ ] Reachable Wasm usage reports structured unsupported-backend diagnostics.
- [ ] Public API:
  - [ ] `now_millis` and `now_seconds` are fully removed from the supported surface.
  - [ ] Docs do not teach raw wall-clock numbers for frame deltas.
  - [ ] `Timestamp` docs do not imply monotonic behavior.
  - [ ] `TimeMark` docs do not imply serializability or real-world timestamp meaning.
- [ ] Deferred features:
  - [ ] All explicitly deferred features are reflected in roadmap/matrix/docs.
  - [ ] No docs page promises Temporal/calendar APIs in the first implementation.

### Style-guide review

- [ ] No user-input panics were added.
- [ ] No stale comments remain from the old design.
- [ ] No compatibility wrappers preserve obsolete APIs.
- [ ] Main functions read as sequences of named steps.
- [ ] Match arms, if any, are grouped by meaning.
- [ ] No clever helper abstractions hide stage ownership.
- [ ] Tests are outside production files.
- [ ] Diagnostics tests assert stable diagnostic codes where practical.
- [ ] Documentation examples compile or are clearly illustrative.

### Full validation

- [ ] Run `cargo fmt --all --check`.
- [ ] Run `cargo clippy`.
- [ ] Run `cargo test`.
- [ ] Run `cargo run -- tests`.
- [ ] Run `just validate`.
- [ ] Manually review generated docs output if docs pages were added and the project build produces docs artifacts.

---

# Expected final state

## Supported after implementation

- `@core/time` exposes type-safe `Duration`, `TimeMark`, and `Timestamp` opaque types.
- Game/animation delta-time code can use monotonic `TimeMark` and `Duration` without raw wall-clock numbers.
- Wall-clock code can use `Timestamp` and UTC ISO string conversion/parsing.
- Invalid ISO timestamp parsing is handled through normal Beanstalk `Error!` flow.
- The JS/HTML backend supports the full first-slice API.
- HTML-Wasm rejects reachable `@core/time` usage cleanly until Wasm lowerings exist.
- Docs explain the new API and its boundaries.
- Roadmap/matrix clearly list the calendar/time-zone/Temporal/timer features as deferred.

## Not supported after implementation

- Full calendar date/time APIs.
- Time zones and local-time conversion.
- Locale formatting.
- Async sleep/timers/intervals.
- `requestAnimationFrame` or game-loop scheduling.
- Wasm/native time implementations.
- Nanosecond precision or integer timestamp guarantees beyond the current JS `number` representation.
