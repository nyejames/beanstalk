import @html {center}
import @styles/docs {navbar, title, section, palette, styled_table, header, data, codeblock, theme_head}

page_title #= "Progress Matrix"
page_description #= "Current implementation status, coverage, and feature progress for the Beanstalk compiler."
page_head #= theme_head

red #= [$css("inline"): background-color: rgb(232, 109, 171)]
orange #= [$css("inline"): background-color: rgb(223, 162, 100)]
green #= [$css("inline"): background-color: rgb(109, 232, 138)]

#[navbar]
#[title, center: PROGRESS MATRIX]

#[section, $markdown:
    This page tracks Beanstalk's current implementation progress.

    It is the source of truth for what the compiler currently supports, partially supports,
    cleanly rejects, keeps experimental, defers, or treats as outside scope. It is not a full
    language reference, roadmap, or test manifest.

    Use this together with `tests/cases/manifest.toml` when changing language features.

    ## How to read and edit this page

    - Keep rows focused on current compiler behavior.
    - Put user-facing explanations and examples in docs pages under `docs/src/docs/**`.
    - Put future design plans and unimplemented ideas in `docs/roadmap/roadmap.md`.
    - Put compiler architecture and stage ownership in `docs/compiler-design-overview.md`.
    - Put contributor workflow, benchmarking, and tooling details in `CONTRIBUTING.md`.
    - Prefer short coverage summaries over long fixture lists unless fixture names identify a fragile surface.
    - Use the status labels from this page's legend. Put nuance such as removed, reserved,
      not Alpha, or backend-specific in the notes column instead of inventing new status labels.

    ## Alpha target

    The supported Alpha target is:

    - frontend parsing and diagnostics
    - typed AST construction
    - HIR lowering
    - borrow validation
    - JavaScript backend
    - HTML project builder
    - docs-style static site generation

    All tests should pass before each commit, so tests can be assumed green for the current state
    of the compiler unless this page explicitly says otherwise.
]

#[section, $markdown:
    # Status legend

    [styled_table:
        [header: [: Status] [: Meaning] ]

        [data, green:
            [: Supported]
            [: Implemented for the Alpha surface and expected to compile, type check, lower, and run through the JS / HTML pipeline.]
        ]

        [data, orange:
            [: Partial]
            [: A useful subset is implemented, but some sub-surfaces are intentionally deferred, target-gated, or still being shaped before Alpha.]
        ]

        [data, red:
            [: Experimental]
            [: Present in the repo, but not part of the Alpha stability promise.]
        ]

        [data, red:
            [: Deferred]
            [: Planned or design-compatible, but not part of the current implemented Alpha surface.]
        ]

        [data, red:
            [: Rejected]
            [: Not accepted by the compiler today and expected to fail with a structured diagnostic.]
        ]

        [data, red:
            [: Outside Scope]
            [: Intentionally not planned for Beanstalk's language design. Syntax should be rejected with structured diagnostics when encountered.]
        ]
    ]

    # Coverage legend

    [styled_table:
        [header: [: Coverage] [: Meaning] ]

        [data:
            [: Broad]
            [: Multiple focused cases exist, including success and failure coverage.]
        ]

        [data:
            [: Targeted]
            [: Dedicated coverage exists, but not across all important edges.]
        ]

        [data:
            [: Thin]
            [: Coverage exists indirectly or through only a small number of cases.]
        ]

        [data:
            [: None]
            [: No clear canonical coverage found.]
        ]
    ]
]

#[section, $markdown:
    # Core Alpha surface

    [styled_table:
        [header: [: Surface] [: Status] [: Coverage] [: Runtime target] [: Notes] ]

        [data:
            [: Entry files and implicit start]
            [: Supported]
            [: Broad entry-start, dependency-sort, non-entry top-level rejection, imported-start, and fragment-order coverage.]
            [: Frontend / HTML builder]
            [: Only module entry files run top-level runtime code. The implicit [$html:<code>start</code>] function is build-system-owned and not importable or callable as a user declaration.]
        ]

        [data:
            [: Structured diagnostics]
            [: Supported]
            [: Broad import, config, project-structure, syntax, type, rule, borrow, backend-feature, and warning coverage.]
            [: Frontend]
            [: User-facing failures use typed [$html:<code>CompilerDiagnostic</code>] payloads with stable codes. Infrastructure and invariant failures use [$html:<code>CompilerError</code>]. Type diagnostics are [$html:<code>TypeId</code>]-first at semantic boundaries.]
        ]

        [data:
            [: Variables, assignment, and scoped blocks]
            [: Supported]
            [: Broad declaration, reassignment, mutability, scope, shadowing, undefined-mutation, block-scope, and diagnostic coverage.]
            [: JS / HTML]
            [: Mutable writes require mutable places. [$html:<code>block:</code>] creates a lexical/control-flow scope. Bare labeled scopes are rejected.]
        ]

        [data:
            [: Control flow]
            [: Supported]
            [: Broad parser, HIR, integration, dispatcher, branch, loop, break, continue, and match lowering coverage.]
            [: JS / HTML]
            [: Branch-heavy CFG cases and loop exits remain important regression surfaces.]
        ]

        [data:
            [: Assertions]
            [: Supported]
            [: Broad parser, diagnostics, HIR lowering, backend artifact, and integration coverage.]
            [: Frontend / HIR / JS / HTML]
            [: [$html:<code>assert</code>] is a statement-only language intrinsic. It is always checked, unrecoverable, and accepts optional string-literal messages. [$html:<code>assert(false)</code>] is statically terminal.]
        ]

        [data:
            [: Functions and calls]
            [: Supported]
            [: Broad declaration, terminality, call validation, named and positional arguments, defaults, parameter passing, external calls, and return coverage.]
            [: Frontend / JS / HTML]
            [: Named calls and defaulted parameters are supported. Non-unit success-return functions are validated for all-path terminality before HIR. External and compiler-owned builtin calls remain positional-only.]
        ]

        [data:
            [: Default arguments and field defaults]
            [: Supported]
            [: Broad function parameter default, struct field default, named-call default skipping, constructor default skipping, and diagnostics coverage.]
            [: Frontend / JS / HTML]
            [: Function parameters and struct fields can define defaults with [$html:<code>=</code>]. Choice payload defaults, choice default variants, and broad default semantics outside function/struct construction remain deferred.]
        ]

        [data:
            [: Templates and style directives]
            [: Supported]
            [: Broad template parsing, directive, slot, const/runtime fragment, template control-flow, markdown inline-code, diagnostics, and JS/HTML integration coverage.]
            [: HTML / JS]
            [: Main Alpha surface. Supported shapes include const and runtime templates, slots/inserts, [$html:<code>$children</code>] / [$html:<code>$fresh</code>] routing, markdown/code/raw/html/css/escape directives, template Bool and option branching, template loops, runtime slot applications, and const expansion guards. Full match-style template heads and full CommonMark remain outside the current surface.]
        ]

        [data:
            [: Beandown [$html:<code>.bd</code>] content assets]
            [: Partial]
            [: Targeted compiler-integrated import, synthetic content constant, markdown folding, facade re-export, direct API, diagnostics, and integration coverage.]
            [: HTML builder / frontend]
            [: [$html:<code>.bd</code>] files are builder-supported source assets that synthesize one private compile-time [$html:<code>content #String</code>] constant from an implicit [$html:<code>$markdown</code>] template body. They import extensionlessly into [$html:<code>.bst</code>], can be re-exported by [$html:<code>#mod.bst</code>], and must fully fold. They are not entries, config files, standalone HTML files, or general source handlers.]
        ]

        [data:
            [: Plain Markdown [$html:<code>.md</code>] content assets]
            [: Supported]
            [: Broad extensionless import, grouped alias, facade re-export, parser options, raw HTML, literal links/images, collision, direct-extension, and entry-rejection coverage.]
            [: HTML builder / frontend]
            [: [$html:<code>.md</code>] files are plain Markdown content assets that render to HTML at compile time and synthesize one private [$html:<code>content #String</code>] constant. They do not parse Beanstalk imports, declarations, interpolation, templates, frontmatter, metadata, or same-directory facade scope. Links/images render literally and are not tracked assets in V1.]
        ]

        [data:
            [: Builder-supported source file kinds]
            [: Partial]
            [: Targeted Beandown and Markdown source-kind registry, extensionless resolution, unsupported-kind diagnostics, source discovery, and tokenizer/preparation coverage.]
            [: Stage 0 / frontend]
            [: Builders can declare non-[$html:<code>.bst</code>] source file kinds that participate in source import discovery and frontend preparation. The HTML builder implements Beandown [$html:<code>.bd</code>] and plain Markdown [$html:<code>.md</code>] content assets.]
        ]

        [data:
            [: Structs, records, and receiver methods]
            [: Supported]
            [: Broad parser, constructor, default, receiver, cross-file, borrow, and diagnostics coverage.]
            [: JS / HTML]
            [: Source-authored receiver methods are statically resolved and belong to the same file as their nominal receiver type. External packages do not expose receiver methods.]
        ]

        [data:
            [: Choices]
            [: Supported]
            [: Broad declaration, constructor, import, return, assignment, match, payload, JS carrier-shape, and structural equality coverage.]
            [: JS / HTML]
            [: Unit and record-payload variants are supported. Payload fields are immutable. Payload shorthand, direct payload field access, nested payload patterns, recursive choices, choice defaults, and tagged/default declarations remain deferred or rejected.]
        ]

        [data:
            [: Pattern matching]
            [: Partial]
            [: Broad literal, choice-match, payload-capture, relational pattern, guard, exhaustiveness, warning, and diagnostics coverage.]
            [: JS / HTML]
            [: Literal patterns, choice variants, payload extraction, relational patterns, guards, and [$html:<code>else =&gt;</code>] arms are supported. Wildcard patterns and no-op body placeholders are rejected. Full relational overlap analysis and nested choice payload patterns remain deferred.]
        ]

        [data:
            [: Collections]
            [: Supported]
            [: Broad literal, mutation, builtin method, indexing, loop, result, fixed-capacity, AST/HIR, JS helper, and diagnostics coverage.]
            [: JS / HTML]
            [: Growable [$html:<code>{T}</code>] and fixed [$html:<code>{N T}</code>] collections are distinct semantic types. Fixed capacities accept positive literals or bare visible [$html:<code>#Int</code>] constants. Collection [$html:<code>get</code>], [$html:<code>set</code>], [$html:<code>push</code>], and [$html:<code>remove</code>] are fallible, and [$html:<code>length</code>] is infallible.]
        ]

        [data:
            [: Hash maps]
            [: Supported]
            [: Broad frontend, HIR, borrow, JS, backend-validation, diagnostics, and integration coverage.]
            [: JS / HTML / backend validation for other targets]
            [: V1 supports insertion-ordered scalar-keyed maps with [$html:<code>{K = V}</code>] types, map literals, compiler-owned map members, HIR map operations, borrow validation, and JS runtime lowering. Unsupported key families, const maps, equality, language hashsets, and non-JS lowering details remain deferred or outside scope.]
        ]

        [data:
            [: Results, options, multiple returns, and multi-bind]
            [: Partial]
            [: Broad option, fallible return, postfix propagation, catch, value-producing block, multi-return, multi-bind, and diagnostic coverage.]
            [: JS / HTML]
            [: [$html:<code>Error!</code>], postfix [$html:<code>!</code>], [$html:<code>catch</code>], value-producing blocks, and multi-bind are supported. First-class public Result values and result-pattern matching remain deferred.]
        ]

        [data:
            [: Constants and compile-time folding]
            [: Supported]
            [: Broad top-level constant, dependency sorting, template folding, numeric folding, const-record, external constant, config, and diagnostics coverage.]
            [: Frontend]
            [: Top-level [$html:<code>#</code>] constants must fully fold. Same-file constant evaluation follows source order unless dependency sorting can order top-level declarations. Const records are compile-time field-access-only values.]
        ]

        [data:
            [: Type aliases]
            [: Supported]
            [: Broad builtin, imported, collection, option, generic instance, alias-to-alias, remap, and diagnostic coverage.]
            [: Frontend]
            [: Aliases are transparent and do not create nominal identity. Fully concrete generic aliases are supported. Parameterized aliases remain deferred.]
        ]

        [data:
            [: Arithmetic, explicit casts, and copy]
            [: Partial]
            [: Broad arithmetic, precedence, checked numeric, cast parser/AST/HIR/JS, const-folding, explicit copy, and diagnostics coverage.]
            [: JS / HTML]
            [: Runtime numeric operations lower through checked HIR operations. Explicit [$html:<code>cast</code>] uses compiler-owned builtin cast policies and evidence. Scalar constructor-style conversions are removed. Const [$html:<code>Error</code>] materialization, branching const catch handlers, and Wasm cast lowering remain deferred.]
        ]

        [data:
            [: Float formatting and boundary validation]
            [: Supported]
            [: Targeted stable formatting, finite-source, external-boundary, HIR statement, JS helper, and validator coverage.]
            [: JS / HTML]
            [: Beanstalk-owned Float formatting is used for AST folding, runtime template interpolation, and casts. External Float success returns are validated before ordinary Beanstalk code observes them.]
        ]

        [data:
            [: Type checking and coercion]
            [: Supported]
            [: Broad type compatibility, contextual promotion, strict expression typing, call argument, return, declaration, and diagnostics coverage.]
            [: Frontend]
            [: Strict expression typing stays separate from contextual declaration, return, parameter, template/string, and explicit-cast boundaries.]
        ]

        [data:
            [: Traits]
            [: Partial]
            [: Broad declaration, conformance, visibility, generic-bound receiver-call, trait-name rejection, core cast trait metadata, incompatibility metadata, facade leak, and diagnostics coverage.]
            [: Frontend static semantics]
            [: Static trait declarations, explicit same-file nominal conformances, reusable evidence, trait incompatibility metadata, and generic bounds are implemented. Dynamic trait values, trait aliases/composition, associated items, inheritance, generic traits/methods, conditional/specialized conformance, default methods, and standard trait taxonomy remain deferred or outside scope.]
        ]

        [data:
            [: String coercion and IO/template boundaries]
            [: Supported]
            [: Broad string-content coercion and IO/template rejection coverage, plus targeted external [$html:<code>StringContent</code>] signature coverage.]
            [: Frontend / JS]
            [: Template heads and reusable external string-content signatures accept supported string values. Console helpers such as [$html:<code>io.line(...)</code>] use the same boundary, so complex values must be extracted or interpolated into templates before output.]
        ]

        [data:
            [: Borrow validation and ownership analysis]
            [: Supported]
            [: Broad borrow, aliasing, mutable/exclusive access, use-after-move, branch merge, loop, short-circuit, lifetime, drop-site, hashmap, reactivity, and adversarial coverage.]
            [: HIR analysis / JS GC baseline]
            [: Borrow validation enforces exclusivity and move-safety while ownership/drop facts remain advisory for GC-only JS. GC fallback remains the language baseline.]
        ]

        [data:
            [: Paths and imports]
            [: Supported]
            [: Broad source import, namespace import, grouped import, external package, facade, module-boundary, collision, and project-structure coverage.]
            [: Frontend / HTML builder]
            [: Source and facade namespace records are shallow. External package namespace records support nested symbol paths such as [$html:<code>io.input.new</code>] and [$html:<code>io.input.Input</code>]. Direct symbol-path imports remain rejected. Cross-module and source-library access goes through [$html:<code>#mod.bst</code>] facades.]
        ]

        [data:
            [: External platform packages]
            [: Partial]
            [: Broad package import, external constant, external signature, provider-created JS package, runtime asset/glue, core package, prelude, and backend validation coverage.]
            [: Frontend / JS]
            [: Rust-side and provider-created external packages expose free functions, opaque types, and compile-time scalar constants. Project-local and builder-owned annotated [$html:<code>.js</code>] files can create JS-backed package surfaces. External packages do not expose receiver methods. Unsupported reachable calls for an active backend fail before lowering.]
        ]

        [data:
            [: HTML project builds]
            [: Supported]
            [: Broad routing, document shell, metadata, tracked asset, config, output cleanup, external JS glue, and page-fragment coverage.]
            [: HTML / JS]
            [: This is the release-critical project backend. HTML-Wasm behavior is tracked under experimental surfaces.]
        ]

        [data:
            [: Logical expressions]
            [: Supported]
            [: Broad precedence, grouping, invalid operand, not, comparison, short-circuit, and JS operator mapping coverage.]
            [: JS / HTML]
            [: Keep short-circuit behavior covered through artifact-shaped tests.]
        ]

        [data:
            [: If statements and conditions]
            [: Supported]
            [: Broad condition validation, nested condition, Bool requirement, same-line else-if rejection, and JS lowering coverage.]
            [: JS / HTML]
            [: Conditions require Bool unless the specific syntax is an option or match-style selector.]
        ]

        [data:
            [: Char]
            [: Supported]
            [: Broad success coverage with targeted diagnostic coverage.]
            [: JS / HTML]
            [: Additional malformed-char diagnostics are cleanup, not a core Alpha blocker.]
        ]

        [data:
            [: Named arguments and call-site mutability]
            [: Supported]
            [: Broad function-call, constructor-call, duplicate, unknown, missing, ordering, and tilde diagnostics coverage.]
            [: JS / HTML]
            [: Fresh rvalues can satisfy mutable parameters without [$html:<code>~</code>]. Existing places still require explicit [$html:<code>~</code>].]
        ]
    ]
]

#[section, $markdown:
    # Core and builder packages

    [styled_table:
        [header: [: Surface] [: Status] [: Coverage] [: Runtime target] [: Notes] ]

        [data:
            [: Core prelude]
            [: Supported]
            [: Targeted prelude symbol registration, namespace-alias plumbing, and collision rejection coverage.]
            [: Frontend / JS]
            [: Every builder provides [$html:<code>@core/prelude</code>]. The builtin prelude registers [$html:<code>io</code>] as a namespace alias to [$html:<code>@core/io</code>] and leaves compiler-owned [$html:<code>Error</code>] as a bare language symbol.]
        ]

        [data:
            [: Core IO package]
            [: Partial]
            [: Console helpers, prelude/explicit namespace imports, old callable rejection, removed public IO, input metadata/type-checking, HTML-JS input helper emission, and backend unsupported-call diagnostics.]
            [: Frontend / JS / HTML-Wasm validation]
            [: [$html:<code>@core/io</code>] exposes [$html:<code>io.print</code>], [$html:<code>io.line</code>], [$html:<code>io.debug</code>], [$html:<code>io.warn</code>], [$html:<code>io.error</code>], and [$html:<code>io.input.*</code>]. The prelude injects [$html:<code>io</code>] as a compile-time namespace alias, not a runtime value. Deferred IO domains such as filesystem, network, timers, event queues, targeted input sources, and non-JS lowerings belong in the roadmap.]
        ]

        [data:
            [: Core math package]
            [: Supported]
            [: Targeted direct, grouped, aliased, namespace, const, arity, type, missing-symbol, and non-imported access coverage.]
            [: JS / HTML]
            [: [$html:<code>@core/math</code>] exposes Float math functions and scalar constants. It is explicit-import only. Non-JS support is backend-specific and tracked under experimental surfaces.]
        ]

        [data:
            [: Core text package]
            [: Partial]
            [: Targeted runtime, JS helper, arity, type-error, edge-case, unsupported-builder, and Wasm-unsupported coverage.]
            [: JS / HTML]
            [: [$html:<code>@core/text</code>] exposes string helpers such as length, emptiness, containment, prefix, and suffix checks. Receiver-method forms and non-JS lowerings remain deferred.]
        ]

        [data:
            [: Core random package]
            [: Partial]
            [: Targeted import, JS helper, runtime smoke, arity, type-error, and Wasm-unsupported coverage.]
            [: JS / HTML]
            [: [$html:<code>@core/random</code>] exposes random Float and inclusive random Int helpers. Seeded random and non-JS lowerings remain deferred.]
        ]

        [data:
            [: Core time package]
            [: Partial]
            [: Targeted import, grouped alias, namespace, helper, runtime smoke, fallible parse/catch, arity/type diagnostic, old-name rejection, opaque-field rejection, and backend validation coverage.]
            [: JS / HTML]
            [: [$html:<code>@core/time</code>] exposes opaque Duration, TimeMark, and Timestamp values plus the first JS-backed time helper slice. Calendar/time-zone APIs, timers, animation scheduling, and non-JS lowerings remain deferred.]
        ]

        [data:
            [: Source library roots]
            [: Partial]
            [: Targeted AST-backed config loading, known-key and shape enforcement, config support declarations, source-library imports, project-local/relative config import rejection, library root discovery, collision detection, and facade enforcement coverage.]
            [: Frontend / HTML]
            [: Source libraries and regular modules share visibility and facade rules. Authored config entries are folded known top-level [$html:<code>#</code>] constants in [$html:<code>#config.bst</code>]. Import/type support declarations are accepted, while plain config bindings and trait metadata are rejected. Config imports may target core/builder libraries only.]
        ]

        [data:
            [: Builder-provided source libraries]
            [: Partial]
            [: Targeted builder source-library root registration and HTML builder library coverage.]
            [: Frontend / HTML]
            [: Builders expose source libraries through [$html:<code>LibrarySet.source_libraries</code>]. The HTML builder provides [$html:<code>@html</code>] as a source library. Builder and project-local prefix collisions are hard config errors.]
        ]

        [data:
            [: Project-local libraries]
            [: Partial]
            [: Targeted config-defined library folders, import resolution, and facade-gating coverage.]
            [: Frontend / HTML]
            [: Project-local source libraries are discovered from [$html:<code>library_folders</code>], with `/lib` as the default convention. Each direct child folder becomes an import prefix. Public cross-module access goes through [$html:<code>#mod.bst</code>].]
        ]

        [data:
            [: Builder runtime packages]
            [: Partial]
            [: Targeted built-in JS-backed package, runtime asset, runtime module, import-map, glue, and backend validation coverage.]
            [: HTML / JS]
            [: Builder-owned runtime packages such as [$html:<code>@web/canvas</code>] share the external package registry and HTML external JS asset/glue pipeline with provider-created imports.]
        ]
    ]
]

#[section, $markdown:
    # Compiler-owned builtin surfaces

    [styled_table:
        [header: [: Surface] [: Status] [: Coverage] [: Runtime target] [: Notes] ]

        [data:
            [: Collection methods]
            [: Supported]
            [: Broad JS helper, fallible get/set/push/remove, infallible length, removed indexed-write, bounds, and mutable-receiver coverage.]
            [: JS / HTML]
            [: Collection get, set, push, remove, and length are compiler-owned members. Mutating calls require explicit mutable receiver access, and fallible members require [$html:<code>!</code>] or [$html:<code>catch</code>] handling.]
        ]

        [data:
            [: Error helper methods]
            [: Rejected]
            [: Public helper-call removal coverage. Hidden JS metadata helper coverage remains backend-internal.]
            [: Frontend / JS runtime internal]
            [: Public [$html:<code>err.bubble()</code>], location, trace, and [$html:<code>@core/error</code>] helper calls are not exposed.]
        ]

        [data:
            [: Receiver methods]
            [: Supported]
            [: Broad same-file struct/choice receiver, aligned generic receiver, imported receiver type, namespace receiver type, facade exposure, nonlocal/builtin/external receiver rejection, duplicate, collision, and mutability coverage.]
            [: JS / HTML]
            [: Source-authored receiver methods travel with receiver type visibility. They are never grouped-imported, aliased, or exposed as namespace-record fields.]
        ]

        [data:
            [: Fallible suffix handling]
            [: Supported]
            [: Broad propagation, shared then catch recovery, catch handler, template-boundary, explicit HIR branch, direct-return, call-argument, receiver-base, match-guard, and obsolete HIR propagation-expression removal coverage.]
            [: JS / HTML]
            [: Fallible calls lower through explicit success/error HIR control flow. Value-producing catch recovery uses shared value-block result locals. Collection and map fallible members use the same path.]
        ]

        [data:
            [: Builtin Error type]
            [: Supported]
            [: Constructor, default-field, named-argument, public field, and backend-generated Error coverage.]
            [: Frontend / JS]
            [: Public fields are [$html:<code>message</code>] and [$html:<code>code</code>]. Hidden location/trace metadata remains backend-internal.]
        ]
    ]
]

#[section, $markdown:
    # Reactivity V1

    [styled_table:
        [header: [: Surface] [: Status] [: Coverage] [: Current surface] ]

        [data:
            [: Reactive declarations and parameters]
            [: Partial]
            [: Frontend syntax, AST/HIR metadata, borrow invalidation facts, JS lowering, HTML-JS mounting, and unsupported sink/backend diagnostics.]
            [: V1 supports [$html:<code>$Type</code>] / [$html:<code>$=</code>] local declarations and [$html:<code>$T</code>] parameters as source metadata, not wrapper [$html:<code>TypeId</code>]s. Reactive declarations are runtime-local and not importable/exportable top-level declarations.]
        ]

        [data:
            [: Reactive template subscriptions]
            [: Partial]
            [: Template-head parsing, source validation, subscription metadata, AST/HIR propagation, reachability, borrow invalidation, JS helper lowering, HTML-JS mounting, and unsupported sink/backend diagnostics.]
            [: V1 accepts one bare reactive source identifier in template head/capture positions through [$html:<code>$(source)</code>]. Snapshot reads remain ordinary captures. Field paths, calls, computed expressions, ordinary expression use, and compile-time templates are rejected.]
        ]

        [data:
            [: HTML-JS reactive runtime fragments]
            [: Partial]
            [: JS reactive source, scheduler, template-string runtime helper, slot mounting/rerender registration, and unsupported sink coverage.]
            [: V1 live sink support is top-level runtime HTML fragments in the HTML-JS builder. Fine-grained DOM updates, nested live regions, keyed diffing, and other UI runtime strategies are deferred.]
        ]

        [data:
            [: Reactive follow-ups]
            [: Deferred]
            [: No accepted syntax beyond V1 source subscriptions.]
            [: Deferred surfaces include reactive-aware template control flow, field/path subscriptions, collection item subscriptions, expression dependency tracking, derived reactive values, live IO sinks, template-owned event/action/effect syntax, [$html:<code>$bind</code>], typed component messages, and HTML-Wasm reactive runtime support.]
        ]
    ]
]

#[section, $markdown:
    # Experimental backend surfaces

    [styled_table:
        [header: [: Surface] [: Status] [: Current role] [: Alpha rule] ]

        [data:
            [: HTML-Wasm]
            [: Experimental]
            [: Backend experiment and integration path for future Wasm output.]
            [: Keep behind [$html:<code>--html-wasm</code>]. Tests may pin current behavior, but Alpha support is JS / HTML.]
        ]

        [data:
            [: Wasm backend]
            [: Experimental]
            [: Long-term backend direction with current HIR-to-Wasm-LIR lowering, runtime contracts, request validation, debug output, and binary emission work.]
            [: Do not count Wasm support as an Alpha blocker except for clean diagnostics and no-panic guarantees on covered paths.]
        ]

        [data:
            [: Wasm payload/runtime parity]
            [: Experimental]
            [: Covered paths validate or lower selected payload, assert, generic, map, collection, numeric, cast, reactive, and external-call cases.]
            [: Backend parity progress belongs here, not repeated in every feature row.]
        ]
    ]
]

#[section, $markdown:
    # Deferred, rejected, and outside-scope surfaces

    [styled_table:
        [header: [: Surface] [: Status] [: Coverage / diagnostic direction] [: Notes] ]

        [data:
            [: Trait ecosystem extensions]
            [: Deferred]
            [: Current implemented trait diagnostics are tracked in the core Traits row.]
            [: Default methods, associated items, inheritance/composition, generic traits, specialized conformance, deriving, operator traits, formatting/display coercion, hashing, ordering, iteration, serialization, downcasting/reflection, and broad standard trait taxonomy remain outside the Alpha trait slice.]
        ]

        [data:
            [: Async / concurrency scopes]
            [: Deferred]
            [: Reserved keyword diagnostics for async statement/declaration positions.]
            [: Future async syntax is design-only. Current async statement and declaration positions are rejected with structured diagnostics.]
        ]

        [data:
            [: Checked scopes]
            [: Deferred]
            [: Reserved keyword diagnostics for checked statement/declaration positions.]
            [: Checked/effect semantics are not designed yet.]
        ]

        [data:
            [: Labeled scopes]
            [: Rejected]
            [: Targeted labeled-scope diagnostic coverage.]
            [: Bare [$html:<code>label:</code>] syntax is not supported. Use [$html:<code>block:</code>] for ordinary scoped blocks.]
        ]

        [data:
            [: Legacy and foreign-language syntax]
            [: Rejected]
            [: Targeted diagnostics for common TypeScript/Rust/JS-shaped mistakes.]
            [: Compatibility syntax is not planned Alpha surface.]
        ]

        [data:
            [: Public panic/recover syntax]
            [: Rejected]
            [: Unknown-name and assert result-handling diagnostics.]
            [: There is no public panic keyword, [$html:<code>#panic</code>] directive, recover, catch_panic, or public panic payload. Expected failures use [$html:<code>Error!</code>] and [$html:<code>catch</code>].]
        ]

        [data:
            [: Assertion extensions]
            [: Deferred]
            [: Current assert diagnostics and backend trap coverage.]
            [: Deferred surfaces include debug-only assertions, runtime/const assertion-message expressions, additional explicit stop helpers, catchable panic semantics, and richer non-JS trap payloads.]
        ]

        [data:
            [: Choice payload field access]
            [: Deferred]
            [: Targeted direct-field-access diagnostic coverage.]
            [: Use pattern matching to extract payload fields until variant narrowing/refinement is designed.]
        ]

        [data:
            [: Choice nested payload patterns and richer exhaustiveness]
            [: Deferred]
            [: Targeted nested-pattern deferred diagnostics.]
            [: Exhaustiveness is tag-level for now. General pattern binding renames are also deferred outside choice payload capture renames.]
        ]

        [data:
            [: Choice defaults and tagged declarations]
            [: Deferred]
            [: Targeted diagnostics for choice defaults/default variants where present.]
            [: Variant defaults, default payload fields, and tagged/default declaration sugar remain deferred.]
        ]

        [data:
            [: Choice recursive types]
            [: Deferred]
            [: Targeted recursive declaration deferred diagnostics.]
            [: Requires layout and indirection design.]
        ]

        [data:
            [: `as` keyword domains]
            [: Supported]
            [: Targeted success and invalid-position diagnostics.]
            [: [$html:<code>as</code>] is valid for type aliases, namespace/grouped import aliases, and choice payload capture renames. Other positions are rejected.]
        ]

        [data:
            [: Explicit facade export/re-export syntax]
            [: Supported]
            [: Broad authored export, grouped re-export, alias, source/external symbol, privacy, duplicate, and placement coverage.]
            [: [$html:<code>#mod.bst</code>] exposes public authored declarations with [$html:<code>export</code>] and public grouped re-exports with [$html:<code>export import</code>] or [$html:<code>export @path { ... }</code>]. Bare namespace exports, wildcard exports, function alias values, and automatic method re-export through type aliases remain deferred.]
        ]

        [data:
            [: Source-library HIR caching]
            [: Deferred]
            [: No precompiled source-library cache.]
            [: Source libraries are compiled from source each build. Caching is deferred until the library system is stable.]
        ]

        [data:
            [: Package manager]
            [: Deferred]
            [: No package manager, remote registries, lockfiles, or dependency resolver.]
            [: Package management, versioning, lockfiles, and remote fetching remain roadmap work.]
        ]

        [data:
            [: External JS library expansion]
            [: Deferred]
            [: JS provider diagnostics reject unsupported public library forms.]
            [: The HTML builder supports annotated single-file [$html:<code>.js</code>] imports and builder-owned JS-backed packages. JS constants, classes, dependency graphs, default exports, re-exports, CommonJS, callbacks, async functions, generic external types, collection/option signatures, and multi-success returns remain deferred.]
        ]

        [data:
            [: Namespace / wildcard imports]
            [: Partial]
            [: Source, facade, external package, nested external value/type traversal, and wildcard rejection coverage.]
            [: Namespace imports are compile-time records. Source/facade records stay shallow. External package records can expose nested symbol paths such as [$html:<code>io.input.new</code>] and [$html:<code>io.input.Input</code>]. Wildcard imports remain deferred.]
        ]

        [data:
            [: Generic type infrastructure]
            [: Supported]
            [: Broad TypeId, nominal instance, alias, constructor, import, HIR validation, trait-bound, and diagnostic coverage.]
            [: Generic structs, choices, concrete aliases, generic free functions, aligned generic receiver methods, and trait-bound validation use canonical [$html:<code>TypeId</code>] identity before HIR.]
        ]

        [data:
            [: Generic functions]
            [: Partial]
            [: Broad declaration, body validation, inference, instantiation, import/facade, diagnostics, fallible return, and non-value-use coverage.]
            [: Generic free functions infer from immediate call arguments and immediate expected result context only. They emit concrete instances before HIR and are not first-class values.]
        ]

        [data:
            [: Trait bounds on generics]
            [: Partial]
            [: Broad declaration-site bound, visible evidence, facade privacy, receiver-call, nominal generic instance, and negative diagnostic coverage.]
            [: Bounds use [$html:<code>is</code>]. Concrete generic calls and nominal generic instances require visible reusable evidence. File-local evidence-backed dispatch remains outside scope.]
        ]

        [data:
            [: Explicit generic call-site application]
            [: Rejected]
            [: Targeted diagnostics for of, angle, square, and typed-argument call-site forms.]
            [: Generic calls use ordinary call syntax and inference only. Use annotations at receiving sites when inference needs help.]
        ]

        [data:
            [: Recursive generic types]
            [: Deferred]
            [: Targeted recursive generic type diagnostics.]
            [: Requires layout and indirection design.]
        ]

        [data:
            [: Nested generic type application]
            [: Deferred]
            [: Targeted nested [$html:<code>of</code>] diagnostics and alias workaround coverage.]
            [: Name intermediate concrete aliases instead of writing nested inline generic applications.]
        ]

        [data:
            [: Parameterized generic aliases]
            [: Deferred]
            [: Targeted rejection where covered.]
            [: Aliases to fully concrete generic instances are supported. Aliases with their own parameters remain deferred.]
        ]

        [data:
            [: Generic external package functions]
            [: Deferred]
            [: External signature validation rejects unsupported generic external surfaces.]
            [: External packages currently expose concrete functions, opaque types, and constants only.]
        ]

        [data:
            [: Type values and type-level compile-time evaluation]
            [: Outside Scope]
            [: Invalid syntax or ordinary name/type diagnostics.]
            [: Type-returning functions, runtime type IDs, type-level conditionals, and compile-time type inspection are outside the current language design.]
        ]

        [data:
            [: General function-value surfaces]
            [: Outside Scope]
            [: Generic function value diagnostics and invalid syntax diagnostics where covered.]
            [: General closures, anonymous function values, generic function values, higher-order polymorphism, and hidden event-handler function values are outside the current language design.]
        ]

        [data:
            [: Rich numeric redesign]
            [: Deferred]
            [: Current numeric coverage is tracked in the core arithmetic and Float rows.]
            [: Numeric check elision, explicit unchecked numeric modes, narrower numeric types, BigInt, Decimal, and broader numeric tower work remain deferred.]
        ]
    ]
]

#[section, $markdown:
    # Maintenance rule

    This matrix should stay concise and current.

    - Move workflow and benchmark details to `CONTRIBUTING.md`.
    - Move future sequencing and design plans to `docs/roadmap/roadmap.md`.
    - Move syntax examples and beginner explanations to docs pages under `docs/src/docs/**`.
    - Move compiler architecture details to `docs/compiler-design-overview.md`.
    - Keep this page focused on support status, coverage level, clean rejection, target-gating, and experimental status.
]
