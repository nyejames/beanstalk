# Standard Math Library plan
This will be the implementation of the core standard math library.
The goal is to use this as a way to harden and fix bugs with the external import system.

The template for the library: 

`
@std/math

Constants:
  PI  Float
  TAU Float
  E   Float

Functions:
  sin(x Float) -> Float
  cos(x Float) -> Float
  tan(x Float) -> Float
  atan2(y Float, x Float) -> Float

  log(x Float) -> Float
  log2(x Float) -> Float
  log10(x Float) -> Float
  exp(x Float) -> Float
  pow(base Float, exponent Float) -> Float

  sqrt(x Float) -> Float
  abs(x Float) -> Float
  floor(x Float) -> Float
  ceil(x Float) -> Float
  round(x Float) -> Float
  trunc(x Float) -> Float

  min(a Float, b Float) -> Float
  max(a Float, b Float) -> Float
  clamp(x Float, min Float, max Float) -> Float
`

This won't be testing methods or Types yet, just:
- constants
- free functions
- Float ABI
- package-scoped imports
- backend lowering

for now.

## Package-system hardening required for `@std/math`

`@std/math` should be treated as the first real standard external package, not just a math helper. Its purpose is to force the external package system through a realistic Alpha path:

- package-scoped imports
- explicit non-prelude import behavior
- external constants
- Float/F64 ABI metadata
- stable backend lowering
- integration-test coverage through the normal HTML/JS build path

The existing design already expects project builders to provide external packages through `BackendBuilder::external_packages()`, with virtual package symbols resolved through file-local `visible_external_symbols`, not global lookup. Backends then map stable external function IDs to backend-specific lowering names. :contentReference[oaicite:0]{index=0}

The current implementation matrix explicitly marks external platform packages as partial and calls out the blockers for `@std/math`: no external constants, no Float/F64 ABI metadata, explicit JS ID-to-runtime-name mapping, and Wasm not counting as Alpha support yet. :contentReference[oaicite:1]{index=1}

### 1. Keep the Alpha surface deliberately narrow

Implement only:

```beanstalk
import @std/math {PI, TAU, E}
import @std/math {sin, cos, tan, atan2, log, log2, log10, exp, pow}
import @std/math {sqrt, abs, floor, ceil, round, trunc, min, max, clamp}
```

Do not include:

* methods like `x.sin()`
* numeric traits/interfaces
* generic `Int | Float` overloads
* user-authored external binding files
* Wasm support as an Alpha requirement

The language docs already allow virtual external packages such as `@std/io` and describe them as builder-provided, non-source packages exposing typed external functions and opaque external types.  For `@std/math`, keep the package virtual and Rust-side for now.

### 2. Add Float/F64 to the external ABI

Current `ExternalAbiType` supports `I32`, `Utf8Str`, `Void`, `Handle`, and `Inferred`, but not Float/F64. That blocks typed math functions in the registry. The implementation should add:

```rust
ExternalAbiType::F64
```

Mapping:

```rust
ExternalAbiType::F64 => Some(DataType::Float)
```

Then register all math function parameters and returns as `F64`.

Required checks:

* `sin(1.0)` works.
* `sin(1)` should only work if existing call-argument compatibility permits contextual `Int -> Float`; current docs say function arguments still require exact compatibility, so initially this should fail unless you deliberately change that rule. 
* `sin("x")` gives a structured type error.
* `sqrt()` and `pow(1.0)` give arity diagnostics.
* named args remain rejected for external calls until external metadata has public parameter names.

### 3. Add external constants to the package registry

`PI`, `TAU`, and `E` should be first-class external package constants, not fake functions.

Add an external constant model beside functions/types:

```rust
pub enum ExternalConstantValue {
    Float(f64),
    Int(i64),
    StringSlice(&'static str),
    Bool(bool),
}

pub struct ExternalConstantDef {
    pub name: &'static str,
    pub data_type: ExternalAbiType,
    pub value: ExternalConstantValue,
}
```

Then extend:

```rust
ExternalSymbolId::Constant(ExternalConstantId)
ExternalPackage {
    constants: HashMap<&'static str, ExternalConstantDef>,
}
ExternalPackageRegistry {
    constants_by_id: ...
    constant_ids_by_package_symbol: ...
}
```

AST import binding must resolve constants from virtual packages the same way it already resolves functions and types. The important rule: imported constants must still be file-local visible symbols, not globally visible names. The compiler design says external expression/type resolution must go through the active `ScopeContext`, not global registry lookup. 

### 4. Represent external constants as compile-time expressions

`PI`, `TAU`, and `E` must behave like constants:

```beanstalk
import @std/math/PI

# turn = PI * 2.0
```

Expected behavior:

* usable in runtime expressions
* usable in top-level constants
* usable in const templates
* fully foldable when only combined with compile-time operations
* rejected if not imported
* collision-checked against same-file declarations/import aliases

Do not lower external constants as runtime host calls. They should enter AST as literal `ExpressionKind::Float(...)` or equivalent compile-time expression data.

### 5. Register `@std/math` in the builtin external registry

Add the package in `ExternalPackageRegistry::new()`:

```rust
// @std/math
registry.register_package(ExternalPackage::new("@std/math"))?;
```

Functions:

```text
sin(x Float) -> Float
cos(x Float) -> Float
tan(x Float) -> Float
atan2(y Float, x Float) -> Float
log(x Float) -> Float
log2(x Float) -> Float
log10(x Float) -> Float
exp(x Float) -> Float
pow(base Float, exponent Float) -> Float
sqrt(x Float) -> Float
abs(x Float) -> Float
floor(x Float) -> Float
ceil(x Float) -> Float
round(x Float) -> Float
trunc(x Float) -> Float
min(a Float, b Float) -> Float
max(a Float, b Float) -> Float
clamp(x Float, min Float, max Float) -> Float
```

Constants:

```text
PI  Float
TAU Float
E   Float
```

Do not add these to the prelude. `io` is prelude because it is special. Math should require explicit imports.

### 6. Add a fully dynamic backend lowering path

`@std/math` should not add a large list of hardcoded `ExternalFunctionId::MathSin`, `MathCos`, etc. This is the right point to introduce the larger backend abstraction for dynamic external packages.

Current JS lowering maps `ExternalFunctionId` through `resolve_host_function_id(...)`, and unknown synthetic IDs fail backend lowering. That is fine for tests, but it is not good enough for real standard packages. :contentReference[oaicite:0]{index=0}

The external package registry should carry enough backend-facing metadata for each external function so HIR can keep using stable external IDs while backends can resolve those IDs dynamically.

Add backend-lowering metadata to external function definitions:

```rust
pub struct ExternalFunctionDef {
    pub name: &'static str,
    pub parameters: Vec<ExternalParameter>,
    pub return_type: ExternalAbiType,
    pub return_alias: ExternalReturnAlias,
    pub receiver_type: Option<ExternalAbiType>,
    pub receiver_access: ExternalAccessKind,

    pub lowerings: ExternalFunctionLowerings,
}

pub struct ExternalFunctionLowerings {
    pub js: Option<ExternalJsLowering>,
    pub wasm: Option<ExternalWasmLowering>,
}

pub enum ExternalJsLowering {
    RuntimeFunction(&'static str),
    InlineExpression(&'static str),
}
```

For `@std/math`, register JS lowerings like:

```rust
sin   -> ExternalJsLowering::RuntimeFunction("__bs_math_sin")
cos   -> ExternalJsLowering::RuntimeFunction("__bs_math_cos")
atan2 -> ExternalJsLowering::RuntimeFunction("__bs_math_atan2")
pow   -> ExternalJsLowering::RuntimeFunction("__bs_math_pow")
```

Then change JS backend lowering from:

```rust
resolve_host_function_id(id)
```

to:

```rust
external_package_registry
    .get_function_by_id(id)
    .and_then(|function| function.lowerings.js.as_ref())
```

This means:

- `ExternalFunctionId::Synthetic(_)` can become production-usable.
- Builder-provided external packages no longer need enum variants.
- Standard packages can be registered through normal metadata.
- Test packages and real packages use the same path.
- Backends remain responsible for deciding how each external symbol lowers.

The JS emitter will need access to the `ExternalPackageRegistry`, either directly in `JsEmitter` or through the backend compile input. This is a good abstraction boundary: HIR stores stable IDs, the backend resolves those IDs to backend-specific lowering metadata.

### 7. Add JS runtime helpers through package lowering metadata

The JS backend should emit or include runtime helpers based on the external functions actually referenced by HIR.

For `@std/math`, helper bodies can be simple wrappers around `Math`:

```js
function __bs_math_sin(x) { return Math.sin(x); }
function __bs_math_cos(x) { return Math.cos(x); }
function __bs_math_tan(x) { return Math.tan(x); }
function __bs_math_atan2(y, x) { return Math.atan2(y, x); }
function __bs_math_log(x) { return Math.log(x); }
function __bs_math_log2(x) { return Math.log2(x); }
function __bs_math_log10(x) { return Math.log10(x); }
function __bs_math_exp(x) { return Math.exp(x); }
function __bs_math_pow(base, exponent) { return Math.pow(base, exponent); }
function __bs_math_sqrt(x) { return Math.sqrt(x); }
function __bs_math_abs(x) { return Math.abs(x); }
function __bs_math_floor(x) { return Math.floor(x); }
function __bs_math_ceil(x) { return Math.ceil(x); }
function __bs_math_round(x) { return Math.round(x); }
function __bs_math_trunc(x) { return Math.trunc(x); }
function __bs_math_min(a, b) { return Math.min(a, b); }
function __bs_math_max(a, b) { return Math.max(a, b); }
function __bs_math_clamp(x, min, max) {
    return Math.min(Math.max(x, min), max);
}
```

Do not make these global unconditional helpers if avoidable. Track referenced external IDs during JS lowering and emit only the helpers required by the current module.

### 8. Backend abstraction acceptance criteria

The dynamic lowering path is complete when:

```beanstalk
import @std/math {PI, sin, pow}

# half_turn = PI

value = sin(half_turn)
squared = pow(value, 2.0)
```

works without adding math-specific `ExternalFunctionId` enum variants.

Required guarantees:

- HIR still stores `CallTarget::ExternalFunction(ExternalFunctionId)`.
- The backend resolves the ID through the external registry.
- Unknown external IDs produce structured backend errors, not panics.
- Synthetic/test external IDs and real package IDs use the same lowering path.
- JS runtime helper names are not recovered from source import names.
- Import aliases do not affect backend lowering.
- Wasm metadata can remain `None` for now, with a clean diagnostic if a backend tries to lower an unsupported external function.

### 9. Adjust the implementation order

Do this in this order:

1. Add `ExternalAbiType::F64`.
2. Add dynamic external JS lowering metadata.
3. Thread `ExternalPackageRegistry` into JS lowering.
4. Convert existing hardcoded JS external mapping to the new path.
5. Keep existing builtin helpers working: `io`, collections, errors.
6. Add external constants.
7. Register `@std/math`.
8. Add tests for both existing builtin externals and new math externals.

This avoids breaking the current prelude and collection/error helper path while replacing the brittle hardcoded backend mapping.

### 7. Add JS runtime helpers

Lower helpers to JavaScript `Math`:

```js
function __bs_math_sin(x) { return Math.sin(x); }
function __bs_math_cos(x) { return Math.cos(x); }
function __bs_math_atan2(y, x) { return Math.atan2(y, x); }
function __bs_math_pow(base, exponent) { return Math.pow(base, exponent); }
function __bs_math_clamp(x, min, max) {
  return Math.min(Math.max(x, min), max);
}
```

Notes:

* `log` should mean natural log, mapping to `Math.log`.
* `trunc` maps to `Math.trunc`.
* `round` maps to JS `Math.round`, but document that JS rounds `x.5` toward `+∞`, not necessarily banker’s rounding.
* `abs`, `min`, `max`, `clamp` are Float-only for now, even if they would be useful for `Int`.

### 8. Harden reachable-file discovery for virtual packages

Stage 0 already says virtual package imports are recognized during reachable-file discovery and skipped as filesystem paths, then validated later by AST import binding. 

Add tests for:

```beanstalk
import @std/math/sin
import @std/math {sin, cos, PI}
```

Expected:

* no filesystem lookup for `@std/math`
* missing symbol gives package diagnostic, not missing file diagnostic
* `@std/math/unknown` reports package found, symbol missing
* `@std/unknown/foo` reports missing import target
* grouped imports expand correctly for virtual packages

### 9. Add integration tests

Add canonical cases under `tests/cases/`:

```text
std_math_basic_functions
std_math_constants
std_math_grouped_import
std_math_alias_import
std_math_requires_import
std_math_missing_symbol
std_math_type_errors
std_math_external_constants_const_context
```

Minimum assertions:

* rendered HTML/JS output includes computed math results where deterministic
* `PI`, `TAU`, `E` can be imported and used in constants
* unimported `sin` is rejected
* importing `sin as wave` works and original `sin` is not visible through the alias
* importing `@std/math/NOPE` fails with a package-symbol diagnostic
* `sin("bad")` fails with a type diagnostic
* `sin(x = 1.0)` fails because external calls are positional-only for now

The codebase style guide says integration tests are the main regression check and new type-system syntax should include positive and negative diagnostics around import visibility and cross-file resolution. 

### 10. Update docs/progress together with implementation

Update `docs/src/docs/progress/#page.bst` when the work lands:

* change `Standard math package` from “Alpha testing ground” to `Supported` or `Partial`
* update `External platform packages` watch points:

  * external constants implemented for Rust-side packages
  * Float/F64 ABI implemented
  * JS lowering implemented for `@std/math`
  * Wasm still experimental / not Alpha-supported

Also add a small language/docs section for:

```beanstalk
import @std/math {PI, sin}

angle = PI / 2.0
value = sin(angle)
```

### Shortfalls to note before implementation

* **External constants do not exist yet.** This is the biggest real gap.
* **External ABI has no Float/F64 yet.** Math cannot be typed honestly without it.
* **JS lowering is still explicit ID mapping.** Either add stable math IDs now or design dynamic backend lowering, but dynamic lowering is larger than this task.
* **External calls are positional-only.** Keep this constraint for now.
* **No overloads.** Use Float-only functions. Do not try to support `Int` versions yet.
* **No Wasm requirement.** The progress matrix says Wasm external package behavior is experimental, so JS/HTML is the Alpha target.
* **`clamp(x, min, max)` uses parameter names that may shadow common words.** This is fine in user-facing docs, but Rust metadata does not expose parameter names yet, so diagnostics will still talk about positional parameters.