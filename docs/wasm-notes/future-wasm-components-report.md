# Beanstalk and the WebAssembly Component Model Backend Plan

## Beanstalk’s current Wasm pipeline baseline and where it strains today

Beanstalk’s compiler pipeline is explicitly split into a frontend that produces a stable semantic IR (HIR) plus borrow-check facts, and a backend/build-system layer that consumes that output to generate artefacts. The core stages are: project structure → tokenization → header parsing → dependency sorting → AST construction → HIR → borrow validation; then project builders perform backend lowering (JS or Wasm) using the `BackendBuilder`/`ProjectBuilder` seam. fileciteturn9file0L1-L1

Two current design choices are especially relevant to a long-term “components-first” Wasm story:

Beanstalk’s memory model is GC-first with static analysis used as an optimisation layer, not as a semantic requirement. Ownership is described as a runtime-tagged optimisation (e.g., an “ownership bit” in tagged pointers), with “possible_drop” sites that become no-ops in GC-only backends and conditional frees in hybrid backends. fileciteturn8file0L1-L1

The Wasm backend is the long-term primary target, but it is currently constrained to emitting a core Wasm module in a “phase-2” state: feature flags for Wasm GC, multi-value, and reference types are actively rejected by request validation, and the backend is explicitly focused on “core linear-memory Wasm only” at present. fileciteturn16file0L1-L1 fileciteturn15file0L1-L1

HTML+Wasm mode in the build system also reveals exactly where the component model can simplify the pipeline. The HTML builder currently:
- Plans an export set plus helper exports dedicated to JS interop (`memory`, `bst_str_ptr`, `bst_str_len`, `bst_release`). fileciteturn13file0L1-L1 fileciteturn17file0L1-L1  
- Emits a builder-owned JS bootstrap that instantiates Wasm, defines host imports under a `host` module (e.g., `host.log_string`), provides DOM-handle registries, and performs manual string decoding by reading exported linear memory. fileciteturn10file0L1-L1 fileciteturn23file0L1-L1  
- Couples host-call lowering to a small, explicit host-call registry (`io` today), and then maps host calls into Wasm imports with module/item names (currently `"host"`, `"log_string"`, etc.). fileciteturn20file0L1-L1 fileciteturn21file0L1-L1

That “manual shared-memory interop” pattern is precisely what the component model is designed to replace, but the replacement needs to be staged to match Beanstalk’s current backend maturity and its browser-first build workflows.

## What the component model provides and what is realistically usable in early 2026

At the component layer, a WebAssembly component is intended to be a self-describing binary that interacts through interfaces rather than by sharing linear memory. In the component model’s framing, components can still *use* memory internally, but “memories are never exported or imported; they are not shared” across component boundaries. citeturn11search0

Interfaces for components are defined in WIT (WebAssembly Interface Types), where:
- Interfaces and worlds define contracts (imports/exports) but do not define behaviour. citeturn0search2  
- Identifiers are restricted to ASCII kebab-case (with details like “no leading/trailing hyphens” and “no underscores”). This has a direct impact on how Beanstalk symbol names will map into externally visible ABI names. citeturn13search0  
- A “world” describes a full component contract (imports the component needs; exports it provides), and the world boundary is intentionally a sandbox: if an interface is not imported, the component cannot access that capability. citeturn0search3  

The “canonical ABI” is the component model’s key enabling mechanism: a standardised ABI for lifting/lowering rich types (strings, lists, records/variants, etc.) so components written in different languages can interoperate without sharing internal representations. citeturn0search1

As of early 2026, the component ecosystem is genuinely practical, but the deployment story is segmented:

Runtimes commonly standardise on two “runnable” worlds: `wasi:cli/command` and `wasi:http/proxy`. Everything else is treated as a custom world/interface. citeturn14search0

Wasmtime positions itself as the reference implementation for the component model, with CLI support for running `wasi:cli/command`, serving `wasi:http/proxy`, and (in newer versions) invoking functions on components with custom exports. citeturn4search0

For JavaScript, `jco` is the key bridging tool. It is explicitly designed to “transpile” components into ES modules, so that environments that only support core modules (including browsers) can still use components. The component-model docs are direct: browsers can run core modules but “cannot yet execute WebAssembly components,” so transpilation is required for browser usage. citeturn12search2  
The `jco` transpiling documentation also makes two practical points that matter to Beanstalk: transpiled output is a JS module that imports the component’s imports and re-exports the component’s exports, and WASI imports are automatically mapped to a Preview 2 shim that targets both Node.js and browsers (with browser WASI explicitly described as experimental). citeturn12search1

On the WASI side, the stable platform target in the component era remains WASI 0.2. WASI 0.2 APIs are defined in WIT and are meant to be composed into components; the identified set includes clocks, random, filesystem, sockets, CLI, and HTTP. citeturn0search0  
A forthcoming WASI 0.3 line is positioned as adding native async at the component ABI level (with `future<T>` and `stream<T>` types) and refactoring 0.2 interfaces to take advantage of async; as of the current WASI roadmap snapshot, 0.3.0 is still described in preview terms with completion targeted around February 2026. citeturn3search0

image_group{"layout":"carousel","aspect_ratio":"16:9","query":["WebAssembly component model architecture diagram WIT world imports exports","WebAssembly canonical ABI lift lower diagram","Wasm component model composition diagram","WIT interface world diagram"]}

## A near-term integration strategy that fits Beanstalk’s current backend maturity

The highest-leverage design choice for Beanstalk is to treat “componentisation” as an *outer packaging layer* at first, not as a rewrite of the Wasm backend:

The core Wasm backend continues to emit a core module from HIR→LIR→Wasm, preserving your ongoing work on LIR, ownership lowering, and runtime scaffolding. fileciteturn6file0L1-L1 fileciteturn16file0L1-L1

A new “interface planning” stage is introduced *before* core module emission finalises names and host imports, with one primary output: a WIT package (world + exported interfaces + imported interfaces). This aligns with the component model’s “world-first” framing: a runtime only needs to know what world a component targets in order to execute or embed it. citeturn13search2

A new “componentisation” stage is introduced *after* core module emission, converting the module into a component and embedding the WIT interface information. The `wit-component` tooling explicitly describes this pipeline shape: creating components from input core modules, driven by embedded WIT interface metadata, with support for canonical ABI-based imported/exported interfaces. citeturn6search9

This gives you a workable adoption split:

What Beanstalk can utilise now (early alpha):
- WIT as an explicit ABI contract for interop and host capability boundaries (even if the internal backend is still evolving). citeturn0search2turn0search3  
- Componentisation for “server/CLI style” runtimes and tooling built around `wasi:cli/command` and custom-export invocation (Wasmtime), without requiring browsers to support components. citeturn4search0turn14search0  
- `jco transpile` as the bridge to browsers, replacing builder-specific JS memory/string glue with a generated wrapper that speaks the component world in JS terms. citeturn12search1turn12search2  

What should be treated as “eventual / maturing” (to avoid stalling alpha):
- Designing Beanstalk’s async semantics around WASI 0.3 `future/stream` types (because WASI 0.3 is still in preview and the language async design is explicitly “still evolving”). citeturn3search0turn24file0L1-L1  
- Deep exploitation of resource types and richer inter-component composition patterns, beyond a small set of stable host interfaces in the early language. citeturn2search3

## How components change library, package, host-interface, and interop design for Beanstalk

The key mental model shift is that you stop thinking in terms of “module exports + shared memory ABIs”, and instead treat a package boundary as “world + canonical ABI + composition”.

The component model’s own documentation frames composition as the analogue of building higher-level libraries/applications by linking packages, except the unit is a component and the contract is WIT, enabling cross-language composition. citeturn13search5

### Reusable libraries as component packages

A realistic Beanstalk-aligned design is:

Each Beanstalk library package ships two artefacts:
- A component binary (the actual implementation).
- Its WIT package (interfaces + worlds describing the surface area), either embedded in the component and/or distributable as a package dependency.

This fits naturally with:
- Beanstalk’s desire for backend-agnostic build systems that can consume compilation output and apply their own codegen. WIT becomes the backend-agnostic ABI contract, while the emitted component is one backend product. fileciteturn9file0L1-L1  
- A “GC-first semantics” memory model: you can keep internal memory/ownership lowering as an optimisation, while the component boundary stays copy/handle based via canonical ABI. fileciteturn8file0L1-L1 citeturn0search1turn11search0

Practically, Beanstalk should define “ABI-safe public surface” rules early:
- Public exports are restricted to types that map cleanly to WIT (primitives, string, list, record, variant/result, and later resources). citeturn0search2turn2search3  
- Generic, highly-polymorphic, or compiler-internal types are exported as opaque resources (eventual) or not exported at all (near-term). citeturn2search3  
- Function and type names must have a deterministic mapping to WIT kebab-case, with an escape hatch for explicit WIT naming where Beanstalk naming would be lossy. citeturn13search0

### Host interfaces as first-class WIT, not ad-hoc “host imports”

Beanstalk already has an explicit host-call registry and a clear rule that host calls are preserved as explicit call nodes in HIR (no abstraction layer today). fileciteturn9file0L1-L1 fileciteturn20file0L1-L1  
The component model gives you a principled way to evolve this:

Define host capabilities as versioned WIT packages in a `beanstalk:*` namespace (for web-specific concepts like DOM) and adopt WASI packages for portable/system capabilities.

Two concrete near-term moves align with the current repo architecture:

Replace the Wasm backend’s host-import mapping (currently “module = host”, “item = log_string”) with a WIT import such as `beanstalk:host/logging@0.1.0` and a function like `log: func(text: string)`. The lowering stage remains similar (you still need an import), but the *contract* becomes WIT and the binding generation becomes tool-assisted rather than “manual pointer/len reading”. fileciteturn21file0L1-L1 citeturn0search2turn0search1

Adopt WASI 0.2 packages for anything that is plausibly portable across hosts (filesystem, sockets, clocks, CLI, HTTP). WASI 0.2 is explicitly the current stable WASI release and is designed for the component model/WIT ecosystem. citeturn0search0turn14search5

For browsers, you still need a JavaScript “host implementation” for web-only interfaces. The crucial difference is that, with component tooling, that host implementation plugs into the WIT import boundary rather than by peeking into exported memory. `jco transpile` explicitly supports remapping imports via a `--map` configuration, which Beanstalk’s HTML builder can generate automatically. citeturn12search1

## Medium-term packaging and dependency management for a component-native Beanstalk ecosystem

Once Beanstalk can reliably emit components, the next ecosystem unlock is packaging and dependency resolution in terms of component/WIT packages rather than language-specific source-level linking.

The component model docs describe `wkg` as the CLI that fetches and publishes components and WIT packages, typically addressed by package names like `namespace:package@version`, with configuration mapping namespaces to registries. citeturn10search0  
This is unusually aligned with Beanstalk’s “modularity-first” goals because it cleanly separates:
- WIT dependency resolution (interface-level linking and version pinning).
- Component binary distribution (implementation artefacts).
- Build-system orchestration (composition and bundling strategies).

A practical Beanstalk “alpha-to-beta” plan here looks like:

Use `wkg wit fetch` / lockfile semantics to make WIT dependencies reproducible in CI and local builds, and treat WIT dependencies as the driver for which host imports the component can legally call. citeturn10search0turn0search3

Publish Beanstalk standard library interfaces (and later stdlib components) under a stable namespace (e.g., `beanstalk:std@…`) and keep WASI dependencies external and versioned (e.g., `wasi:http@0.2.x`). citeturn10search0turn0search0

Treat Warg as a future-facing registry protocol option rather than a dependency today. Warg is explicitly described as “in development” and “component model oriented”, aiming to provide canonical names/versions with a transparency-style security model. citeturn10search7

For composition, bring in build-time composition tooling (so a Beanstalk application can be assembled from component libraries). The `wac` CLI is positioned as a “composition tool” that can plug components together, and it can even reference packages from registries in simple operations. citeturn2search12  
This allows Beanstalk to avoid reinventing a component linker while still offering a cohesive `bst build` experience.

## Long-term: full utilisation as the component model and WASI mature

Long term, “taking advantage of the component model” should mean that components are not just the output format, but the organising principle for the entire interop story: libraries, packages, hosts, tooling, and (eventually) async boundaries.

Three maturity-driven upgrades are worth explicitly planning for, because they influence early design decisions:

### Rich boundary types via WIT resources

Resources are the component model’s mechanism for handles with behaviour that lives on one side of the boundary (host or another component), with method-like operations and constructors in WIT. citeturn2search3  
For Beanstalk, resources are a natural eventual replacement for “integer handle registries” (like the DOM handle map in today’s HTML+Wasm JS bootstrap). fileciteturn10file0L1-L1  
The near-term approach can keep integer handles, but the WIT surface should be designed so those can transition into WIT resources without breaking all user code (i.e., the handle type should be abstracted at the interface level, not baked into every function signature).

### Native async at the component ABI level

WASI’s roadmap describes WASI 0.3 as adding native async support to the component model, implemented in terms of canonical ABI changes, including `future<T>` and `stream<T>` types that can appear in parameters/results. citeturn3search0  
This is “eventual” for Beanstalk, but it suggests one very actionable early-alpha guideline: don’t hard-code “sync-only” ABI assumptions into Beanstalk’s host interface strategy. Instead, structure the compiler so that “async vs sync” is a property of the WIT world/bindings generation, not a property of ad-hoc JS glue code.

### Distribution as OCI artefacts in the wider ecosystem

The CNCF TAG Runtime WASM working group publishes a Wasm OCI artefact layout that identifies a Wasm artefact via a specific config media type (`application/vnd.wasm.config.v0+json`) and uses `application/wasm` layers, explicitly aiming for cross-project registry compatibility; it also notes browser support as out of scope for that packaging format. citeturn5search0  
For Beanstalk, this chiefly matters as an eventual “publish target” for component artefacts and reusable libraries when you want standard cloud-native distribution without being bound to a language registry.

## Concrete pipeline additions that components enable, and what they simplify in Beanstalk’s current codebase

This section ties the plan to the actual seams already present in the repository, and lists the specific compiler/build steps that become simpler or more powerful when moving from “core Wasm module + custom JS glue” to “component + WIT”.

### Add an interface-planning stage next to export planning

Today, HTML+Wasm mode computes an export plan and a helper-export policy. fileciteturn13file0L1-L1  
In a component world, the *export plan becomes a world definition*:

- Exported functions move from “builder-chosen stable names like `bst_call_N`” into a WIT interface. fileciteturn13file0L1-L1 citeturn0search2turn13search0  
- Helper exports designed for memory peeking (`memory`, `bst_str_ptr`, `bst_str_len`, `bst_release`) are no longer part of your public contract, because components do not export/import memories for sharing. fileciteturn17file0L1-L1 citeturn11search0  

Immediate simplification: you can delete an entire class of “JS interop helper export” logic once browser integration is done through `jco transpile` or another component-aware wrapper generator.

### Introduce a componentisation stage after core Wasm emission

Your backend already has an explicit, testable seam: `lower_hir_to_wasm_module` emits bytes, with clear “request” and “debug outputs” structure. fileciteturn16file0L1-L1  
The componentisation step can be an additional artefact transformer:

- Input: core module bytes + WIT package (world) + mapping metadata.
- Output: component bytes with embedded interface information.

The `wit-component` tooling describes the key requirement: the WIT interface is embedded in the core module, and then the core module is converted into a component whose imported/exported interfaces follow the canonical ABI. citeturn6search9

Implementation-wise, Beanstalk can start by shelling out to standard tooling (during alpha) and later move to linking the relevant libraries to avoid external toolchain drift. That choice is orthogonal to the design plan; the key is that this becomes a *pipeline stage* with deterministic inputs/outputs and clear debug/validation.

### Replace HTML+Wasm manual bootstrap with a generated component wrapper

The HTML builder currently emits bespoke JS that:
- Instantiates the module.
- Implements host imports under a “host” module.
- Defines wrapper exports and manual string decoding by reading memory buffers. fileciteturn10file0L1-L1

A component-based HTML build can instead:
- Emit a component.
- Run `jco transpile` to generate an ES module wrapper that exports the component exports in JS form, and wires imports through mapping rules and WASI shims when applicable. citeturn12search1turn12search2  

This directly addresses the most brittle part of the current HTML+Wasm pipeline: manual memory/string ABI coupling. It also future-proofs you against expanding type surfaces (records/results/variants) where manual JS glue becomes painful.

Important caveat: `jco`’s runtime WASI implementation can grant broad access to system resources, so you’ll want the Beanstalk build system to control which imports exist in the world and what shims are enabled. citeturn12search0turn0search3

### Unify host-interface definitions across backends via WIT

Beanstalk’s frontend currently has a host registry with a small ABI type set (`I32`, `Utf8Str`, `Void`) and a single built-in host function `io`. fileciteturn20file0L1-L1  
The Wasm lowering currently maps host calls to Wasm imports based on names and signatures, using a backend-specific enum of host functions. fileciteturn23file0L1-L1 fileciteturn21file0L1-L1  

A component-native design replaces:
- “backend-private host import enums”  
with  
- “versioned WIT package definitions for host interfaces”.

That unification has two payoffs:
- Other backends (JS, future native) can implement the same WIT-defined “host surface”, instead of each backend inventing a different FFI contract.
- Interop becomes language-agnostic: a Rust/Go/JS host can generate bindings from the same WIT world and host Beanstalk components consistently. citeturn0search1turn0search2

## Risks, stabilisation strategy, and early decisions worth locking in

The plan above assumes a realistic constraint: the component model ecosystem is usable now, but it is still evolving—especially around async (WASI 0.3) and around browser-native component execution. citeturn3search0turn12search2

For Beanstalk, the stabilisation strategy that best matches an early-alpha language is:

Stabilise “external contracts” early, not internal lowering details. WIT worlds and exported interfaces can become the compatibility boundary, allowing you to evolve HIR→LIR and ownership lowering internally without constantly disrupting user-facing bindings.

Keep the component boundary narrow in alpha. Export fewer, higher-level functions and avoid leaking internal runtime representations (handles, tagged pointers, etc.) into public WIT where you can. This aligns with the component model’s emphasis on interface-driven development and strong boundaries. citeturn0search3turn11search0

Design the naming scheme now. Because WIT identifiers are constrained to kebab-case ASCII, you should decide early how Beanstalk names map to WIT names (and how you escape collisions). citeturn13search0

Treat browser support as “component + transpile”, and make that an explicit build mode. The docs are unambiguous that browsers can’t execute components directly yet, so a first-class “transpile to browser runnable” step (likely through `jco transpile`) should be part of the planned artefact pipeline, not an afterthought. citeturn12search2turn12search1