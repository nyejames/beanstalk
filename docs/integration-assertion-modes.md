# Integration Assertion Modes

The integration runner supports four assertion surfaces. Choose the one that most directly expresses what the fixture is actually testing.

---

## Decision table

| Mode | When to use | TOML key |
|---|---|---|
| **Strict golden** | Exact code shape is the contract (HTML structure, specific JS layout) | _(default — add a `golden/` directory)_ |
| **Normalized golden** | Semantics matter, but counter-name drift in backend output is noise | `golden_mode = "normalized"` |
| **Rendered output** | Runtime behavior is the contract — what the program actually does | `rendered_output_contains` |
| **Artifact assertions** | Targeted checks on produced files without a full golden snapshot | `artifact_assertions = [...]` |

---

## Strict golden (default)

Byte-for-byte comparison of every file in `golden/<backend>/`. Fails on any change, including whitespace and counter-name changes.

**Use when:** The fixture tests that a specific code structure is emitted — e.g., HTML element ordering, exact script tag placement, WASM export layout.

```toml
[backends.html]
mode = "success"
warnings = "forbid"
# golden/html/index.html contains the expected exact output
```

Failure label: `[strict golden mismatch]`

---

## Normalized golden

Same as strict golden, but compiler-generated counter suffixes in `bst_`-prefixed identifiers are canonicalized before comparison. Counter drift (e.g., `bst_rhs_and_fn0` becoming `bst_rhs_and_fn1` after an unrelated backend change) does not cause failures.

**Use when:** The fixture tests emitted code structure, but the exact counter assignments are not contractual. Common for fixtures that were previously brittle to HIR/backend refactoring.

```toml
[backends.html]
mode = "success"
warnings = "forbid"
golden_mode = "normalized"
# golden/html/index.html still required — comparison runs on normalized text
```

Identifiers normalized:

| Before | After |
|---|---|
| `bst_rhs_and_fn0` | `bst_rhs_and_fnN` |
| `bst_calls_l2` | `bst_calls_lN` |
| `bst___hir_tmp_3_l13` | `bst___hir_tmp_N_lN` |
| `bst___template_fn_1_fn4` | `bst___template_fn_N_fnN` |
| `bst___bst_frag_0_fn2` | `bst___bst_frag_N_fnN` |

Runtime library names (`__bs_*`) are **never** normalized — they are stable semantic contracts.

Failure label: `[normalized mismatch]`

---

## Rendered output

Executes the compiled `index.html` through a minimal Node.js harness. The harness stubs `document.getElementById` and captures `insertAdjacentHTML` calls and `console.log` output, then checks the combined result against the supplied fragment list.

**Use when:** Runtime behavior is the contract — e.g., short-circuit evaluation, fallback values, collection mutations. The emitted JS layout is noise; what matters is what the page renders.

**Requires:** `node` on PATH. If node is not found, the test fails with `[harness error]` and a clear message.

```toml
[backends.html]
mode = "success"
warnings = "forbid"
rendered_output_contains = ["my_fixture value=false calls=0"]
rendered_output_not_contains = ["error"]   # optional
# No golden/ directory needed
```

The assertion checks the combined text of:
- All `console.log` output lines
- All slot HTML strings inserted via `insertAdjacentHTML`

Failure label: `[rendered output mismatch]`

---

## Artifact assertions

Targeted checks on specific produced files — fragment presence, ordering, WASM imports/exports — without a full golden snapshot. Can be combined with a golden directory.

```toml
[backends.html]
mode = "success"
warnings = "forbid"

[[backends.html.artifact_assertions]]
path = "index.html"
kind = "html"
must_contain = ["__bs_result_fallback"]
must_contain_in_order = ["bst-slot-0", "bst-slot-1"]

[[backends.html.artifact_assertions]]
path = "index.html"
kind = "html"
normalized_contains = ["bst_rhs_and_fnN"]   # counter-normalized check
```

Available fields for `html`/`js` artifacts:
- `must_contain` — fragment must appear anywhere
- `must_not_contain` — fragment must not appear
- `must_contain_in_order` — fragments must appear in sequence
- `must_contain_exactly_once` — fragment must appear exactly once
- `normalized_contains` — fragment must appear after counter normalization
- `normalized_not_contains` — fragment must not appear after counter normalization

Available fields for `wasm` artifacts:
- `validate_wasm` — parse as valid WebAssembly
- `must_export` — required export names
- `must_import` — required import names as `module.item`

Failure label: `[expectation violation]`

---

## Failure kinds

Every test failure now carries a label that distinguishes the root cause:

| Label | Meaning |
|---|---|
| `[strict golden mismatch]` | Byte-for-byte golden comparison failed |
| `[normalized mismatch]` | Normalized golden comparison failed (semantic change detected) |
| `[rendered output mismatch]` | Executed output did not satisfy `rendered_output_contains` |
| `[harness error]` | Infrastructure failure (panic, node not found, I/O error) |
| `[expectation violation]` | Artifact assertion, warning count, or error type mismatch |

---

## Choosing the right mode

```
Is the exact emitted code structure the contract?
├── Yes → Strict golden
│   └── Does counter-name drift cause false failures?
│       └── Yes → Normalized golden
└── No → Is the runtime behavior the contract?
    ├── Yes → Rendered output
    └── No → Artifact assertions (targeted fragment checks)
```

When in doubt, prefer rendered output for runtime-semantic fixtures. It gives the clearest signal and the least noise during backend refactoring.
