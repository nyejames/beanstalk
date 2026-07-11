# HTML Project Builder Backend Strategy and Wasm Lowering Final Implementation Plan

**Scope:** HTML project builder mixed JS/Wasm backend strategy, final Wasm LIR/runtime/ABI design, and implementation migration plan  

---

## 0. Executive summary

The final backend shape is:

```text
Frontend / HIR / borrow facts
  -> compiled project module graph
  -> HTML artifact link plan
  -> deterministic JS/Wasm function partition
  -> JS companions + page entry JS
  -> per-module Wasm modules
  -> one generated page-local Wasm runtime instance
  -> route HTML + runtime/static assets
```

The main architectural change is that the current experimental HTML-Wasm path is replaced with a mixed backend where:

- `start` is always lowered to JS.
- DOM/browser/project-JS work is JS-owned.
- Console-style IO is neutral.
- Remaining Wasm-supported functions are Wasm-owned by default.
- JS companions are the stable generated module facade.
- Wasm modules share page-local runtime memory and import runtime helpers from a generated runtime module.
- Wasm LIR is redesigned as structured, emission-shaped IR with typed builders.
- Flat basic-block LIR, dispatcher-loop emission, per-module memory, helper duplication, and `i64` bridge paths are removed.

This is a **final-shape refactor**, not a compatibility migration. Old APIs, scaffolding, and parallel paths should be deleted as soon as their replacement is wired.

---

## 1. Current repo snapshot and anchor points

This section records the current shape to keep agents anchored while implementing the plan.

### 1.1 Build handoff

Current file: `src/build_system/build.rs`

Current facts:

- `BackendBuilder::build_backend` receives `modules: Vec<Module>`, not a graph.
- `Module` currently carries:
  - `entry_point: PathBuf`
  - `hir: HirModule`
  - `type_environment: TypeEnvironment`
  - `borrow_analysis: BorrowCheckReport`
  - `warnings`
  - `const_top_level_fragments`
  - `entry_runtime_fragment_count`
  - `external_package_registry`
  - `module_external_imports`
- `Project` carries output files, entry page, cleanup policy, and warnings.
- `FileKind` already supports `Wasm(Vec<u8>)`, `Js(String)`, `Html(String)`, and raw bytes.

Implication:

- The first major structural change is to add a graph payload around `Vec<Module>` or replace the backend handoff with a graph-aware payload.
- Do not make the HTML builder rediscover module dependencies from source paths.

### 1.2 Frontend module compilation

Current file: `src/build_system/create_project_modules/frontend_orchestration.rs`

Current facts:

- One module runs through file preparation, dependency sort, AST, HIR, borrow checking.
- Reachability from `start` is currently used to collect reachable external package IDs for backend metadata.
- Module-level external imports are filtered using entry-start reachability.

Implication:

- Current reachability filtering is page/start-centric.
- The mixed backend needs a module-graph and partition-aware runtime dependency model, not only start reachability.
- Facade-only modules and reusable modules need runtime ABI surface planning even when they have no page `start`.

### 1.3 HTML builder

Current file: `src/projects/html_project/html_project_builder.rs`

Current facts:

- `HtmlProjectBuilder::build_backend` loops over `modules` and compiles one module at a time.
- Backend mode is selected through `wasm_enabled = flags.contains(&Flag::HtmlWasm)`.
- `compile_one_module` validates the module for either JS or Wasm as a whole.
- Wasm mode validates backend features using explicit roots from the current Wasm export plan.
- JS mode validates from `start`.

Implication:

- The current global per-module JS-vs-Wasm mode must be replaced by function-level partitioning.
- Validation should be partition-aware, not whole-module target-aware.
- The builder loop should become graph/link-plan-driven.

### 1.4 JS-only HTML path

Current file: `src/projects/html_project/js_path.rs`

Current facts:

- JS-only rendering lowers HIR to JS and embeds the JS bundle into route HTML.
- Runtime fragments are hydrated by inline bootstrap code that calls `start()`.
- `render_entry_fragments` already owns const/runtime fragment interleaving and slot generation.
- Existing JS path has module-script support when external JS glue requires ES module imports.

Implication:

- Fragment interleaving and JS `start` hydration should be preserved and generalized.
- The final mixed backend should reuse or move `render_entry_fragments` into a shared page-entry module.
- Inline classic-script generation should be replaced or isolated once generated ES modules become the mixed backend baseline.

### 1.5 HTML-Wasm integration path

Current files:

- `src/projects/html_project/wasm/mod.rs`
- `src/projects/html_project/wasm/artifacts.rs`
- `src/projects/html_project/wasm/export_plan.rs`
- `src/projects/html_project/wasm/js_bootstrap.rs`
- `src/projects/html_project/wasm/request.rs`

Current facts:

- The module owns builder-side Wasm planning, request wiring, and bootstrap generation.
- Current Wasm mode exports `start` as `bst_start`.
- Current bootstrap instantiates `page.wasm`, calls `bst_start`, reads returned Vec/String handles, and hydrates slots.
- Helper export structs are duplicated between HTML-Wasm and core Wasm request types.

Implication:

- The final design removes Wasm-owned `start` and `bst_start` as page contract.
- HTML-Wasm bootstrap should become page-entry JS + module companion initialization.
- Helper export booleans should be replaced by a runtime capability/helper import plan.

### 1.6 JS backend

Current file: `src/backends/js/mod.rs`

Current facts:

- JS backend is GC-baseline lowering.
- It has three lowering configs: direct JS, HTML page bundle, HTML-Wasm companion.
- Function emission policy is currently all-functions or reachable-from-start.
- The HTML-Wasm companion config disables generated ES module export glue and expects Wasm validation to reject unsupported external calls.

Implication:

- JS backend needs a new partition-selected emission mode, not just reachable-from-start.
- JS backend should lower explicitly selected JS-owned functions plus JS wrappers/import preamble selected by the artifact link plan.

### 1.7 Wasm backend module map

Current file: `src/backends/wasm/mod.rs`

Current modules:

```text
backend
_debug
emit
hir_to_lir
lir
request
result
runtime
```

Actual module names:

```rust
pub(crate) mod backend;
pub(crate) mod debug;
pub(crate) mod emit;
pub(crate) mod hir_to_lir;
pub(crate) mod lir;
pub(crate) mod request;
pub(crate) mod result;
pub(crate) mod runtime;
```

Current facts:

- Backend is explicitly experimental.
- It currently owns HIR -> LIR lowering, optional core Wasm emission, runtime helper contracts, and debug output.

Implication:

- This module boundary remains valid, but most internals should be redesigned.

### 1.8 Wasm backend request

Current file: `src/backends/wasm/request.rs`

Current facts:

- `WasmBackendRequest` contains:
  - export policy
  - feature flags
  - emit options
  - debug flags
  - external package registry
  - function emission policy
- `WasmFunctionEmissionPolicy` is currently `AllFunctions` or `ReachableFromExports`.
- `WasmCfgLoweringStrategy` exposes `DispatcherLoop` and reserved `Structured`.
- Current validator rejects GC, multi-value, reference types, and structured CFG.

Implication:

- Final request should accept explicit selected Wasm functions and imports from the partition/link plan.
- Remove `AllFunctions`/`ReachableFromExports` from final module path.
- Remove CFG strategy enum and dispatcher option.
- Multi-value must become normal final ABI, not rejected.

### 1.9 Current Wasm LIR

Current files:

- `src/backends/wasm/lir/module.rs`
- `src/backends/wasm/lir/function.rs`
- `src/backends/wasm/lir/instructions.rs`
- `src/backends/wasm/lir/types.rs`
- `src/backends/wasm/lir/linkage.rs`

Current facts:

- `WasmLirModule` contains functions, imports, exports, static data, memory plan.
- `WasmLirFunction` contains a flat `Vec<WasmLirBlock>`.
- `WasmLirBlock` contains statements and a terminator.
- Terminators are `Jump`, `Branch`, `Return`, `Trap`.
- ABI includes `I64`, `Handle`, and `Void`.
- Instructions include bridge-like `StringFromI64`.

Implication:

- Replace flat block LIR with structured body-tree LIR.
- Remove `Jump`/`Branch` terminators from final LIR.
- Remove `I64` as Beanstalk `Int` path.
- Remove `Void` as a real ABI type; represent no result as `results: []`.
- Remove `StringFromI64`; replace with `StringFromI32` / generic numeric format helpers.

### 1.10 Current Wasm emission

Current files:

- `src/backends/wasm/emit/module.rs`
- `src/backends/wasm/emit/sections.rs`
- `src/backends/wasm/emit/functions.rs`

Current facts:

- Emission builds a section/index/data plan before writing sections.
- The current emitter defines memory inside each emitted module.
- Runtime helper functions are synthesized separately during code-section emission.
- User functions are emitted through a dispatcher-loop strategy.
- The dispatcher uses an artificial local as a program counter.

Implication:

- Keep ephemeral section/index/data planning.
- Replace per-module memory section with imported memory from the generated runtime module.
- Generate runtime helper functions through structured Wasm LIR like user functions.
- Delete dispatcher-loop code.

### 1.11 Current Wasm runtime

Current files:

- `src/backends/wasm/runtime/mod.rs`
- `src/backends/wasm/runtime/imports.rs`
- `src/backends/wasm/runtime/memory.rs`
- `src/backends/wasm/runtime/strings.rs`

Current facts:

- Runtime module defines host imports, memory constants, and runtime string contracts.
- `WasmHostFunction` is currently an empty enum.

Implication:

- Host imports need metadata-driven target affinity and real neutral host entries.
- Runtime helper definition should move to a runtime capability registry with signatures, import names, and implementation builders.

### 1.12 Cargo/tooling

Current file: `Cargo.toml`

Current facts:

- Edition is 2024.
- `wasm-encoder = "0.247.0"`.
- `wasmparser = "0.247.0"`.
- Release optimizer libraries are not currently committed; `oxc_minifier` is a commented future note.

Current file: `justfile`

Current facts:

- `just validate` runs clippy, unit tests, integration tests, docs check, and benchmark check.

Implication:

- This plan should not require adding an optimizer dependency initially.
- Optional post-emission optimizer integration should be a later phase and correctness-independent.
- Every phase ends with `just validate` unless the phase is documentation-only and no code was changed; even then, docs check should run.

---

## 2. Complexity reduction and consolidation opportunities

These are not optional polish. They should guide the implementation.

### 2.1 Replace whole-module JS/Wasm branching with artifact planning

Current pattern:

```text
if wasm_enabled:
    compile_html_module_wasm(...)
else:
    compile_html_module_js(...)
```

Final pattern:

```text
build HtmlArtifactLinkPlan
for each page/module artifact:
    emit selected JS functions
    emit selected Wasm functions
    emit companions
    emit runtime/static assets
```

Benefit:

- Removes mode-specific duplicate logic.
- Avoids divergent JS-only vs Wasm-only page lifecycle paths.
- Makes mixed output natural.

### 2.2 Collapse duplicate helper export policy structs

Current duplication:

- `HtmlWasmHelperExports`
- `WasmHelperExportPolicy`
- backend helper export names
- bootstrap assumptions about helper names

Final consolidation:

```rust
pub struct RuntimeCapabilityPlan {
    pub helper_families: FxHashSet<RuntimeHelperFamily>,
    pub host_imports: FxHashSet<HtmlHostImport>,
    pub memory_required: bool,
    pub layout_metadata_required: bool,
}
```

Benefit:

- One source of truth for helper requirements.
- No manual boolean synchronization.
- Easier to test.

### 2.3 Replace `AllFunctions` / `ReachableFromExports` with explicit function sets

Final `WasmBackendRequest` should use:

```rust
pub struct WasmBackendRequest {
    pub module_id: ModuleId,
    pub selected_functions: Vec<WasmSelectedFunction>,
    pub imports: WasmImportPlan,
    pub exports: WasmExportPlan,
    pub runtime: RuntimeImportPlan,
    pub layout_ids: Arc<ProjectLayoutRegistry>,
    pub emit_options: WasmEmitOptions,
    pub debug_flags: WasmDebugFlags,
}
```

Benefit:

- Function selection belongs to HTML builder partitioning.
- Wasm backend no longer performs reachability policy.
- Generic direct-Wasm tests can still construct an explicit all-functions request.

### 2.4 Consolidate HTML document/page JS rendering

Move shared page concerns into a new module:

```text
src/projects/html_project/page_entry/
  mod.rs
  fragments.rs
  document.rs
  entry_script.rs
```

Move or reuse:

- `render_entry_fragments`
- document shell rendering calls
- page metadata extraction
- runtime slot hydration planning

Benefit:

- JS-only and mixed backend do not duplicate page shell logic.
- Future MPA/SPAs can share primitives without sharing runtime semantics.

### 2.5 Replace raw JS string concatenation with a small writer

Do not introduce a large JS AST. Use a small writer:

```rust
pub(crate) struct JsModuleWriter {
    output: String,
    indent: usize,
}
```

Use it for:

- companions;
- runtime JS;
- page entry JS;
- import/export boilerplate.

Benefit:

- Less indentation noise.
- Better generated-code tests.
- Avoids giant string-push functions.

### 2.6 Make runtime helpers data-driven

Final helper registry:

```rust
pub enum RuntimeHelperId { Alloc, Release, StringNew, VecPushI32, MapGetString, ... }

pub struct RuntimeHelperDef {
    pub id: RuntimeHelperId,
    pub import_name: &'static str,
    pub signature: WasmLirSignature,
    pub family: RuntimeHelperFamily,
}
```

Benefit:

- One signature source.
- User modules import helpers from runtime module.
- Runtime module builds helper bodies from same definitions.

### 2.7 Remove compatibility wrappers aggressively

Per style guide direction, do not preserve old API shapes when moving code.

Delete after replacement:

- old HTML-Wasm `bst_start` export path;
- dispatcher-loop emitter;
- flat LIR block terminators;
- helper export booleans;
- `StringFromI64`;
- `I64` Int bridge;
- per-module memory section path;
- separate raw helper code emission.

---

## 3. Standard phase gate

Every implementation phase ends with this gate.

```text
Phase Gate
  1. cargo fmt
  2. just validate
  3. manual style-guide audit
  4. stale-path cleanup audit
  5. docs/progress/roadmap audit if behavior changed
```

Checklist for every phase:

- [ ] `cargo fmt`
- [ ] `just validate`
- [ ] Review touched modules for file-level docs with WHAT/WHY where new modules are added.
- [ ] Check diagnostics: user-facing errors use `CompilerDiagnostic`; backend/internal invariant failures use `CompilerError`.
- [ ] Check no stale compatibility wrapper remains unless explicitly marked as temporary in this plan.
- [ ] Check no raw source re-resolution was added to backend/project builder.
- [ ] Check generated artifacts have stable ordering.
- [ ] Add or update tests owned by the phase.
- [ ] Update docs/progress/roadmap if user-visible behavior, backend support, or TODO state changed.

---

## 4. Implementation phases

Each phase is sized so a coding agent can complete it within one context window.

---

### Phase 0 — Add the design document and repo anchor documentation

#### Context

Before code changes, commit the final design and implementation plan as source-of-truth docs. This prevents agents from relying on stale interview context.

#### Primary files

- `docs/roadmap/plans/html_project_backend_wasm_final_design_plan.md` — new
- `docs/roadmap/roadmap.md`
- `docs/compiler-design-overview.md`
- `docs/language-overview.md`
- `docs/src/docs/progress/#page.bst`

#### Checklist

- [ ] Add the final design document with ownership boundaries, ABI rules, partition rules, runtime model, and implementation phases.
- [ ] Update `docs/roadmap/roadmap.md` to link this plan under active plans.
- [ ] Move the old Wasm follow-up bullet into a reference to this plan.
- [ ] Add a short `compiler-design-overview.md` note under backend lowering describing mixed HTML artifact planning and core Wasm backend ownership.
- [ ] Add a short `language-overview.md` note that HTML builder target selection is backend policy, not language semantics.
- [ ] Update progress matrix with final-plan status labels.
- [ ] Add a repo snapshot section to the plan listing current relevant files and replacement targets.

#### Tests / validation

- [ ] `cargo run --quiet -- check docs`
- [ ] Standard phase gate.

#### Exit criteria

- [ ] Documentation explains that this is a final-shape refactor.
- [ ] Roadmap no longer has disconnected Wasm TODOs that are covered by this plan.

---

### Phase 1 — Introduce graph-aware backend handoff scaffolding

#### Context

Current `BackendBuilder::build_backend` receives `Vec<Module>`. The mixed backend needs stable module IDs, source dependency edges, runtime-relevant edges, and topological order. This phase adds graph payloads without changing backend behavior.

#### Primary files

- `src/build_system/build.rs`
- `src/build_system/create_project_modules/`
- `src/projects/html_project/html_project_builder.rs`
- `src/build_system/tests/` or relevant build-system test module

#### New/changed types

```rust
pub struct CompiledProjectModuleGraph {
    pub modules: Vec<GraphModule>,
    pub dependency_edges: Vec<ModuleDependencyEdge>,
    pub topological_order: Vec<ModuleId>,
}

pub struct GraphModule {
    pub id: ModuleId,
    pub module: Module,
    pub kind: ModuleEntryKind,
}

pub enum ModuleEntryKind {
    Page,
    FacadeOnly,
    OtherBuilderEntry,
}

pub struct ModuleDependencyEdge {
    pub from: ModuleId,
    pub to: ModuleId,
    pub runtime_relevant: bool,
    pub reason: ModuleDependencyReason,
}
```

#### Checklist

- [ ] Add stable `ModuleId` newtype.
- [ ] Add graph payload types in `build.rs` or a new `build_system/module_graph.rs`.
- [ ] Extend project frontend compilation to produce a graph payload.
- [ ] Preserve the old `Vec<Module>` behavior temporarily only at the outer boundary if needed.
- [ ] Mark transitional adapters as temporary with a deletion phase reference.
- [ ] Record module entry kind: page, facade-only, future builder entry.
- [ ] Add deterministic ordering by existing module order/topological input order.
- [ ] Add unit tests for graph construction and deterministic IDs.
- [ ] Ensure compile-time-only edges are representable even if initially all edges are conservative.

#### Tests / validation

- [ ] Build-system unit tests for stable IDs and graph order.
- [ ] Existing HTML JS-only integration tests still pass.
- [ ] Standard phase gate.

#### Exit criteria

- [ ] HTML builder can receive a graph payload without re-resolving source imports.
- [ ] Existing output behavior is unchanged.

---

### Phase 2 — Add HTML artifact link plan types

#### Context

The HTML builder needs a target-specific projection of the compiled module graph. This phase adds planning structures only. It should not yet change emitted artifacts.

#### Primary files

- `src/projects/html_project/mod.rs`
- `src/projects/html_project/html_project_builder.rs`
- `src/projects/html_project/artifact_plan.rs` — new
- `src/projects/html_project/output_plan.rs`
- `src/projects/html_project/tests/`

#### New types

```rust
pub struct HtmlArtifactLinkPlan {
    pub runtime: HtmlRuntimeArtifactPlan,
    pub modules: Vec<HtmlModuleArtifactPlan>,
    pub pages: Vec<HtmlPageArtifactPlan>,
}

pub struct HtmlModuleArtifactPlan {
    pub module_id: ModuleId,
    pub companion_path: PathBuf,
    pub wasm_path: Option<PathBuf>,
    pub runtime_exports: Vec<HtmlRuntimeExport>,
    pub dependencies: Vec<HtmlModuleDependency>,
}

pub struct HtmlPageArtifactPlan {
    pub module_id: ModuleId,
    pub html_path: PathBuf,
    pub page_js_path: PathBuf,
    pub dependency_modules: Vec<ModuleId>,
}
```

#### Checklist

- [ ] Add plan module with file-level doc.
- [ ] Derive output paths from existing route/output policy; do not duplicate route semantics.
- [ ] Add module companion paths under `modules/`.
- [ ] Add runtime artifact paths under `runtime/`.
- [ ] Add page JS paths beside route HTML.
- [ ] Add deterministic sorting for module/page artifact lists.
- [ ] Keep existing JS-only output path behavior unchanged.
- [ ] Add tests for path derivation and stable ordering.

#### Tests / validation

- [ ] Unit tests for artifact plan from simple graph.
- [ ] Integration smoke test still emits same JS-only HTML.
- [ ] Standard phase gate.

#### Exit criteria

- [ ] Artifact planning exists as data but does not force mixed output yet.

---

### Phase 3 — Add external target-affinity metadata

#### Context

Partitioning must be metadata-driven, not package-name-driven. This phase adds target-affinity metadata and documents neutral IO.

#### Primary files

- `src/compiler_frontend/external_packages/`
- `src/backends/external_package_validation.rs`
- `src/projects/html_project/external_libraries/`
- `src/projects/html_project/external_js/`
- `docs/language-overview.md`
- `docs/src/docs/libraries/core/#page.bst` or relevant core docs

#### New type

```rust
pub enum HtmlTargetAffinity {
    NeutralHost,
    JsRequired,
    WasmNative,
    TargetSpecific,
}
```

#### Checklist

- [ ] Add affinity metadata to external function definitions or lowering metadata.
- [ ] Mark console-style IO functions as `NeutralHost`.
- [ ] Mark DOM/canvas/input/browser APIs as `JsRequired`.
- [ ] Mark project `.js` imports as `JsRequired`.
- [ ] Mark core math/text/random functions as `WasmNative` where planned.
- [ ] Update external package validation to expose affinity facts to HTML partitioning.
- [ ] Avoid name-based target checks.
- [ ] Add user-facing docs explaining neutral IO and JS-required APIs.
- [ ] Add compiler-design docs for target-affinity metadata.

#### Tests / validation

- [ ] Unit tests for affinity lookup.
- [ ] Diagnostic tests for unsupported target package calls.
- [ ] Standard phase gate.

#### Exit criteria

- [ ] HTML partitioning can query target affinity from metadata.
- [ ] Neutral IO is documented for compiler developers and users.

---

### Phase 4 — Implement deterministic JS/Wasm partition planner

#### Context

This is the core target split. It must be deterministic, build-mode independent, and inspectable.

#### Primary files

- `src/projects/html_project/partition.rs` — new
- `src/projects/html_project/html_project_builder.rs`
- `src/compiler_frontend/hir/reachability.rs`
- `src/backends/backend_feature_validation.rs`
- tests under `src/projects/html_project/tests/` or `src/compiler_tests/`

#### New types

```rust
pub struct HtmlBackendPartitionPlan {
    pub module_partitions: Vec<HtmlModulePartition>,
    pub crossings: Vec<HtmlPartitionCrossing>,
    pub diagnostics: HtmlPartitionReport,
}

pub enum FunctionTarget {
    Js,
    Wasm,
}

pub enum FunctionTargetReason {
    EntryStart,
    DirectJsRequiredCall,
    CallsJsOwnedFunction,
    WasmSupported,
    NeutralHostOnly,
}
```

#### Checklist

- [ ] Mark each module `start` function as JS.
- [ ] Mark functions with direct JS-required external calls as JS.
- [ ] Do not mark console IO as JS.
- [ ] Propagate JS ownership backward through call graph.
- [ ] Assign remaining Wasm-supported functions to Wasm.
- [ ] Reject or mark unsupported Wasm operations through structured diagnostics before lowering.
- [ ] Record every decision reason.
- [ ] Record every JS -> Wasm crossing.
- [ ] Assert no Wasm -> JS-owned Beanstalk call remains after propagation.
- [ ] Add partition report generation.
- [ ] Ensure partitioning result does not depend on release/debug flags.

#### Tests / validation

- [ ] Unit tests for propagation.
- [ ] Fixture where `io.line` does not flip target.
- [ ] Fixture where DOM/canvas/project JS flips containing function and callers.
- [ ] Fixture where JS caller calling pure callee leaves callee Wasm.
- [ ] Snapshot-style test for partition report.
- [ ] Standard phase gate.

#### Exit criteria

- [ ] HTML builder can produce a deterministic partition plan.
- [ ] Partition plan is debug-printable and testable.

---

### Phase 5 — Convert JS backend to selected-function emission

#### Context

Current JS backend emits all functions or start-reachable functions. Mixed output needs selected JS-owned functions plus generated wrappers for Wasm-owned callees.

#### Primary files

- `src/backends/js/mod.rs`
- `src/backends/js/emitter.rs`
- `src/backends/js/reachability.rs`
- `src/projects/html_project/partition.rs`
- JS backend tests

#### Checklist

- [ ] Add `JsFunctionEmissionPolicy::SelectedFunctions(Vec<FunctionId>)` or a named selected-function plan.
- [ ] Thread selected function IDs through JS lowering.
- [ ] Ensure JS emitter does not emit Wasm-owned Beanstalk function bodies.
- [ ] Add placeholder/import hook for generated Wasm wrapper calls.
- [ ] Keep direct JS and JS-only page bundle paths working during transition.
- [ ] Replace HTML-Wasm companion config with selected-function config when possible.
- [ ] Add tests that selected JS emission omits Wasm-owned function bodies.
- [ ] Keep `function_name_by_id` available for emitted JS functions and wrappers.

#### Tests / validation

- [ ] JS backend unit tests for selected emission.
- [ ] Existing JS-only integration tests.
- [ ] Standard phase gate.

#### Exit criteria

- [ ] JS backend can emit only JS-owned functions selected by HTML partitioning.

---

### Phase 6 — Add ES module companion generator

#### Context

Every module needs a JS companion as the stable generated facade. This phase can generate companions that wrap existing JS functions first, then later add Wasm wrappers.

#### Primary files

- `src/projects/html_project/companions.rs` — new
- `src/projects/html_project/js_writer.rs` — new small writer
- `src/projects/html_project/artifact_plan.rs`
- `src/projects/html_project/html_project_builder.rs`

#### Checklist

- [ ] Add `JsModuleWriter` with indentation helpers.
- [ ] Generate one companion ES module per Beanstalk module.
- [ ] Expose explicit `init_<module>()` initializer.
- [ ] Cache initialized namespace per page load.
- [ ] Export JS-facing facade functions from the companion namespace.
- [ ] Route JS-owned exported functions through JS-lowered implementations.
- [ ] Reserve Wasm-owned wrapper generation hooks.
- [ ] Do not expose compile-time-only declarations.
- [ ] Add golden tests for companion output shape.

#### Tests / validation

- [ ] Unit tests for JS writer formatting.
- [ ] Golden/snapshot tests for companion generation.
- [ ] Standard phase gate.

#### Exit criteria

- [ ] Generated companions exist and are testable independent of Wasm.

---

### Phase 7 — Add page entry ES module generation with JS-owned start

#### Context

Page entry JS initializes runtime and companions, then calls JS-lowered `start`. This replaces inline bootstrap logic and removes the final design need for Wasm-owned start.

#### Primary files

- `src/projects/html_project/page_entry/` — new
- `src/projects/html_project/js_path.rs`
- `src/projects/html_project/wasm/js_bootstrap.rs` — transitional removal target
- `src/projects/html_project/document_shell.rs`

#### Checklist

- [ ] Move `render_entry_fragments` into `page_entry/fragments.rs`.
- [ ] Add `page_entry/script.rs` to generate `page.js` ES module.
- [ ] Page JS initializes runtime and module companions in dependency order.
- [ ] Page JS calls JS-lowered `start`.
- [ ] Page JS hydrates runtime fragments in source order.
- [ ] Preserve reactive runtime fragment mounting behavior for JS-owned start.
- [ ] Emit `<script type="module" src="./page.js"></script>` instead of inline mixed bootstrap for mixed mode.
- [ ] Keep JS-only inline path only if still needed as transitional legacy path.
- [ ] Add tests for generated `page.js` and HTML script tag.

#### Tests / validation

- [ ] Existing page-fragment ordering tests.
- [ ] New page entry JS snapshot tests.
- [ ] Integration test: simple `#page.bst` with runtime fragments.
- [ ] Standard phase gate.

#### Exit criteria

- [ ] `start` is JS-owned in the mixed pipeline.
- [ ] No new code assumes `bst_start`.

---

### Phase 8 — Add generated runtime artifact skeleton

#### Context

Runtime helper files are project-level static artifacts, but runtime instance/memory is page-local. This phase emits skeleton runtime JS/Wasm and wires page initialization without moving all helpers yet.

#### Primary files

- `src/projects/html_project/runtime_artifacts.rs` — new
- `src/backends/wasm/runtime/`
- `src/projects/html_project/html_project_builder.rs`
- `src/projects/html_project/artifact_plan.rs`

#### Checklist

- [ ] Add runtime artifact plan paths: `runtime/bst_runtime.js`, `runtime/bst_runtime.wasm`.
- [ ] Generate runtime JS initializer.
- [ ] Generate minimal runtime Wasm module or placeholder through Wasm backend path.
- [ ] Runtime JS creates/caches page-local runtime object.
- [ ] Runtime object owns/exports shared `WebAssembly.Memory` for page graph.
- [ ] Page entry JS imports and initializes runtime.
- [ ] Emit runtime artifacts once per project output.
- [ ] Avoid sharing runtime memory across pages.
- [ ] Add tests proving artifacts are emitted once even with multiple pages.

#### Tests / validation

- [ ] Multi-page artifact emission test.
- [ ] Output path conflict tests.
- [ ] Standard phase gate.

#### Exit criteria

- [ ] Runtime artifact skeleton exists and is page-local at execution time.

---

### Phase 9 — Redesign Wasm ABI type model

#### Context

The final ABI removes `i64` for `Int`, removes `Void` as a value type, and standardizes handles/options/errors.

#### Primary files

- `src/backends/wasm/lir/types.rs`
- `src/backends/wasm/abi.rs` — new
- `src/backends/wasm/hir_to_lir/`
- `src/backends/wasm/emit/types.rs`
- tests

#### Checklist

- [ ] Add `WasmAbiType` final shape: `I32`, `F64`, `Handle` plus any strictly needed internal types.
- [ ] Remove `I64` from Beanstalk language lowering.
- [ ] Remove `Void`; use empty results.
- [ ] Add `WasmResultAbi` helpers for normal/multi/fallible returns.
- [ ] Add `WasmOptionAbi` helpers for niche option representations.
- [ ] Add tests for ABI mapping: `Int`, `Bool`, `Char`, `Float`, handles, `Error!`, `T?`.
- [ ] Add internal errors if HIR-to-Wasm tries to use `i64` for Beanstalk `Int`.
- [ ] Leave actual helper implementations for later phases.

#### Tests / validation

- [ ] ABI unit tests.
- [ ] Compile smoke tests for simple scalar functions.
- [ ] Standard phase gate.

#### Exit criteria

- [ ] Final ABI model is represented in types.
- [ ] `i64` bridge use is blocked or removed from new code.

---

### Phase 10 — Add project-wide layout registry

#### Context

All modules in one page runtime graph share memory, so layout IDs must be project-wide. This phase builds metadata without fully using it for object allocation yet.

#### Primary files

- `src/projects/html_project/layout_plan.rs` — new
- `src/backends/wasm/layout.rs` — new
- `src/compiler_frontend/datatypes/environment/` consumers
- `src/projects/html_project/artifact_plan.rs`

#### Checklist

- [ ] Add `ProjectLayoutRegistry` with deterministic layout IDs.
- [ ] Assign layout IDs for structs, choices, collections, maps, errors, strings/runtime objects.
- [ ] Record layout metadata: size, alignment, field offsets, ABI types, scan/drop flags.
- [ ] Use canonical `TypeEnvironment` definitions.
- [ ] Use aligned field layout: `i32` for `Bool`/`Int`/`Char`/handles, 8-byte `f64`.
- [ ] Add fixed collection inline threshold config key: `wasm_inline_fixed_collection_max_bytes`.
- [ ] Validate default `4096`, positive `Int`, hard cap `1_048_576`.
- [ ] Mark config changes as ABI-affecting.
- [ ] Add layout snapshot tests.

#### Tests / validation

- [ ] Struct layout tests.
- [ ] Choice layout tests.
- [ ] Fixed/growable collection layout tests.
- [ ] Config validation tests.
- [ ] Standard phase gate.

#### Exit criteria

- [ ] Layout IDs are project-wide and deterministic.
- [ ] Config key is registered and documented.

---

### Phase 11 — Add backend-neutral structured HIR view

#### Context

The final Wasm backend must not recover structure from arbitrary CFG. It needs a backend-neutral structured HIR view.

#### Primary files

- `src/compiler_frontend/hir/structured_view.rs` — new
- `src/compiler_frontend/hir/mod.rs`
- `src/compiler_frontend/hir/validation.rs`
- HIR tests

#### Checklist

- [ ] Add structured view types for `Block`, `If`, `Loop`, `Break`, `Continue`, `Return`, and effect statements.
- [ ] Preserve match/catch/value-producing block structure where needed.
- [ ] Keep view backend-neutral; no Wasm terms in type names.
- [ ] Provide builder/query API from existing HIR.
- [ ] Fail with internal compiler error if selected Wasm function lacks a valid structured view.
- [ ] Do not modify JS backend to depend on this view.
- [ ] Add tests for if/loop/break/continue/value-producing control flow.
- [ ] Add tests for invalid/unrepresentable structured views as internal errors.

#### Tests / validation

- [ ] HIR structured view unit tests.
- [ ] Existing HIR validation tests.
- [ ] Standard phase gate.

#### Exit criteria

- [ ] Wasm backend can request a structured view for selected functions.
- [ ] HIR remains backend-neutral.

---

### Phase 12 — Replace Wasm LIR with structured builder-owned LIR

#### Context

This is the core Wasm backend redesign. Do not preserve old flat block/terminator LIR.

#### Primary files

- `src/backends/wasm/lir/`
- `src/backends/wasm/lir/builder.rs` — new
- `src/backends/wasm/lir/validation.rs` — new Alpha guard
- `src/backends/wasm/hir_to_lir/`

#### New final shape

```rust
pub struct WasmLirFunction {
    pub id: WasmLirFunctionId,
    pub debug_name: Option<String>,
    pub origin: WasmLirFunctionOrigin,
    pub signature: WasmLirSignature,
    pub locals: WasmLocalSet,
    pub body: WasmBody,
    pub linkage: WasmFunctionLinkage,
}

pub struct WasmBody {
    pub items: Vec<WasmItem>,
}

pub enum WasmItem {
    Let(WasmLet),
    Store(WasmStore),
    Call(WasmCall),
    CallRuntime(WasmRuntimeCall),
    If(WasmIf),
    Loop(WasmLoop),
    Block(WasmBlock),
    Break(WasmBreak),
    Continue(WasmContinue),
    Return(WasmReturn),
    Trap(WasmTrap),
}
```

#### Checklist

- [ ] Replace flat `WasmLirBlock` storage with structured body tree.
- [ ] Split pure `WasmValue` from effectful `WasmItem`.
- [ ] Add `WasmLirFunctionBuilder`, `WasmBodyBuilder`, `WasmLocalAllocator`.
- [ ] Hide raw mutable vectors behind builders where practical.
- [ ] Add Alpha LIR validation as migration guard.
- [ ] Delete or quarantine old `Jump`/`Branch` terminators.
- [ ] Remove dispatcher-specific assumptions from LIR.
- [ ] Add tests for structured LIR construction.
- [ ] Add tests that invalid old shapes cannot be constructed or fail validation.

#### Tests / validation

- [ ] LIR builder unit tests.
- [ ] LIR validation tests.
- [ ] Standard phase gate.

#### Exit criteria

- [ ] New structured LIR compiles.
- [ ] Old flat block API is not used by new lowering.

---

### Phase 13 — Rebuild HIR-to-Wasm lowering over structured LIR

#### Context

The lowerer should preserve HIR structure and build Wasm-shaped LIR directly.

#### Primary files

- `src/backends/wasm/hir_to_lir/`
- `src/backends/wasm/backend.rs`
- `src/backends/wasm/request.rs`
- tests

#### Checklist

- [ ] Change `WasmBackendRequest` to receive selected functions from partition/link plan.
- [ ] Remove request-side reachability selection from final path.
- [ ] Lower selected HIR functions through structured HIR view.
- [ ] Emit `WasmItem::If` for structured conditionals.
- [ ] Emit `WasmItem::Loop`, `Break`, `Continue` for loops.
- [ ] Lower postfix `!` / `catch` using trailing error-handle branches.
- [ ] Lower options using niche ABI helpers.
- [ ] Lower scalar pure expressions into `WasmValue` trees where safe.
- [ ] Materialize values into locals when reused or crossing control boundaries.
- [ ] Add runtime helper calls as `WasmItem::CallRuntime`.
- [ ] Add tests for simple scalar functions, ifs, loops, returns, fallible calls.

#### Tests / validation

- [ ] HIR-to-LIR unit tests.
- [ ] Debug LIR dump tests if supported.
- [ ] Standard phase gate.

#### Exit criteria

- [ ] Selected functions lower to structured Wasm LIR.
- [ ] No old arbitrary CFG lowerer is used in final path.

---

### Phase 14 — Rewrite Wasm emitter for structured LIR and imported memory

#### Context

Emission should be direct from structured LIR with ephemeral planning only.

#### Primary files

- `src/backends/wasm/emit/`
- `src/backends/wasm/emit/functions.rs`
- `src/backends/wasm/emit/module.rs`
- `src/backends/wasm/emit/sections.rs`
- `src/backends/wasm/emit/instructions.rs`

#### Checklist

- [ ] Remove dispatcher-loop emission.
- [ ] Emit `If`, `Loop`, `Block`, `Break`, `Continue`, `Return` from structured LIR.
- [ ] Recursively emit pure `WasmValue` to stack.
- [ ] Keep ephemeral type/import/function/global/data planning.
- [ ] Import memory from runtime module instead of defining memory in user modules.
- [ ] Import runtime helper functions from runtime module.
- [ ] Support multi-value function signatures/results.
- [ ] Validate emitted bytes with `wasmparser`.
- [ ] Add tests for Wasm sections, imports, memory import, multi-value signatures.

#### Tests / validation

- [ ] Wasm parser validation tests.
- [ ] Section/index planning tests.
- [ ] Simple emitted Wasm integration tests.
- [ ] Standard phase gate.

#### Exit criteria

- [ ] Structured LIR emits valid Wasm.
- [ ] User modules no longer define their own memory.
- [ ] Dispatcher path is deleted or unused and marked for Phase 20 deletion.

---

### Phase 15 — Generate runtime helpers through structured LIR

#### Context

Runtime helpers must use the same LIR/emitter path as user functions. This avoids two codegen systems.

#### Primary files

- `src/backends/wasm/runtime/`
- `src/backends/wasm/runtime/helpers.rs` — new
- `src/backends/wasm/runtime/module.rs` — new
- `src/backends/wasm/emit/helpers.rs` — deletion/replacement target

#### Checklist

- [ ] Add `RuntimeHelperId` and `RuntimeHelperDef` registry.
- [ ] Add runtime module builder that emits helper functions as structured LIR.
- [ ] Generate allocator helpers.
- [ ] Generate `release` and `drop_if_owned` stubs.
- [ ] Generate memory import/export shape for runtime module.
- [ ] Remove raw helper function body emission from code-section path.
- [ ] Add tests for helper signatures and runtime module emission.

#### Tests / validation

- [ ] Runtime helper registry tests.
- [ ] Runtime module Wasm validation.
- [ ] Standard phase gate.

#### Exit criteria

- [ ] Runtime helpers are normal structured LIR functions.
- [ ] Raw helper emission path is removed or fully unused.

---

### Phase 16 — Implement tagged handles, object metadata, and string baseline

#### Context

This phase makes the runtime memory model concrete.

#### Primary files

- `src/backends/wasm/runtime/memory.rs`
- `src/backends/wasm/runtime/strings.rs`
- `src/backends/wasm/runtime/helpers.rs`
- `src/projects/html_project/runtime_artifacts.rs`
- JS runtime tests

#### Checklist

- [ ] Define tagged handle masks/constants.
- [ ] Define packed object `meta` layout.
- [ ] Implement `addr = handle & !0b111` helper.
- [ ] Treat `0` as no-op/null.
- [ ] Implement bump allocator with 8-byte alignment.
- [ ] Implement UTF-8 string object allocation.
- [ ] Implement string pointer/length helpers for JS decoding.
- [ ] Implement JS runtime decoding/encoding helpers.
- [ ] Reserve representation field for future UTF-16/JS-ref strings.
- [ ] Replace `StringFromI64` with `StringFromI32` / generic numeric formatting path.
- [ ] Add tests for tagged handle masking and string roundtrip.

#### Tests / validation

- [ ] Runtime memory unit tests.
- [ ] Wasm string helper integration test.
- [ ] JS/Wasm string decode test.
- [ ] Standard phase gate.

#### Exit criteria

- [ ] Tagged handles and UTF-8 baseline strings are working.
- [ ] No `i64` bridge remains for string formatting.

---

### Phase 17 — Implement struct, choice, error, option ABI runtime support

#### Context

Non-scalar values cross ABI as handles. Errors and options use niche representations.

#### Primary files

- `src/backends/wasm/runtime/layout.rs`
- `src/backends/wasm/runtime/errors.rs` — new
- `src/backends/wasm/hir_to_lir/`
- `src/backends/wasm/abi.rs`
- tests

#### Checklist

- [ ] Allocate structs as inline-field heap objects.
- [ ] Allocate fixed-layout choice payloads with tag + payload.
- [ ] Represent unit choices efficiently where possible.
- [ ] Implement `Error` object layout.
- [ ] Implement trailing `error_handle` helper lowering.
- [ ] Implement `Bool?`, `Char?`, handle-backed option, `Int?`, `Float?` ABI support.
- [ ] Add JS wrapper decoding for success/error ABI.
- [ ] Add tests for struct construction/field access/mutation.
- [ ] Add tests for choice construction/matching/equality where supported.
- [ ] Add tests for `Error!` propagation and `catch`.
- [ ] Add tests for option present/none handling.

#### Tests / validation

- [ ] Backend unit tests.
- [ ] Integration tests with `html_wasm` backend assertions.
- [ ] Standard phase gate.

#### Exit criteria

- [ ] Structs, choices, errors, and options work in Wasm-owned functions.

---

### Phase 18 — Implement fixed/growable collection runtime support

#### Context

Collections are data/runtime features and should not push functions to JS. Fixed and growable collections get distinct layouts from the first final implementation.

#### Primary files

- `src/backends/wasm/runtime/collections.rs` — new
- `src/backends/wasm/hir_to_lir/`
- `src/backends/backend_feature_validation.rs`
- tests

#### Checklist

- [ ] Implement inline fixed collection layout under threshold.
- [ ] Implement out-of-line fixed buffer layout above threshold.
- [ ] Implement growable collection layout.
- [ ] Add helper families specialized by element ABI category.
- [ ] Lower literals for fixed and growable collections.
- [ ] Lower `get`, `set`, `push`, `remove`, `length`.
- [ ] Use trailing `error_handle` for fallible operations.
- [ ] Ensure scalar elements are unboxed.
- [ ] Update backend feature validation: collections no longer unsupported for Wasm where implemented.
- [ ] Add tests for threshold behavior.

#### Tests / validation

- [ ] Unit tests for collection layout.
- [ ] Integration tests for fixed/growable operations.
- [ ] Error-path tests for out-of-bounds/capacity.
- [ ] Standard phase gate.

#### Exit criteria

- [ ] Collections work in Wasm-owned functions with distinct fixed/growable layouts.

---

### Phase 19 — Implement ordered scalar-key map runtime support

#### Context

Maps are built-in language data structures and should be Wasm-native for the existing scalar-keyed surface.

#### Primary files

- `src/backends/wasm/runtime/maps.rs` — new
- `src/backends/wasm/hir_to_lir/`
- `src/backends/backend_feature_validation.rs`
- tests

#### Checklist

- [ ] Implement map object layout with ordered entries and lookup index/table.
- [ ] Add map layout metadata for key/value ABI and scan/drop behavior.
- [ ] Implement helper families for `String`, `Int`, `Bool`, `Char` keys.
- [ ] Avoid scalar key boxing.
- [ ] Lower map literals.
- [ ] Lower `get`, `set`, `remove`, `contains`, `length`, `clear`.
- [ ] Preserve insertion order semantics.
- [ ] Use trailing `error_handle` for fallible operations.
- [ ] Keep map iteration deferred.
- [ ] Update backend feature validation: existing map surface no longer unsupported for Wasm.

#### Tests / validation

- [ ] Map literal tests.
- [ ] Ordered semantics tests.
- [ ] get/set/remove/contains/clear tests.
- [ ] Borrow alias tests around map `get` and mutation remain valid.
- [ ] Standard phase gate.

#### Exit criteria

- [ ] Existing scalar-key map surface works in Wasm-owned functions.

---

### Phase 20 — Implement checked numerics, casts, Float formatting/validation

#### Context

Checked numeric operations, runtime casts, and Float formatting are language semantics. They must be Wasm-owned, not JS fallback.

#### Primary files

- `src/backends/wasm/runtime/numerics.rs` — new
- `src/backends/wasm/runtime/casts.rs` — new
- `src/backends/wasm/runtime/float_format.rs` — new
- `src/backends/wasm/hir_to_lir/`
- `src/backends/backend_feature_validation.rs`
- tests

#### Checklist

- [ ] Implement checked `i32` arithmetic helpers or inline lowering.
- [ ] Implement divide/modulo by zero checks.
- [ ] Implement checked range/cast helpers.
- [ ] Implement trap-mode lowering.
- [ ] Implement recoverable mode using trailing `error_handle`.
- [ ] Implement Beanstalk-stable Float formatting.
- [ ] Implement external Float boundary validation.
- [ ] Implement runtime builtin casts.
- [ ] Remove Wasm unsupported-feature diagnostics for implemented numeric/cast/Float features.
- [ ] Add parity tests against JS backend outputs where possible.

#### Tests / validation

- [ ] Checked arithmetic success/failure tests.
- [ ] Float formatting tests.
- [ ] Runtime cast tests.
- [ ] JS/Wasm parity integration tests.
- [ ] Standard phase gate.

#### Exit criteria

- [ ] Numeric/cast/Float semantics are Wasm-owned and parity-tested.

---

### Phase 21 — Implement neutral IO host imports and Wasm-native core helpers

#### Context

Console IO is neutral. Core compute/text/random helpers should be Wasm-native where possible. DOM/browser/project JS remains JS-required.

#### Primary files

- `src/backends/wasm/runtime/imports.rs`
- `src/projects/html_project/runtime_artifacts.rs`
- `src/compiler_frontend/external_packages/`
- `src/projects/html_project/external_libraries/`
- tests

#### Checklist

- [ ] Replace empty `WasmHostFunction` enum with real neutral host imports.
- [ ] Add console IO imports: line/print/debug/warn/error.
- [ ] Add runtime JS host import implementations.
- [ ] Add Wasm-native lowering for core math/text/random functions where selected.
- [ ] Keep DOM/canvas/input/project JS as JS-required.
- [ ] Add diagnostics for unsupported Wasm-native package calls.
- [ ] Add tests that console IO does not change partition target.
- [ ] Add tests that DOM/canvas/project JS does change partition target.

#### Tests / validation

- [ ] Target-affinity tests.
- [ ] Neutral IO Wasm integration tests.
- [ ] Standard phase gate.

#### Exit criteria

- [ ] Neutral IO works from Wasm-owned functions.
- [ ] External package metadata drives partitioning and validation.

---

### Phase 22 — Wire direct Wasm-to-Wasm imports across modules

#### Context

When a Wasm-owned function calls a facade-exported Wasm-owned function from another module, it should use a direct Wasm function import wired by JS companions, not a JS callback.

#### Primary files

- `src/projects/html_project/artifact_plan.rs`
- `src/projects/html_project/companions.rs`
- `src/backends/wasm/request.rs`
- `src/backends/wasm/lir/linkage.rs`
- `src/backends/wasm/emit/imports.rs`

#### Checklist

- [ ] Identify Wasm-owned cross-module calls from partition/link plan.
- [ ] Add Wasm import plan entries for those calls.
- [ ] Export only facade runtime ABI surface from provider modules.
- [ ] Keep private functions internal.
- [ ] Generate companion initialization that passes dependency Wasm exports into dependent module instantiation.
- [ ] Ensure generated artifact graph is acyclic.
- [ ] Add tests for module A Wasm calling module B Wasm.
- [ ] Add tests that JS wrappers are not used for direct Wasm-to-Wasm calls.

#### Tests / validation

- [ ] Cross-module Wasm integration tests.
- [ ] Companion JS snapshot tests.
- [ ] Standard phase gate.

#### Exit criteria

- [ ] Direct cross-module Wasm imports work through companions.

---

### Phase 23 — Wire JS-to-Wasm wrappers in companions

#### Context

JS-owned functions, page entry JS, and consumer modules call Wasm-owned exports through generated wrappers.

#### Primary files

- `src/projects/html_project/companions.rs`
- `src/backends/js/`
- `src/projects/html_project/js_writer.rs`
- tests

#### Checklist

- [ ] Generate JS wrappers for Wasm-owned facade exports.
- [ ] Encode JS scalar/string/non-scalar arguments into ABI lanes/handles.
- [ ] Decode scalar/handle/multi-return results.
- [ ] Decode trailing `error_handle` into JS-side internal error behavior.
- [ ] Ensure wrappers call runtime string/object helpers rather than inspecting Wasm memory ad hoc.
- [ ] Ensure wrappers do not duplicate layout metadata source of truth.
- [ ] Add tests for scalar calls, string calls, multi-return, `Error!`, options.

#### Tests / validation

- [ ] JS wrapper unit/snapshot tests.
- [ ] End-to-end page test where JS start calls Wasm helper.
- [ ] Standard phase gate.

#### Exit criteria

- [ ] JS can call Wasm-owned functions through companions.

---

### Phase 24 — Replace old HTML-Wasm artifact path

#### Context

Once mixed page entry, companions, runtime artifacts, and Wasm modules work, delete the old `compile_html_module_wasm` path that assumes `bst_start`.

#### Primary files

- `src/projects/html_project/wasm/artifacts.rs`
- `src/projects/html_project/wasm/export_plan.rs`
- `src/projects/html_project/wasm/js_bootstrap.rs`
- `src/projects/html_project/wasm/request.rs`
- `src/projects/html_project/html_project_builder.rs`

#### Checklist

- [ ] Replace calls to `compile_html_module_wasm` with artifact/link-plan pipeline.
- [ ] Delete `bst_start` export planning.
- [ ] Delete old JS bootstrap generator.
- [ ] Delete duplicated HTML-Wasm helper export structs.
- [ ] Keep only builder-owned Wasm request conversion that still applies to per-module Wasm artifacts.
- [ ] Rename remaining modules to match final responsibilities.
- [ ] Ensure no code path treats Wasm as whole-page backend mode.
- [ ] Update tests/goldens for new artifact layout.

#### Tests / validation

- [ ] HTML mixed backend integration tests.
- [ ] Artifact path conflict tests.
- [ ] Standard phase gate.

#### Exit criteria

- [ ] Old HTML-Wasm page mode is gone.
- [ ] `start` is never exported to Wasm.

---

### Phase 25 — Delete legacy Wasm backend scaffolding

#### Context

The final implementation must not leave old backend scaffolding as codebase bloat.

#### Primary files

- `src/backends/wasm/request.rs`
- `src/backends/wasm/lir/`
- `src/backends/wasm/emit/`
- `src/backends/wasm/hir_to_lir/`
- tests

#### Checklist

- [ ] Delete `WasmCfgLoweringStrategy`.
- [ ] Delete dispatcher-loop emission code.
- [ ] Delete old `WasmLirBlock` if fully replaced.
- [ ] Delete old `Jump`/`Branch` terminators.
- [ ] Delete `I64` bridge paths for Beanstalk `Int`.
- [ ] Delete `StringFromI64`.
- [ ] Delete per-module memory section emission for user modules.
- [ ] Delete raw helper body emission path.
- [ ] Search for `DispatcherLoop`, `StringFromI64`, `I64`, `bst_start`, `export_str_ptr`, old helper booleans.
- [ ] Remove obsolete tests that assert old shapes.
- [ ] Replace broad old tests with behavior-focused integration tests.

#### Tests / validation

- [ ] `rg "DispatcherLoop|StringFromI64|bst_start" src tests docs` returns only intentional docs/history if any.
- [ ] `rg "I64" src/backends/wasm` has no Beanstalk `Int` lowering use.
- [ ] Standard phase gate.

#### Exit criteria

- [ ] Legacy scaffolding is removed.
- [ ] Final code path has one current API shape.

---

### Phase 26 — Optional post-emission optimizer hook

#### Context

Deep release optimization is optional, external, and correctness-independent.

#### Primary files

- `src/backends/wasm/optimize.rs` — new
- `src/projects/html_project/artifact_plan.rs`
- config handling if needed
- tests

#### Checklist

- [ ] Add optimizer hook after Wasm emission and before final artifact writing.
- [ ] Keep optimizer disabled by default unless explicitly configured.
- [ ] Do not add a hard dependency on a specific optimizer unless chosen deliberately.
- [ ] Validate compiler-emitted Wasm before optimizer during Alpha.
- [ ] Validate optimized Wasm after optimizer.
- [ ] Ensure optimizer cannot change JS/Wasm partitioning or ABI.
- [ ] Add tests with optimizer disabled.
- [ ] Add a mock optimizer test if practical.

#### Tests / validation

- [ ] Unit tests for optimizer hook/no-op path.
- [ ] Standard phase gate.

#### Exit criteria

- [ ] Optimization is swappable and correctness-independent.

---

### Phase 27 — Documentation pass

#### Context

The compiler, user docs, roadmap, and progress matrix must reflect the final backend model.

#### Primary files

- `docs/compiler-design-overview.md`
- `docs/language-overview.md`
- `docs/memory-management-design.md`
- `docs/src/docs/project-structure/#page.bst`
- `docs/src/docs/libraries/#page.bst`
- `docs/src/docs/progress/#page.bst`
- `docs/src/docs/async/#page.bst`
- `docs/roadmap/roadmap.md`

#### Checklist

- [ ] Document HTML builder mixed JS/Wasm strategy in compiler overview.
- [ ] Document Wasm backend ownership and what it does not own.
- [ ] Document JS/Wasm target selection in user-facing docs.
- [ ] Document neutral IO.
- [ ] Document `start` always JS-owned.
- [ ] Document MPA page-local runtime memory.
- [ ] Document Wasm ABI summaries in backend-facing docs.
- [ ] Update memory-management doc with tagged handle details if accepted as source of truth.
- [ ] Update async docs to mention reserved backend/runtime seams, without claiming implementation.
- [ ] Update roadmap by removing completed/replaced TODOs.
- [ ] Update progress matrix.

#### Tests / validation

- [ ] `cargo run --quiet -- check docs`
- [ ] Standard phase gate.

#### Exit criteria

- [ ] Docs describe the final architecture without tracking implementation incompleteness.
- [ ] Roadmap/progress track remaining work separately.

---

### Phase 28 — Final integration matrix and audit

#### Context

The final backend needs integration coverage across JS-only behavior, mixed JS/Wasm behavior, Wasm runtime helpers, and unsupported feature diagnostics.

#### Primary files

- `tests/cases/`
- `src/compiler_tests/`
- `src/backends/wasm/tests/`
- `src/projects/html_project/tests/`

#### Checklist

- [ ] Add canonical mixed backend fixture where JS `start` calls Wasm helper.
- [ ] Add fixture where DOM function is JS and pure callee is Wasm.
- [ ] Add fixture where console IO in pure function stays Wasm.
- [ ] Add cross-module Wasm-to-Wasm import fixture.
- [ ] Add string/multi-return/error/option fixtures.
- [ ] Add struct/choice/collection/map fixtures.
- [ ] Add numeric/cast/Float parity fixtures.
- [ ] Add unsupported DOM/browser-in-Wasm diagnostic fixture if partitioning cannot move it.
- [ ] Add artifact assertions for runtime, companions, module Wasm, page JS, page HTML.
- [ ] Add Wasm validation assertions.
- [ ] Add partition report tests.
- [ ] Add stale-output cleanup tests for new artifact layout.

#### Tests / validation

- [ ] Full `just validate`.
- [ ] Manual artifact inspection for at least one multi-page project.
- [ ] Manual generated JS/Wasm path review.

#### Exit criteria

- [ ] Final backend behavior is covered by integration tests.
- [ ] Generated artifact shape is stable.

---

## 5. Agent implementation rules

Agents implementing this plan should follow these constraints.

### 5.1 Do not preserve old APIs for compatibility

Beanstalk is pre-release. Prefer one current API shape. Delete old wrappers and transitional paths once replacements are wired.

### 5.2 Keep modules small and owned

Suggested final module shape:

```text
src/projects/html_project/
  artifact_plan.rs
  partition.rs
  companions.rs
  js_writer.rs
  page_entry/
    mod.rs
    fragments.rs
    script.rs
  runtime_artifacts.rs
  layout_plan.rs
  wasm/
    mod.rs              -- only builder-owned wasm orchestration that remains

src/backends/wasm/
  abi.rs
  backend.rs
  request.rs
  result.rs
  hir_to_lir/
  lir/
    mod.rs
    builder.rs
    body.rs
    values.rs
    validation.rs
  emit/
    mod.rs
    module.rs
    functions.rs
    instructions.rs
    sections.rs
  runtime/
    mod.rs
    helpers.rs
    memory.rs
    strings.rs
    collections.rs
    maps.rs
    numerics.rs
    casts.rs
    float_format.rs
    layout.rs
```

### 5.3 Keep request objects explicit

Do not pass many parameters. Use named plan/input structs.

Good:

```rust
pub struct HtmlMixedBuildInput<'a> {
    pub graph: &'a CompiledProjectModuleGraph,
    pub config: &'a Config,
    pub flags: &'a [Flag],
    pub string_table: &'a mut StringTable,
}
```

Bad:

```rust
fn build_mixed(a, b, c, d, e, f, g, h) { ... }
```

### 5.4 Keep diagnostics typed

- User-facing unsupported backend/package/config errors: `CompilerDiagnostic`.
- Internal invalid LIR/ABI/layout bugs: `CompilerError`.
- Prefer stable diagnostic codes in integration tests.

### 5.5 Keep generated JS deterministic

- Stable import order.
- Stable export order.
- Stable helper order.
- Stable path ordering.
- Stable partition report ordering.

### 5.6 Do not make debug/release target split differ

Allowed release differences:

- omit debug names;
- minify JS;
- run optional Wasm optimizer;
- omit reports;
- strip source comments.

Disallowed release differences:

- different JS/Wasm function target selection;
- different ABI;
- different layout IDs;
- different runtime helper semantics.

---

## 6. Final acceptance checklist

The implementation is complete only when all items below are true.

### Architecture

- [ ] HTML builder is graph/link-plan-driven.
- [ ] `start` is always JS-owned.
- [ ] Mixed JS/Wasm function partitioning is deterministic.
- [ ] Partition report exists and is testable.
- [ ] JS companions are the stable module facade.
- [ ] Runtime artifacts are emitted once per project output.
- [ ] Runtime instance/memory is page-local.
- [ ] Module Wasm imports runtime memory and helpers.
- [ ] Cross-module Wasm calls use direct Wasm imports.

### ABI/runtime

- [ ] `Int` is `i32` only.
- [ ] Non-scalars cross ABI as tagged `i32` handles.
- [ ] Ownership bit lives in handle tag bits.
- [ ] Packed object metadata exists.
- [ ] Project-wide layout IDs exist.
- [ ] UTF-8 baseline string representation works.
- [ ] Multi-return uses Wasm multi-value.
- [ ] `Error!` uses trailing error-handle niche ABI.
- [ ] `T?` uses niche ABI where available.
- [ ] Collections are Wasm-native.
- [ ] Maps are Wasm-native for existing scalar-keyed surface.
- [ ] Numeric/cast/Float helpers are Wasm-native.
- [ ] Neutral IO works from Wasm.

### LIR/emission

- [ ] Structured Wasm LIR is the only persistent Wasm backend IR.
- [ ] Pure values are separate from effect statements.
- [ ] Runtime helpers are generated through the same LIR/emitter path.
- [ ] Flat CFG LIR is removed.
- [ ] Dispatcher-loop emission is removed.
- [ ] Per-module memory definition is removed.
- [ ] Raw helper body emission is removed.
- [ ] Emitted Wasm validates.

### Cleanup

- [ ] No `bst_start` page-entry contract remains.
- [ ] No `StringFromI64` remains.
- [ ] No Beanstalk `Int` lowering uses `i64`.
- [ ] No duplicated helper export boolean structs remain.
- [ ] No old HTML-Wasm bootstrap path remains.
- [ ] Roadmap/progress docs updated.
- [ ] User-facing docs explain target selection.

### Validation

- [ ] `cargo fmt`
- [ ] `just validate`
- [ ] Manual stage-boundary audit complete.
- [ ] Integration backend matrix covers mixed artifacts.
- [ ] Artifact assertions cover `.js`, `.wasm`, runtime files, companions, and route HTML.

---

## 7. Known deferred work

These are explicitly out of scope for this implementation plan.

- Channels / green threads / async implementation.
- Stack switching.
- JS Promise Integration.
- Component Model packaging.
- Cross-module Wasm merging.
- SPA/router builder model.
- Wasm GC runtime representation.
- Memory64.
- Threads/atomics.
- Map iteration.
- User-defined map keys / `HASHABLE`.
- User-facing explicit `#wasm.bst` module entry.
- UTF-16/JS-string-backed string representation implementation.
- Deep compiler-owned Wasm optimization passes.

The plan reserves runtime/linking seams for async and channels, but does not implement them.

