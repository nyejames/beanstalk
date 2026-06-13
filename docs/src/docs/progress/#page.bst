import @html {center}
import @styles/docs {navbar, title, section, palette, styled_table, header, data, codeblock, theme_head}

page_title #= "Implementation Matrix"
page_description #= "Current implementation and coverage status for the Beanstalk compiler."
page_head #= theme_head

red #= [$css("inline"): background-color: rgb(232, 109, 171)]
orange #= [$css("inline"): background-color: rgb(223, 162, 100)]
green #= [$css("inline"): background-color: rgb(109, 232, 138)]

#[navbar]
#[title, center: IMPLEMENTATION MATRIX]

#[section, $markdown:
    This page tracks the current implemented compiler surface.

    It is an implementation matrix, not a full language specification.
    The wider language docs may describe intended or aspirational syntax.
    This page should describe what the compiler currently supports, rejects cleanly, or keeps experimental.

    Use this together with `tests/cases/manifest.toml` when changing language features.

    ## Alpha target

    The supported Alpha target is:

    - frontend parsing and diagnostics
    - typed AST construction
    - HIR lowering
    - borrow validation
    - JavaScript backend
    - HTML project builder
    - docs-style static site generation

    A policy for the repo is all tests must pass before each commit.
    So all tests on all platforms can be assumed to be green for the current state of the compiler unless specifically noted here.

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
            [: A useful subset is implemented, but some sub-surfaces are intentionally deferred or still being shaped before Alpha.]
        ]

        [data, red:
            [: Experimental]
            [: Present in the repo, but not part of the Alpha stability promise.]
        ]

        [data, red:
            [: Deferred]
            [: Not part of Alpha. Syntax should either be reserved or rejected with structured diagnostics.]
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
        [header: [: Surface] [: Status] [: Coverage] [: Runtime target] [: Watch points] ]

        [data:
            [: Entry files and implicit start]
            [: Supported]
            [: Broad entry-start, dependency-sort, non-entry top-level rejection, imported-start, and fragment-order coverage.]
            [: Frontend / HTML builder]
            [: Only the module entry file runs top-level runtime code. Non-entry top-level executable code and runtime templates are rejected. The implicit [$html:<code>start</code>] function is build-system-owned and is not an importable/callable user declaration.]
        ]

        [data:
            [: Structured diagnostics]
            [: Supported]
            [: Import, import path resolution, compile-time path literal validation, name-resolution, semantic-rule, type, borrow, deferred-feature, unsupported external-package/backend-feature validation, Stage 0 config parsing/value validation, Stage 0 project-structure validation, HTML project config validation, and AST expression/template diagnostics use typed CompilerDiagnostic payloads with stable codes. Infrastructure failures use a dedicated stable infrastructure code, and warnings share the same ordered diagnostic stream.]
            [: Frontend]
            [: Do not add legacy user-diagnostic macros, rendered-string payloads, or user-facing CompilerError routes. AST and borrow source failures use [$html:<code>CompilerDiagnostic</code>]; HIR keeps [$html:<code>CompilerError</code>] / [$html:<code>ErrorType::HirTransformation</code>] for invariant failures only. Local fallible compiler-stage helpers and diagnostic boundary enums may box large [$html:<code>CompilerDiagnostic</code>] / [$html:<code>CompilerError</code>] payloads to keep Result boundaries small, but they unbox only at [$html:<code>DiagnosticBag</code>], [$html:<code>CompilerMessages</code>], or render boundaries so this remains an internal transport detail rather than a user-facing diagnostic-shape change. Executable AST and HIR call/result typing is [$html:<code>TypeId</code>]-first, with [$html:<code>DataType</code>] retained only for parse and diagnostic spelling. Frontend boundary cleanup requires manual review of AST/HIR ownership, diagnostic routing, and type-representation paths. Remaining future work is limited to JSON diagnostic output, LSP mapping, localization, and any new diagnostics required by future language features.]
        ]

        [data:
            [: Variables, assignment, and scoped blocks]
            [: Supported]
            [: Broad declaration, reassignment, mutability, scope, shadowing, undefined-mutation, block-scope, and diagnostics coverage.]
            [: JS / HTML]
            [: Mutable writes require mutable places. [$html:<code>block:</code>] creates a lexical/control-flow scope. Bare labeled scopes remain rejected.]
        ]

        [data:
            [: Control flow]
            [: Supported]
            [: Broad parser, HIR, integration, dispatcher, and structured-lowering coverage.]
            [: JS / HTML]
            [: Keep loops, break / continue, match lowering, and branch-heavy CFG cases covered.]
        ]

        [data:
            [: Assertions]
            [: Supported]
            [: Broad parser, diagnostics, HIR lowering, backend artifact, and integration coverage.]
            [: Frontend / HIR / JS / HTML; Wasm trap lowering]
            [: [$html:<code>assert</code>] is a language-owned statement keyword and cannot be shadowed, imported, assigned, passed, or used in expression position. [$html:<code>assert(condition)</code>] and [$html:<code>assert(condition, "message")</code>] are always checked. Failed assertions are unrecoverable and cannot be caught. [$html:<code>assert(false, "message")</code>] is the only current explicit source-level panic path and is statically terminal for value-required functions and [$html:<code>catch</code>] handlers. Expected failures should use [$html:<code>Error!</code>] and [$html:<code>catch</code>]. Messages are optional string literals; runtime and compile-time constant message expressions remain deferred.]
        ]

        [data:
            [: Functions and calls]
            [: Supported]
            [: Broad function declaration, call validation, named/positional argument, default-argument, parameter passing, external-call, and return coverage.]
            [: JS / HTML]
            [: Keep named, positional, and default-argument call semantics strict. External and builtin calls remain positional-only.]
        ]

        [data:
            [: Default arguments and field defaults]
            [: Supported]
            [: Function parameter defaults, struct field defaults, named-call default skipping, constructor default skipping, all-default struct constructors, builtin [$html:<code>Error.code</code>] defaulting, and default diagnostics are covered.]
            [: Frontend / JS / HTML]
            [: Function parameters and struct fields can define defaults with [$html:<code>=</code>]. Named calls and constructors can omit defaulted slots, including earlier defaulted slots when later required slots are supplied by name. Choice payload defaults, choice default variants, and broad default-argument semantics outside function/struct construction remain deferred.]
        ]

        [data:
            [: Templates and style directives]
            [: Supported]
            [: Broad template parsing, slot, directive, const-fragment, and runtime-fragment coverage, with targeted unit coverage for template head suffix control-flow parsing and diagnostics, strict EOF diagnostics for unclosed template heads, bodies, control-flow bodies, nested children, and directive-argument templates, [$html:<code>TypeId</code>]-based template-head type validation for aliases, generic instances, const records, and external opaque values, const Bool [$html:<code>if</code>] folding including dependency-sorted source const references, runtime Bool [$html:<code>if</code>] HIR lowering including expression-level [$html:<code>!</code>] propagation, const option-present [$html:<code>if</code>] folding for already-decidable option values and source const scrutinees, runtime option-present [$html:<code>if</code>] HIR lowering, [$html:<code>["[else]"]</code>], [$html:<code>["[else if ...]"]</code>], grammar-boundary diagnostics for unsupported match-style template heads, [$html:<code>case</code>] as an ordinary identifier, const conditional loops, streamed const range loops with float-step, inclusive/exclusive endpoint, and exact-limit coverage, const collection loops, runtime conditional / range / collection loop HIR lowering, shared const/runtime aggregate render-plan preparation for loop head wrapping, runtime slot applications with branch, loop, loop-control in append contexts, shared compile-time/runtime child-contribution classification for [$html:<code>$children(...)</code>] / [$html:<code>$fresh</code>] wrapper routing, repeated slot single-evaluation/source-site behavior, nested slot sites, mixed static/runtime ordering, [$html:<code>$children(...)</code>], [$html:<code>$fresh</code>], literal template-body backslash/backtick preservation, string-inserted literal delimiters, and [$html:<code>$markdown</code>] isolated same-line single-backtick inline code spans rendered as plain [$html:<code>code</code>] elements.]
            [: HTML / JS]
            [: This is one of the main Alpha surfaces. Template head suffix control flow is supported for source-authored Bool [$html:<code>if</code>] and option-present [$html:<code>if</code>] branching, conditional [$html:<code>loop</code>], range [$html:<code>loop</code>], collection [$html:<code>loop</code>], standalone [$html:<code>["[else]"]</code>], standalone [$html:<code>["[else if ...]"]</code>] branch chains, and structural [$html:<code>["[break]"]</code>] / [$html:<code>["[continue]"]</code>] loop-control sentinels in runtime-capable contexts. Template heads do not support full pattern-match branch chains; use ordinary statement/value [$html:<code>if value is:</code>] blocks outside templates for that shape. [$html:<code>case</code>] is not a keyword or template sentinel and remains valid as an ordinary identifier. Const-required Bool [$html:<code>if</code>], already-decidable option-present [$html:<code>if</code>], [$html:<code>["[else if ...]"]</code>] branch chains, conditional [$html:<code>loop</code>] with a compile-time false condition, range [$html:<code>loop</code>], collection [$html:<code>loop</code>], and structural loop-control sentinels inside const range/collection loop bodies fold during AST construction. Const range loops stream counters while enforcing the const expansion guard, checked integer stepping, explicit float steps, non-finite / zero-step rejection, and non-progressing float-step rejection. Const-required template [$html:<code>if</code>] conditions, option scrutinees and conditional-loop conditions can reference dependency-sorted source constants, with same-file forward references still rejected by the header dependency pass. Runtime slot applications support default, named, positional, loose contribution routing, repeated slots, missing slots as empty, [$html:<code>$insert(...)</code>], runtime branch/loop contributions, loop-control propagation inside template loop append contexts, source evaluation once with slot-site replay, site-local [$html:<code>$children(...)</code>] / [$html:<code>$fresh</code>] wrapper behavior, and mixed static/runtime contribution ordering. Template-body backticks and backslashes are formatter-visible text, and literal template delimiters should be inserted through strings. [$html:<code>$markdown</code>] supports same-line isolated single-backtick inline code spans, but CommonMark fenced code blocks, variable-length delimiters, multiline code spans, and markdown-level backtick escaping are intentionally outside Beanstalk markdown. Supported paths preserve structural no-output semantics, lazy runtime branch/loop lowering, nearest-template-loop [$html:<code>["[break]"]</code>] / [$html:<code>["[continue]"]</code>] targeting, output-before-loop-control preservation, expression-level [$html:<code>!</code>] propagation, runtime slot/insert leakage diagnostics for invalid helper artifacts, literal-body [$html:<code>["[else]"]</code>] / [$html:<code>["[else if ...]"]</code>] / loop-control rejection, malformed/orphan/inline sentinel diagnostics, and the [$html:<code>template_const_loop_iteration_limit</code>] const expansion guard with a default of [$html:<code>10_000</code>] iterations and a hard maximum of [$html:<code>1_000_000</code>].]
        ]

        [data:
            [: Beandown [$html:<code>.bd</code>] content assets]
            [: Partial]
            [: Targeted unit coverage for builder-declared source-kind registration, extensionless path resolution, Beandown token entry behavior, synthetic [$html:<code>content #String</code>] headers, literal template-body backslash preservation, ordinary outer template close rejection, string-inserted delimiter output, markdown folding, compile-time template [$html:<code>if</code>] and collection [$html:<code>loop</code>] folding, non-foldable condition, runtime-call, external/prelude-call, and dynamic-loop rejection, explicit facade re-export syntax, grouped and namespace [$html:<code>content</code>] imports into normal [$html:<code>.bst</code>] constants, namespace-record constant dependency sorting, flat [$html:<code>@html</code>] constant visibility, same-directory facade constant visibility, const-record field projection, override ordering, non-constant filtering, facade-supplied [$html:<code>content</code>] references, self-originating re-export exclusion, no-runtime/start header and AST behavior for imported [$html:<code>.bd</code>] files, and direct API file, directory, recursive directory, file-list, in-memory source, duplicate-input, invalid caller-scope, and no-artifact behavior. Integration coverage now includes HTML namespace imports, grouped aliases, explicit facade re-exports, source-library imports, unimported [$html:<code>.bd</code>] files, direct [$html:<code>.bd</code>] entry rejection, direct [$html:<code>.bd</code>] extension-import rejection, [$html:<code>.bd</code>] import ambiguities, body-scope runtime/type/API rejection, unescaped outer-close diagnostics, and HTML-Wasm compile-path parity.]
            [: HTML builder / frontend]
            [: [$html:<code>.bd</code>] is implemented as a builder-supported source kind that synthesizes a private compile-time [$html:<code>content #String</code>] constant from an implicit [$html:<code>$markdown</code>] template body. Beandown bodies follow normal template-body and [$html:<code>$markdown</code>] semantics: backslashes and backticks are body text, literal delimiters use string insertion, and inline code follows the same isolated single-backtick rule. Current support covers source-kind discovery, Beandown entry tokenization, synthetic header preparation, dependency-sorted AST folding, normal HTML project imports, crate-local HTML-project direct API compilation, user-facing documentation, and final redundancy/output-artifact auditing. [$html:<code>@html</code>] exported constants and same-directory [$html:<code>#mod.bst</code>] exported constants are visible flat in compiler-integrated Beandown bodies, const-record field projection folds through the existing AST template-head path, grouped imports and namespace imports such as [$html:<code>intro.content</code>] fold into normal [$html:<code>.bst</code>] constants, functions/types/runtime APIs are not visible, generated self [$html:<code>content</code>] is excluded, recognized source-asset entries such as direct [$html:<code>.bd</code>] page builds are rejected with a dedicated diagnostic, and direct API duplicate/caller-scope mistakes use dedicated stable diagnostics. Direct caller-supplied folded scope conversion, CLI [$html:<code>bean beandown</code>], [$html:<code>project = "beandown"</code>], generated standalone HTML files, frontmatter/metadata/raw-source retention, and generic Markdown/CSS/JSON source handlers remain deferred.]
        ]

        [data:
            [: Builder-supported source file kinds]
            [: Partial]
            [: Targeted coverage through Beandown source-kind registry, extensionless resolution, unsupported-kind diagnostics, explicit source-extension diagnostics, direct-entry rejection, ambiguity handling, source discovery, and tokenizer entry-mode tests.]
            [: Stage 0 / frontend]
            [: Builders can declare non-[$html:<code>.bst</code>] source file kinds that participate in source import discovery and frontend preparation. V1 implements only Beandown [$html:<code>.bd</code>] for the HTML builder. This is not a general source-handler plugin system yet; future Markdown/CSS/JSON handlers remain deferred until each has a concrete semantic owner.]
        ]

        [data:
            [: Structs, records, and receiver methods]
            [: Supported]
            [: Broad integration coverage with targeted parser, struct-default, constructor-default, and receiver-catalog unit coverage.]
            [: JS / HTML]
            [: Field mutation, nested field mutation, nominal mismatch, constructor named arguments, struct field defaults, receiver borrow behavior, compiler-owned scalar receiver behavior, and cross-file receiver use are covered. Keep Wasm receiver behavior experimental.]
        ]

        [data:
            [: Choices]
            [: Supported]
            [: Broad parser, declaration, constructor, match, import, assignment, return, JS carrier-shape, payload coverage, unit equality, and payload structural equality coverage.]
            [: JS / HTML]
            [: Unit variants, payload variants, constructor calls, imports, assignment, return, JS carrier shape, payload matching, receiver methods, and structural equality are supported. Structural equality compares variant identity and payload fields recursively when every payload field supports equality. Payload fields are immutable. Payload shorthand, generic choices, recursive choices, nested payload patterns, direct payload field access, and variant defaults remain deferred or rejected with diagnostics.]
        ]

        [data:
            [: Pattern matching]
            [: Partial]
            [: Broad literal, choice-match, and payload-capture coverage, including exhaustiveness and diagnostics.]
            [: JS / HTML]
            [: Literal patterns, choice variant patterns, payload extraction, relational patterns including strings, guards, [$html:<code>else =&gt;</code>] arms, empty [$html:<code>else =&gt;</code>] no-op fallback arms, and general capture patterns are supported. Wildcard patterns using [$html:<code>_</code>] and no-op bodies such as [$html:<code>else =&gt; _</code>] are permanently rejected; use [$html:<code>else =&gt;</code>] for fallback and an empty [$html:<code>else =&gt;</code>] to do nothing. Obvious unreachable arms warn. Full relational overlap analysis and nested choice payload patterns remain deferred.]
        ]

        [data:
            [: Collections]
            [: Supported]
            [: Broad collection literal, mutation, builtin method, indexing, loop, result, and JS helper coverage. New capacity parsing, header dependency-ordering, AST capacity-folding, fixed literal declaration, HIR fixed [$html:<code>TypeId</code>] validation, JS fixed wrapper lowering, fixed-capacity runtime enforcement, and fallible [$html:<code>push</code>] coverage.]
            [: JS / HTML]
            [: Collections internally use the generic type infrastructure. Public growable syntax remains [$html:<code>{T}</code>]. Fixed collection syntax [$html:<code>{N T}</code>] and declaration-target shorthand [$html:<code>{N}</code>] are now parsed into collection type syntax, with old post-element capacity syntax such as [$html:<code>{T N}</code>] rejected. Fixed capacity in type position is limited to a positive integer literal or bare visible [$html:<code>#Int</code>] constant name; arithmetic and other expressions must be placed in a named compile-time constant first. Bare capacity constants in top-level type surfaces participate in existing header dependency sorting, so referenced same-file constants can be ordered before declarations that use them. Parsed fixed-capacity annotations now fold during AST type resolution for declarations, multi-bind targets, function parameters and returns, type aliases, struct fields, and choice payloads, producing canonical collection [$html:<code>TypeId</code>] identity with fixed capacity preserved through alias expansion where source visibility is available. Fixed collection literals now use the receiving collection shape, reject over-capacity initializers, preserve fixed shape through aliases, nested literals, and generic identity, and support declaration-target capacity shorthand only for immediate non-empty collection literals. Immutable empty fixed value bindings are rejected while mutable empty fixed bindings and fixed struct field defaults remain accepted. Collection [$html:<code>get</code>], [$html:<code>set</code>], [$html:<code>push</code>], and [$html:<code>remove</code>] are fallible; [$html:<code>length</code>] is infallible. The JS backend lowers growable collections to arrays and fixed collections to branded wrappers that enforce maximum length in [$html:<code>push</code>], preserve dense [$html:<code>get</code>] / [$html:<code>set</code>] / [$html:<code>remove</code>] behavior, and report logical [$html:<code>length</code>]. Deferred follow-ups include default-fill syntax, explicit fixed/growable conversion through [$html:<code>copy</code>] after cast/copy hardening, and growable initial-capacity hints only if future backend work needs them. Collection helper runtime contracts remain JS-owned for Alpha. HTML-Wasm collection behavior is experimental only.]
        ]

        [data:
            [: Hash maps]
            [: Supported / Frontend, HIR, borrow, JS, Wasm validation, and diagnostic coverage implemented]
            [: Canonical TypeEnvironment map identity, display, parsed-ref walking/remap, public [$html:<code>{K = V}</code>] type annotation parsing, semantic scalar-key diagnostics, AST [$html:<code>{key = value}</code>] literal parsing, contextual key/value typing, non-foldable const rejection, AST map member parsing/type-checking, first-class HIR map literals/operations, HIR reachability facts for map construction/use, conservative borrow validation for map aliases and mutation, JS runtime/backend lowering, HTML-Wasm reachable unsupported-feature diagnostics, and broad integration coverage are implemented. Coverage includes positive JS runtime behavior, aliases, struct fields, function parameter/return maps, nested values, empty/contextual typing, negative map syntax/type/member/fallibility/receiver/display/equality/iteration cases, and backend matrix behavior.]
            [: JS / HTML; HTML-Wasm rejects reachable map use; Wasm runtime deferred]
            [: V1 supported: insertion-ordered hashmaps with implemented [$html:<code>{K = V}</code>] type syntax, AST-level [$html:<code>{key = value}</code>] literal parsing, AST-level member parsing for [$html:<code>get</code>], [$html:<code>contains</code>], [$html:<code>set</code>], [$html:<code>remove</code>], [$html:<code>clear</code>], and property-style [$html:<code>length</code>], plus first-class HIR map literals/operations, reachable map-use facts, borrow validation for map operation effects, JS runtime/helper lowering, and HTML-Wasm rejection for reachable map construction or operation. Type annotations resolve through canonical [$html:<code>TypeId</code>] identity across aliases, parameters, returns, struct fields, and choice payloads, including map values containing collections, one direct nested map, or aliased maps. Type syntax rejects malformed separators, empty sides, fixed/capacity map forms, unsupported keys, generic keys, and excessive inline map nesting. Literal parsing accepts inferred non-empty maps, explicit empty maps, runtime key expressions, bare identifier keys as variables, contextual [$html:<code>none</code>] values, nested map values, and aliased map types; it rejects empty inferred [$html:<code>{}</code>], missing key/value entries, mixed collection/map entries in either order, duplicate known scalar keys, unknown bare identifier keys, unsupported key types, and const map literals. Map members require fallible handling for [$html:<code>get</code>], [$html:<code>set</code>], and [$html:<code>remove</code>], reject fallible handling on infallible members, require explicit [$html:<code>~</code>] for mutating receiver calls, keep [$html:<code>length</code>] read-only, and are compiler-owned rather than free functions. Borrow validation rejects mutation while a [$html:<code>get</code>] result is live, keeps [$html:<code>contains</code>] and [$html:<code>length</code>] fresh, treats [$html:<code>remove</code>] results as owned, consumes inserted non-copy [$html:<code>set</code>] and literal keys/values unless [$html:<code>copy</code>] is used, and keeps lookup keys borrowed for [$html:<code>get</code>], [$html:<code>contains</code>], and [$html:<code>remove</code>]. JS map helpers use branded insertion-ordered wrappers, fallible carriers for [$html:<code>get</code>], [$html:<code>set</code>], and [$html:<code>remove</code>], infallible [$html:<code>contains</code>], [$html:<code>clear</code>], and [$html:<code>length</code>], deterministic missing-key errors, deep-copy parity, and deterministic non-display fallback. HTML-Wasm validation reports [$html:<code>hashmap construction</code>] or [$html:<code>hashmap operation</code>] as unsupported before Wasm lowering, while unreachable helper functions containing maps remain ignored. Builtin map keys are permanently limited to [$html:<code>String</code>], [$html:<code>Int</code>], [$html:<code>Bool</code>], and [$html:<code>Char</code>]. Outside-scope map surfaces include [$html:<code>HASHABLE</code>] and generic keys, user-defined key behavior, [$html:<code>Float</code>] keys, map equality, const maps, fixed maps, optimized variants, and language hashsets. Wasm runtime/lowering remains deferred for the existing scalar-keyed builtin map surface.]
        ]

        [data:
            [: Collection generic substrate]
            [: Supported]
            [: Broad existing collection tests plus new capacity parsing, dependency-ordering, canonical identity, AST folding, fixed literal, shorthand, HIR propagation/validation, display, JS lowering/runtime, and mismatch tests.]
            [: JS / HTML]
            [: Internally represented through canonical [$html:<code>TypeEnvironment</code>] constructed collection types. HIR carries frontend [$html:<code>TypeId</code>] values, validates collection expressions against [$html:<code>TypeEnvironment::collection_shape</code>], and renders fixed collection types through canonical type display. Backends classify collection types through [$html:<code>TypeEnvironment</code>] queries. Fixed capacity syntax is represented as a literal-or-bare-constant parsed shape, bare-constant references create existing header dependency edges for top-level type surfaces, AST folds fixed capacities into canonical collection [$html:<code>TypeId</code>] identity through [$html:<code>CollectionShape</code>], and collection literal parsing now preserves explicit fixed collection shape. JS lowering consumes the canonical collection shape directly, using arrays for growable collections and [$html:<code>__bs_fixed_collection</code>] wrappers for fixed collections. Watch points are alias visibility, fixed-capacity diagnostics, and backend parity for supported collection operations.]
        ]

        [data:
            [: Results, options, multiple returns, and multi-bind]
            [: Partial]
            [: Broad current result, option, block and inline [$html:<code>catch:</code>] fallback forms with shared [$html:<code>then</code>] value production, [$html:<code>catch |err|:</code>] handler blocks, value-producing [$html:<code>if</code>] and full [$html:<code>if value is:</code>] match blocks at declaration initializers, simple assignment RHS sites, [$html:<code>return if</code>] sites including multi-return, and multi-bind RHS sites, inline non-option choice-pattern value [$html:<code>if</code>] predicates, inline option unwrap value [$html:<code>if</code>] with present capture [$html:<code>|value|</code>], postfix result propagation including [$html:<code>E -&gt; E?</code>] error-slot wrapping, postfix option propagation [$html:<code>?</code>], fallible external calls, cast, multi-bind, nested-option, contextual option wrapping, option-aware equality, option present capture [$html:<code>|value|</code>], limited option match patterns, and public [$html:<code>Option of T</code>] / [$html:<code>Result of T, E</code>] deferred-syntax diagnostic coverage. Boundary-only [$html:<code>catch</code>] coverage includes function-call arguments, collection items, loop headers, template interpolation, parenthesized arithmetic expressions, and field-access bases. Catch diagnostics cover empty and multiple handler bindings, handler fallthrough, fallback arity, fallback type mismatch, multiline inline fallback rejection, and same-line inline catch chaining before HIR. Invalid-form diagnostics now include empty [$html:<code>then</code>], blocked [$html:<code>then</code>], non-receiving value blocks, direct option fallback syntax, and unsupported optional [$html:<code>none</code>] / literal single-predicate value [$html:<code>if</code>] forms. Lower-level coverage includes value-production completeness, HIR value-block CFG/result locals, option-present capture extraction, JS option-present match conditions, and hidden-result materialization.]
            [: JS / HTML]
            [: Multi-bind supports explicit multi-return function calls plus value-producing [$html:<code>if condition then a else b</code>], inline non-option choice-pattern predicates such as [$html:<code>if status is Ready then a else b</code>], inline option present-capture unwraps such as [$html:<code>if maybe is |value| then value else fallback</code>], block [$html:<code>if condition:</code>] / [$html:<code>else</code>], full [$html:<code>if value is:</code>] match, and block/inline [$html:<code>catch</code>] recovery forms as RHS expressions. These value-producing forms are also accepted at declaration initializers, simple assignment RHS receivers, and return receivers; multi-return [$html:<code>return if</code>] lowers through internal tuple construction. Inline non-option choice predicates and inline option unwraps reuse the full-match pattern parser and lower as one-arm value matches with an inline default. They lower through HIR CFG blocks with hidden result locals rather than a new backend expression form; produced places are materialized into result locals so value-block merges do not alias arm-local captures. Built-in option nesting is rejected, including aliases that expand to an optional type. Contextual [$html:<code>T -&gt; T?</code>] wrapping is supported at explicit typed boundaries including declarations, calls, constructors, collections, and return slots. Fallible calls use postfix [$html:<code>!</code>] for propagation or boundary [$html:<code>catch:</code>] / [$html:<code>catch |err|:</code>] blocks for recovery; value-producing catch recovery keeps [$html:<code>ThenValue</code>] nodes in the handler body and lowers through shared value-block result locals. Inline catch is single-line expression/value-list sugar for the same model. Postfix error compatibility is exact [$html:<code>E -&gt; E</code>] or one-level optional wrapping [$html:<code>E -&gt; E?</code>] only. Postfix option propagation [$html:<code>?</code>] unwraps [$html:<code>T?</code>] on the present path and returns [$html:<code>none</code>] from the current function when absent; it works in optional-returning and fallible optional-success functions, and rejects non-option operands, non-optional return targets, and [$html:<code>? catch</code>]. [$html:<code>catch</code>] is rejected in nested expression positions and loop headers, including function-call arguments, collection literal items, template interpolation, parenthesized arithmetic expressions, and field-access bases. Assignment targets are unavailable inside catch recovery subtrees for the same assignment. Empty [$html:<code>then</code>], [$html:<code>then</code>] across blocked constructs, non-receiving value blocks, fallback-bang expressions, postfix handler syntax, direct option fallback syntax, and optional [$html:<code>none</code>] / literal single-predicate value [$html:<code>if</code>] forms are rejected with structured diagnostics. Option-aware [$html:<code>is</code>] / [$html:<code>is not</code>] supports presence checks and value comparisons when the inner type supports equality; current option matches support [$html:<code>none =&gt;</code>] presence checks, literal and relational present-value patterns, and present capture [$html:<code>|value| =&gt;</code>] with exhaustiveness. Block-form option unwrap value blocks, arbitrary nested value blocks, value-producing loops, user tuple values, option inner choice patterns, and polished diagnostics remain deferred. [$html:<code>none is none</code>] remains invalid. Public [$html:<code>Option of T</code>] and [$html:<code>Result of T, E</code>] remain deferred; use [$html:<code>T?</code>] and final [$html:<code>E!</code>] return slots instead.]
        ]

        [data:
            [: Constants and compile-time folding]
            [: Supported]
            [: Broad constant declaration, new top-level and body-local [$html:<code>#=</code>] / [$html:<code>#Type</code>] binding-mode syntax, const-only validation, explicit dependency ordering, same-file source-order rejection, cross-file imports, folding, overflow, const-template, const-record, and nested const-record field projection coverage.]
            [: Frontend / HTML builder]
            [: Runtime expressions and host calls in constants are rejected. Same-file constants must be declared before use; imported constants are resolved through header-stage dependency sorting. Module constants remain compile-time metadata rather than runtime top-level statements. New binding-mode syntax is supported for top-level constants, including inferred, explicit, collection, and optional annotations. Body-local [$html:<code>#</code>] constants are supported in declaration scopes when their initializer fully folds; runtime locals and function parameters remain invalid in compile-time constant initializers. Const records are field-access-only: [$html:<code>palette.red</code>] and fully projected nested fields such as [$html:<code>palette.primary.label</code>] are supported, but const-record values cannot be assigned to runtime values, passed, returned, collected, or materialized as HIR runtime struct constructs. Private inferred const facts are tracked internally during AST finalization and carried into HIR as advisory metadata; config validation consumes these facts to allow references to earlier compile-time constants while rejecting runtime expressions. Borrow checking, temporary-local reduction, and backend lowering do not consume HIR const metadata yet.]
        ]

        [data:
            [: Type aliases]
            [: Supported]
            [: Broad alias declaration, collection/option/import target, visibility, cycle, duplicate, value-misuse, constructor-misuse, and import-alias interaction coverage.]
            [: Frontend-owned]
            [: Type aliases are transparent and annotation-only. Imported aliases are file-local; exporting an imported type under a public name requires declaring a real exported type alias. Aliases to external opaque types are rejected with structured diagnostics.]
        ]

        [data:
            [: Arithmetic, cast transition, and explicit copy]
            [: Partial]
            [: Broad arithmetic, precedence, integer division, contextual numeric promotion, compound assignment, explicit copy, and JS helper coverage. Targeted coverage now proves removed scalar constructor-style conversions fail with stable diagnostics. Initial explicit [$html:<code>cast</code>] parser/AST support covers annotated declarations, assignments, returns, defaults, typed collection/map entries, inline and block value-producing [$html:<code>then</code>] targets, function/constructor/receiver-method arguments, struct and choice constructor field arguments, and struct field defaults, plus target validation, builtin/user evidence selection, generic-bound source evidence during generic body validation, infallible/propagate/recover handling, and invalid-source/target diagnostics. Generic parameter targets are rejected cleanly. Builtin const folding now covers materializable scalar literal results, failed builtin const diagnostics, direct foldable const [$html:<code>catch:</code>] recovery, const-foldable policy metadata, and foldable function parameter / struct field defaults including fallible direct-catch recovery. Policy-backed HIR lowering now emits [$html:<code>Cast</code>] expressions and [$html:<code>CastOp</code>] statements, reusing existing fallible-carrier branching. The JS backend implements demand-driven runtime helpers for all 14 builtin cast policies, including strict runtime parsing for [$html:<code>String -&gt; Int</code>], Alpha JS-safe integer parity for [$html:<code>String -&gt; Int</code>] and [$html:<code>Float -&gt; Int</code>] cast materialization, and helper emission for non-numeric fallible policies. Runtime integration coverage now exercises all builtin infallible policies, all builtin fallible [$html:<code>cast!</code>] success policies, representative fallible recovery error codes, safe-integer boundary success/recovery for [$html:<code>String -&gt; Int</code>] and [$html:<code>Float -&gt; Int</code>], demand-driven helper absence for [$html:<code>Int -&gt; Float</code>], helper presence for [$html:<code>String -&gt; Int</code>] and [$html:<code>String -&gt; Float</code>], optional target wrapping including inner-value catch recovery, optional/collection/map/function/struct/value-producing target sites, same-file user-defined nominal evidence to [$html:<code>String</code>] and [$html:<code>Error</code>], generic [$html:<code>CASTABLE_TO_STRING</code>] helpers with builtin and user-defined evidence, multi-target user evidence, and structured HTML-Wasm unsupported diagnostics. Negative integration coverage now asserts stable diagnostics for missing or invalid target contexts, generic inference target rejection, same-type casts, optional sources, optional catch recovery with [$html:<code>then none</code>], unhandled operand failures, invalid propagation and recovery forms, infallible [$html:<code>cast!</code>] use, missing evidence for intentionally skipped conversions such as [$html:<code>Bool -&gt; Int</code>], removed scalar constructors, safe-integer const rejection, reserved core cast trait names, and user-defined evidence in const-required contexts. Const [$html:<code>Error</code>] materialization, branching const catch handlers, and Wasm cast lowering remain deferred.]
            [: JS / HTML]
            [: Regular [$html:<code>/</code>] returns [$html:<code>Float</code>]. [$html:<code>//</code>] requires [$html:<code>Int</code>]. Scalar constructor-style conversions such as [$html:<code>Int(...)</code>], [$html:<code>Float(...)</code>], [$html:<code>Bool(...)</code>], [$html:<code>String(...)</code>], and [$html:<code>Char(...)</code>] are removed and now fail with a structured diagnostic. The [$html:<code>cast</code>] keyword now has parser/AST resolution for explicit typed-boundary sets, including [$html:<code>cast</code>] (infallible), [$html:<code>cast!</code>] (propagate), and [$html:<code>cast catch:</code>] / [$html:<code>cast catch |err|:</code>] (recover). Function, constructor, receiver-method, inline value-producing [$html:<code>if ... then ... else ...</code>], full match, and struct/choice constructor field arguments now supply explicit builtin cast targets; generic parameter targets and unknown external parameter types disable cast targets cleanly, while generic-bound source evidence is accepted during generic function body validation and concrete instances still resolve builtin or user-defined evidence. Same-file nominal user evidence lowers through selected direct methods before backend lowering, including generic helper instances that use user-defined evidence. Builtin const folding now routes explicit casts through compiler-owned cast policies when source and target literals can be materialized, rejects user-defined or generic-bound evidence in const-required contexts, folds successful fallible builtin casts such as [$html:<code>String -&gt; Int</code>], reports failed builtin const casts with a typed diagnostic, and folds failed builtin [$html:<code>cast ... catch:</code>] recovery when the handler is a direct single foldable [$html:<code>then</code>] value. Boundary catch recovery is also accepted in function parameter and struct field defaults when it fully folds. Builtin casts to [$html:<code>Int</code>] from [$html:<code>String</code>] and [$html:<code>Float</code>] currently materialize only Alpha JS-safe integers, from [$html:<code>-9007199254740991</code>] through [$html:<code>9007199254740991</code>], so AST folding and JS runtime behavior agree. Optional receiving targets cast to the inner builtin type first; catch recovery handlers for [$html:<code>T?</code>] targets produce inner [$html:<code>T</code>] values, then HIR wraps the merged result into [$html:<code>T?</code>]. Policies that cannot currently materialize an AST compile-time value, such as [$html:<code>String -&gt; Error</code>], are marked non-const-foldable and rejected with a cast diagnostic. Branching const catch handlers are rejected until a real const statement evaluator can choose the executed path. HIR lowering and the JS backend now implement policy-backed casts for all builtin policies, with JS helper emission driven by reachable cast policies rather than a fixed prelude; reachable HTML-Wasm runtime casts report unsupported-backend diagnostics while Wasm cast lowering and const [$html:<code>Error</code>] materialization remain. User-defined, generic, and external opaque cast targets are outside the cast design scope; user-defined evidence only adapts same-file nominal source values to compiler-supported builtin targets. Contextual [$html:<code>Int</code>] to [$html:<code>Float</code>] promotion remains separate from explicit casts. [$html:<code>Float -&gt; String</code>] formatting parity between Rust folding and runtime lowering remains a follow-up watch point. [$html:<code>^</code>] exists in frontend/JS lowering but has thin coverage; Wasm exponent support remains outside Alpha.]
        ]

        [data:
            [: Type checking and coercion]
            [: Supported]
            [: Broad type compatibility, contextual promotion, strict expression typing, call argument, and declaration coverage.]
            [: Frontend-owned]
            [: Keep strict expression typing separate from contextual declaration / return coercion.]
        ]

        [data:
            [: Traits]
            [: Partial]
            [: Broad declaration, conformance, visibility, generic-bound receiver-call, trait-name-as-type rejection, core cast trait metadata, builtin evidence, source-authored trait incompatibility metadata, facade/private relation validation, ordinary-value core cast trait misuse, and incompatible conformance coverage, including [$html:<code>trait_requirement_generic_this_return_rejected</code>], [$html:<code>trait_requirement_generic_this_param_rejected</code>], [$html:<code>trait_requirement_bare_second_this_rejected</code>], [$html:<code>trait_requirement_named_this_param_success</code>], [$html:<code>trait_name_parameter_type_rejected</code>], [$html:<code>trait_bound_trait_name_generic_argument_rejected</code>], [$html:<code>trait_conformance_incompatible_same_declaration_rejected</code>], [$html:<code>trait_conformance_incompatible_cross_declaration_rejected</code>], [$html:<code>trait_incompatibility_parse_success</code>], [$html:<code>trait_incompatibility_self_rejected</code>], [$html:<code>trait_incompatibility_forward_relation_rejected</code>], [$html:<code>trait_incompatibility_duplicate_rejected</code>], [$html:<code>trait_relation_facade_public_leaks_private_rejected</code>], [$html:<code>trait_relation_facade_private_private_success</code>], and [$html:<code>core_cast_trait_name_used_as_value_rejected</code>].]
            [: Frontend static semantics; HIR receives concrete calls]
            [: Trait declarations, explicit same-file nominal [$html:<code>Type must TRAIT</code>] conformances, canonical evidence, and static generic bounds with [$html:<code>type T is TRAIT</code>] are implemented. User-authored conformance is rejected for builtins, imported/dependency/library types, external opaque types, and other-file types. Core cast traits such as [$html:<code>CASTABLE_TO_STRING</code>] and [$html:<code>TRY_CASTABLE_TO_STRING</code>] are compiler-owned static traits that resolve without imports, cannot be shadowed by declarations or imports, cannot be used as ordinary values, carry builtin evidence for the initial cast table, and reject conflicting infallible/fallible evidence pairs for the same target type. Source-authored [$html:<code>TRAIT must not TRAIT</code>] relation metadata is parsed, source-order checked, recorded symmetrically, and used to reject incompatible conformance evidence. Exported facade relation metadata rejects public/private trait leaks while private/private relation metadata remains valid. Trait bounds may enable receiver calls on generic parameters inside the bounded generic declaration, while concrete values use ordinary visible receiver methods. A conformance declaration proves that a type satisfies a trait; it does not independently import trait methods as concrete receiver methods. Trait names are static contracts and are rejected in ordinary value-type positions such as parameters, returns, fields, choice payloads, collection elements, aliases, and concrete generic arguments. Trait requirement [$html:<code>This</code>] is direct-only: receiver [$html:<code>This</code>] / [$html:<code>~This</code>], named non-receiver [$html:<code>other This</code>], and direct return [$html:<code>This</code>] are supported, while composed forms such as [$html:<code>This?</code>], [$html:<code>{This}</code>], and [$html:<code>Box of This</code>] are rejected with structured syntax diagnostics. Method shape alone is not conformance. [$html:<code>DISPLAYABLE</code>] is metadata only and does not affect [$html:<code>io</code>], templates, string coercion, or operators. Dynamic trait values / trait objects, trait aliases/composition/downcasting, associated types/constants, inheritance, generic traits/methods, and conditional or specialized conformance are outside the current language design scope. Default methods, static non-method requirements, output coercion beyond explicit cast evidence, broader compiler-owned builtin conformance facts, operator/boolean keyword trait integration, and standard trait taxonomy remain deferred.]
        ]

        [data:
            [: String coercion and IO/template boundaries]
            [: Supported]
            [: Broad valid string-content coercion plus IO/template rejection coverage for structs, collections, options, results, and multiple-return values.]
            [: Frontend / JS]
            [: Template heads and [$html:<code>io</code>] accept only supported string-content values. Complex values must be handled, extracted, or converted before string-boundary use.]
        ]

        [data:
            [: Borrow validation and ownership analysis]
            [: Supported]
            [: Broad borrow, aliasing, mutable/exclusive access, use-after-move, branch merge, loop, short-circuit, lifetime, drop-site, and adversarial coverage.]
            [: HIR analysis / JS GC baseline]
            [: Borrow validation enforces exclusivity and move-safety, while ownership/drop facts remain advisory for GC-only JS. Do not make ownership semantic; GC fallback remains the language baseline.]
        ]

        [data:
            [: Paths and imports]
            [: Supported]
            [: Broad import syntax, relative child path, entry root fallback, source library prefix, circular dependency, multi-file module, namespace imports, grouped aliases, nested grouped aliases, collision, project-structure collision, module-boundary, and file-local coverage.]
            [: Frontend / HTML builder]
            [: Path rendering is golden-sensitive. Keep normalized output and platform-stable diagnostics covered. Source and external package namespace imports are supported as shallow field-access-only records: value/function/constant members are accessed as [$html:<code>namespace.member</code>], type members are valid in type position as [$html:<code>namespace.TypeName</code>], and record values cannot be passed, returned, assigned, or traversed recursively. Specific imports use grouped syntax; direct symbol-path imports are rejected with [$html:<code>BST-IMPORT-0017</code>], explicit [$html:<code>.bst</code>] import extensions are rejected with [$html:<code>BST-IMPORT-0020</code>], and explicit external file extensions without an active provider are rejected with [$html:<code>BST-IMPORT-0021</code>]. Grouped source import aliases, nested grouped source import aliases, and grouped imports for virtual external package functions, types, and constants are supported. External packages expose free functions, opaque types, and constants only, so grouped external imports no longer have a receiver-method branch. Grouped virtual external package imports resolve through external package metadata before source/module facade enforcement, preventing root/module facades from misclassifying imports such as [$html:<code>import @web/canvas { get_canvas }</code>] while preserving strict direct symbol-path rejection and source facade privacy. Same-module ordinary top-level declarations are importable by default without the old [$html:<code>#</code>] visibility prefix. Directories containing non-config [$html:<code>#*.bst</code>] files are module boundaries; [$html:<code>#mod.bst</code>] is the only public facade. Cross-module and source-library imports go through [$html:<code>#mod.bst</code>] facades, and modules without a facade cannot be imported from outside. Sibling [$html:<code>name.bst</code>] / [$html:<code>name/</code>] collisions are rejected during Stage 0 project discovery, while [$html:<code>.js</code>] files are excluded from that specific collision rule. Explicit facade status: authored declarations in [$html:<code>#mod.bst</code>] are public only when marked with [$html:<code>export</code>], while unmarked declarations and ordinary imports stay private to the facade file. [$html:<code>export import @path { Symbol }</code>] and grouped [$html:<code>export @path { Symbol }</code>] public re-exports are supported for source and external package symbols, aliases define the public facade name, duplicate public names are rejected with [$html:<code>BST-RULE-0081</code>], public authored facade APIs reject private type surfaces, including inferred exported constant types, with [$html:<code>BST-RULE-0082</code>], and bare namespace exports such as [$html:<code>export @path</code>] remain deferred. Alias and visible-name collisions are hard errors across same-file declarations, source imports, external imports, namespace records, type aliases, prelude symbols, builtins, and facade public names. Direct imports of special files such as [$html:<code>#mod.bst</code>], [$html:<code>#page.bst</code>], and [$html:<code>#config.bst</code>] are rejected with [$html:<code>BST-IMPORT-0008</code>]. Prelude and external symbols cannot be redeclared as user functions or variables at top level or in body scope. Alias case mismatches warn. Grouped import aliases reject group-level trailing aliases, invalid local alias names, and duplicate alias syntax with structured diagnostics. Parent-directory imports with `..` are not supported, and imports must stay within allowed module/library/project roots.]
        ]

        [data:
            [: External platform packages]
            [: Partial]
            [: Broad package-scoped import and visibility coverage, with targeted external constant, fallible external signature, Float/F64/Char ABI, core libraries, prelude, duplicate-name, non-imported-symbol, alias diagnostics, JS runtime-import validation, and project-local JS provider frontend coverage.]
            [: Frontend / JS]
            [: Functions, opaque types, and compile-time external constants are supported through Rust-side registry metadata. [$markdown:
                - General user-authored external binding files are not supported. The HTML builder now supports the first provider-backed external files: annotated single-file `.js` imports with `@bst.opaque` and `@bst.sig` comments.
                - Project-local, source-library, and builder-owned JS-backed packages can create frontend-visible package-scoped opaque types and free functions. External packages do not expose receiver methods. Provider-created JS receiver-style signatures are rejected during registration. Namespace imports, grouped imports, grouped aliases for functions/types, fallible `Error!` signatures, parser rejection for malformed `this` receiver parameters, provider cache deduplication, parsed registered-runtime imports, block-bodied arrow export support, expression-bodied arrow export rejection, and emitted JS runtime-asset metadata are covered.
                - Direct `.js` imports obey the same module/source-library privacy boundaries as Beanstalk source imports: private JS implementation files cannot be imported across `#mod.bst` facade boundaries. Dev-server watch scopes include direct `.js` files under the entry root and configured library folders while ignoring generated output.
                - The HTML JS path now generates glue modules for provider-created ES module exports, imports emitted JS assets by relative module specifier, and maps stable external function IDs to generated wrapper names. Fallible JS wrappers convert `bstOk` / `bstErr` results into the backend's internal carrier shape, debug glue rejects invalid wrapper shapes, and release glue returns an error carrier instead of treating arbitrary JS values as success.
                - Registered core JS runtime modules such as `@beanstalk/runtime` are emitted once per build and resolved from generated pages through an import map when accepted JS runtime imports require them. Runtime imports are parsed from the JS source, may use multiline named import formatting, validated against registered exported names, and are not inferred from fallibility. Fallible JS functions may use `bstOk` / `bstErr` helpers or return the wrapper shape manually. Runtime JS assets, generated glue, registered runtime modules, import-map entries, and unsupported-backend checks are driven by external function calls reachable from entry `start`, not by unused source-library/provider declarations. Unused `@web/canvas` / `@html` wrappers do not emit canvas assets or `@beanstalk/runtime`. The HTML builder provides the first built-in JS-backed package, `@web/canvas`, with explicit imports, opaque `CanvasElement`, `Canvas2d`, `CanvasGradient`, `CanvasPattern`, `CanvasImage`, `CanvasImageData`, and `CanvasTextMetrics` types, fallible canvas/context/export/image-data/image-drawing helpers, and broad Canvas 2D free functions for styles, paths, transforms, text, gradients, patterns, image drawing, and pixel editing.
                - External constants are implemented for scalar compile-time values (Int, Float, Bool) and lower through expression parsing as ordinary literals. Non-scalar external constants are rejected in constant contexts.
                - External ABI metadata includes `F64`, `I32`, `Bool`, `Char`, and `Utf8Str` for the current core package skeletons and JS provider signatures.
                - External function metadata supports frontend-visible success slots plus an optional final concrete error slot. Fallible external calls must use postfix `!` or boundary `catch:` / `catch |err|:` handling, and they lower through the same explicit fallible HIR control-flow path as source functions.
                - External custom error slots must reference registered frontend-visible external types. Unknown external signature types are rejected instead of being smuggled through `T!`.
                - Explicit imports are required for non-prelude external packages. `io` and `IO` are registered as external prelude symbols; builtin error symbols are compiler-owned bare names.
                - External function aliases and external constant aliases are supported. External opaque type aliases are rejected with structured diagnostics. Grouped external aliases for functions, constants, and types plus shallow external package namespace imports are supported.
                - Prelude collision rejection is enforced: explicit imports cannot shadow prelude names, and prelude symbols cannot be redeclared as user declarations.
                - Method-style ergonomics for external values must come from source-owned wrapper types, such as `@html`'s `Canvas` wrapper around `@web/canvas`.
                - JS lowering now resolves external runtime names through per-function lowering metadata in the registry.
                - `InlineExpression` JS lowering is implemented for pure expression external functions. Arguments are substituted positionally using `#0`, `#1`, ... placeholders; duplicate placeholders are rejected. No runtime helper is emitted for inline-only functions.
                - Provider-created JS functions carry `ExternalModuleExport` metadata with the original ES module export name for the generated-glue phase. They are intentionally not treated as direct inline runtime helpers.
                - Wasm lowering for external packages is experimental and should not be counted as Alpha support. Reachable `@web/canvas` calls remain JS-backed and are rejected for HTML-Wasm with structured unsupported-backend diagnostics.
                - Unsupported external package function calls for the active backend are rejected with structured `BST-RULE-0058` diagnostics before backend lowering, not during LIR transformation. Validation is reachable-call-based so unused wrappers do not fail builds for backends that would reject the call if reached.
            ]]
        ]

        [data:
            [: Core prelude libraries]
            [: Supported]
            [: Targeted coverage for prelude symbol registration and collision rejection.]
            [: Frontend / JS]
            [: Every builder must provide `@core/prelude`. `io` and `IO` are registered as bare prelude symbols. Prelude collision rejection is enforced: explicit imports cannot shadow prelude names, and prelude symbols cannot be redeclared as user declarations.]
        ]

        [data:
            [: Core math package]
            [: Supported]
            [: Targeted success and diagnostic coverage for direct, grouped, and aliased imports; aliased constants such as [$html:<code>PI as pi</code>]; aliased functions such as [$html:<code>sin as sine</code>]; const-context constants; arity errors; type errors; missing symbols; and non-imported access.]
            [: JS / HTML]
            [: [$html:<code>@core/math</code>] exposes [$html:<code>PI</code>], [$html:<code>TAU</code>], [$html:<code>E</code>], and Float functions backed by JavaScript [$html:<code>Math</code>]. [$markdown:
                - Implemented functions: `sin`, `cos`, `tan`, `atan2`, `log`, `log2`, `log10`, `exp`, `pow`, `sqrt`, `abs`, `floor`, `ceil`, `round`, `trunc`, `min`, `max`, and `clamp`.
                - All functions currently accept and return `Float`.
                - The package is not prelude-imported. Users must explicitly import the required symbols from `@core/math`.
                - Grouped imports, aliased imports, and namespace imports are supported. Direct symbol-path imports are rejected.
                - JS helpers are emitted only when the matching external function is referenced.
                - Wasm support for this package remains deferred/experimental. Imports used in HTML-Wasm builds fail with a structured `Rule` diagnostic before backend lowering.
            ]]
        ]

        [data:
            [: Core text package]
            [: Partial]
            [: Targeted runtime, JS helper, arity, type-error, edge-case, unsupported-builder, and Wasm-unsupported coverage.]
            [: JS / HTML]
            [: [$html:<code>@core/text</code>] exposes [$html:<code>length</code>], [$html:<code>is_empty</code>], [$html:<code>contains</code>], [$html:<code>starts_with</code>], and [$html:<code>ends_with</code>]. It is JS-backed, explicit-import only, and optional by builder. [$html:<code>length</code>] returns backend-defined string length (JS UTF-16 code units for the JS backend). [$html:<code>is_empty</code>] is true only for the exactly empty string. Empty needle/prefix/suffix behavior follows backend-defined JS semantics. Receiver-method forms remain deferred. Wasm support is deferred; HTML-Wasm builds fail with a structured [$html:<code>Rule</code>] diagnostic before backend lowering.]
        ]

        [data:
            [: Core random package]
            [: Partial]
            [: Targeted import, JS helper, runtime smoke, arity, type-error, and Wasm-unsupported coverage.]
            [: JS / HTML]
            [: [$html:<code>@core/random</code>] exposes [$html:<code>random_float()</code>] and [$html:<code>random_int(min, max)</code>]. [$html:<code>random_float()</code>] returns a value in the half-open range 0.0 to 1.0. [$html:<code>random_int(min, max)</code>] is inclusive at both ends. When [$html:<code>min &gt; max</code>] the bounds are swapped automatically; when [$html:<code>min</code>] equals [$html:<code>max</code>] the result is that value. Seeded random remains deferred. Wasm support is deferred; HTML-Wasm builds fail with a structured [$html:<code>Rule</code>] diagnostic before backend lowering.]
        ]

        [data:
            [: Core time package]
            [: Partial]
            [: Targeted import, grouped alias, namespace, free-function helper, runtime smoke, fallible parse/catch, raw method-call rejection, arity/type diagnostics, old-name rejection, opaque-field rejection, reachable Wasm-unsupported, and unused-wrapper reachability coverage.]
            [: JS / HTML]
            [: [$html:<code>@core/time</code>] exposes opaque [$html:<code>Duration</code>], [$html:<code>TimeMark</code>], and [$html:<code>Timestamp</code>] types plus the first JS-backed time helper slice. [$markdown:
                - Monotonic elapsed-time helpers: `mark_now()`, `elapsed_since(start)`, and `duration_between(start, end)`.
                - Wall-clock timestamp helpers: `timestamp_now()`, `timestamp_from_unix_seconds(seconds)`, `timestamp_from_unix_milliseconds(milliseconds)`, and fallible `timestamp_from_iso_string(text)`.
                - Duration helpers: `duration_from_seconds(seconds)`, `duration_from_milliseconds(milliseconds)`, `as_seconds(duration)`, `as_milliseconds(duration)`, `is_negative(duration)`, `abs(duration)`, and `clamp(duration, min, max)`.
                - Timestamp helpers: `unix_seconds(timestamp)`, `unix_milliseconds(timestamp)`, and `to_iso_string(timestamp)`.
                - Use `TimeMark`, not `Timestamp`, for frame deltas and profiling. `Timestamp` is UTC wall-clock time and can move with system clock changes.
                - Current JS representation is opaque to Beanstalk code but numeric internally: durations and marks are milliseconds, timestamps are Unix epoch milliseconds.
                - Full calendar date/time/time-zone APIs, local time-zone lookup, locale-aware formatting/parsing, timers/sleep/intervals, browser animation scheduling, and Wasm/native lowerings remain deferred. Reachable HTML-Wasm usage fails with a structured unsupported-backend diagnostic before backend lowering.
            ]]
        ]

        [data:
            [: Source library roots]
            [: Partial]
            [: Targeted coverage for AST-backed config loading, known-key enforcement, config value-shape enforcement, plain immutable and explicit compile-time config bindings, folded template settings, config-local type/struct/choice support declarations, core/builder source-library config imports, project-local and relative config import rejection, unsupported config functions/mutable bindings/standalone templates/runtime-only values, config local const references between declarations, config-defined source library root resolution, missing/non-directory explicit scan roots, prefix collision detection, empty entry roots, `.bst` file/folder collision rejection, and `#mod.bst` facade enforcement.]
            [: Frontend / HTML]
            [: Source libraries and regular modules share the same visibility model. Config loading now runs `#config.bst` and reachable core/builder source-library support files through tokenizer, header parsing, dependency sorting, and AST construction before extracting folded immutable known-key declarations authored in the config file. Config declarations must use known core or builder-declared keys, and each known key's registered value shape is enforced before core fields are applied or backend settings are stored. String settings accept string literals and folded templates, [$html:<code>project</code>] is a closed string set currently accepting only [$html:<code>"html"</code>], boolean settings require folded [$html:<code>Bool</code>] values, [$html:<code>library_folders</code>] accepts one string or a collection of strings, and [$html:<code>template_const_loop_iteration_limit</code>] requires a positive folded [$html:<code>Int</code>] no greater than [$html:<code>1_000_000</code>]. Preferred source-library scan syntax is [$html:<code>library_folders = {"lib", "packages"}</code>]; explicit compile-time syntax such as [$html:<code>library_folders #= {"lib"}</code>] is also supported. Folded template initializers are accepted for string settings. Const-record field projection is supported for explicit [$html:<code>#=</code>] config keys and deferred for plain [$html:<code>=</code>] config keys pending shared const-context or resolver support. Authored config may also contain imports, type aliases, structs, and choices as compile-time support declarations. Config values may reference earlier compile-time constants (both explicit [$html:<code>#=</code>] and plain immutable declarations) through shared AST const facts, including constants imported from core/builder source libraries; values that do not resolve to compile-time constants are rejected with structured diagnostics. Authored config imports may only target core/builder libraries, while relative and project-local config imports are rejected by design. Path-token folder entries such as [$html:<code>@lib</code>] are no longer accepted in config values; use string folder names instead. Each direct child directory of a configured scan folder becomes an import prefix. The configured scan folder name itself is not an import prefix. `/lib` is the default scan folder when `library_folders` is omitted. Missing default scan folders are ignored; explicitly configured missing scan folders are config errors. [$html:<code>#root_folders</code>] has been replaced by [$html:<code>library_folders</code>] and is rejected with a migration diagnostic. Prefix collisions between builder-provided and project-local libraries produce hard errors. Sibling [$html:<code>name.bst</code>] / [$html:<code>name/</code>] collisions under the entry root or source-library roots are hard project-structure errors. All module roots enforce facade boundaries: any directory with a non-config [$html:<code>#*.bst</code>] file is a boundary, modules that export outside themselves must do so through `#mod.bst`, and outside importers cannot bypass the facade. `#mod.bst` is facade-only: unmarked top-level declarations and ordinary imports are private to the facade, while top-level runtime statements and runtime templates are rejected.]
        ]

        [data:
            [: Builder-provided source libraries]
            [: Partial]
            [: Targeted coverage for builder-provided source library root registration and HTML builder library.]
            [: Frontend / HTML]
            [: Builders expose source libraries through `LibrarySet.source_libraries`. The HTML builder provides `@html` as a default source library with basic HTML tag helpers, formatting utilities, and authored canvas convenience helpers backed by [$html:<code>@web/canvas</code>]. Imports inside [$html:<code>libraries/html/#mod.bst</code>] remain local implementation details; raw [$html:<code>@web/canvas</code>] symbols are not re-exported through [$html:<code>@html</code>] unless [$html:<code>@html</code>] authors real wrapper declarations. These libraries follow the same module visibility/facade rules as project modules. Prefix collisions between builder-provided and project-local libraries produce a hard config error.]
        ]

        [data:
            [: Project-local libraries]
            [: Partial]
            [: Targeted coverage for config-defined project library folders, import resolution, and facade gating.]
            [: Frontend / HTML]
            [: Project-local source libraries are discovered from [$html:<code>library_folders</code>] (default convention: `/lib` when not configured). Imports resolve through the library prefix (for example, `/lib/helper` as `@helper` or `/packages/helper` as `@helper`). Public cross-module access goes through `#mod.bst`; bypassing implementation files from outside the module is rejected. Module boundary enforcement applies to all module roots, not only project-local libraries.]
        ]

        [data:
            [: HTML project builds]
            [: Supported]
            [: Broad routing, document shell, metadata, tracked asset, config, and page-fragment coverage.]
            [: HTML / JS]
            [: This is the release-critical project backend. HTML-Wasm cases must not be treated as Alpha support requirements.]
        ]

        [data:
            [: Logical expressions]
            [: Supported]
            [: Broad precedence, grouping, invalid operand, not, comparison, and JS operator mapping coverage.]
            [: JS / HTML]
            [: Keep short-circuit behavior covered through artifact-shaped tests.]
        ]

        [data:
            [: If statements and conditions]
            [: Supported]
            [: Broad condition validation, nested condition, Bool requirement, same-line [$html:<code>else if</code>] rejection, and JS lowering coverage.]
            [: JS / HTML]
            [: No major Alpha gap if current control-flow coverage remains green.]
        ]

        [data:
            [: Char]
            [: Supported]
            [: Broad success coverage with targeted diagnostic coverage.]
            [: JS / HTML]
            [: Additional malformed-char diagnostics are useful cleanup, not a core Alpha blocker.]
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
    # Compiler-owned / standard external builtin surfaces

    [styled_table:
        [header: [: Surface] [: Status] [: Coverage] [: Runtime target] [: Watch points] ]

        [data:
            [: Collection methods]
            [: Supported]
            [: Broad JS helper, fallible [$html:<code>get</code>] / [$html:<code>set</code>] / [$html:<code>push</code>] / [$html:<code>remove</code>], infallible [$html:<code>length</code>], removed indexed-write, out-of-bounds, and explicit-mutable-receiver coverage.]
            [: JS / HTML]
            [: [$html:<code>get</code>], [$html:<code>set</code>], [$html:<code>push</code>], [$html:<code>remove</code>], and [$html:<code>length</code>] are compiler-owned surfaces. Indexed assignment through [$html:<code>get</code>] has been removed; use handled [$html:<code>~items.set(index, value)</code>] calls instead. Mutating [$html:<code>push</code>] calls now use the same [$html:<code>!</code>] / [$html:<code>catch</code>] fallible handling requirement as other fallible collection methods.]
        ]

        [data:
            [: Error helper methods]
            [: Removed from public surface]
            [: Removal coverage rejects public helper calls such as [$html:<code>err.bubble()</code>]. Hidden JS metadata helper coverage remains backend-internal.]
            [: Frontend / JS runtime internal]
            [: Do not re-expose [$html:<code>with_location</code>], [$html:<code>push_trace</code>], [$html:<code>bubble</code>], or a public [$html:<code>@core/error</code>] helper package before the Result/Error refactor explicitly designs it.]
        ]

        [data:
            [: Receiver methods]
            [: Supported]
            [: Broad method call, same-file struct and choice receivers, aligned generic nominal receivers, imported receiver type calls, namespace receiver type calls, facade receiver type exposure, receiver-method import and alias rejection, nonlocal/builtin/external receiver rejection, duplicate same-receiver method rejection, unsupported receiver, collision, free-function call, and mutable receiver diagnostics coverage.]
            [: JS / HTML]
            [: Source-authored receiver methods stay statically resolved and must be declared in the same file as their user-defined struct or choice receiver type. Methods travel with receiver type visibility and are never imported, aliased, or re-exported independently. Namespace imports may make a receiver type visible, but receiver methods are not namespace-record fields. A facade exposes a type's same-file receiver methods when it exposes the type through an exported source surface. Receiver-method names may be shared across different receiver types because dispatch includes the receiver type. External packages no longer register receiver-method metadata.]
        ]

        [data:
            [: Fallible suffix handling]
            [: Supported]
            [: Broad propagation, shared [$html:<code>then</code>] catch recovery, catch handler, handler fallthrough rejection, template-boundary, explicit HIR fallible-branch, direct alias-return forwarding, nested call-argument propagation, runtime-operator propagation, receiver-base propagation, match-guard propagation, and obsolete HIR propagation-expression removal coverage.]
            [: JS / HTML]
            [: Statement, direct-return, declaration, assignment, multi-bind, call-argument, runtime-operator, compound-child, condition, loop-input, template-chunk, receiver-base, and match-guard propagation now lower through explicit success/error HIR edges. Value-producing catch recovery lowers [$html:<code>then</code>] values through shared value-block result locals; nested ordinary branch paths may produce for the same catch block. Success-producing catch handlers without [$html:<code>then</code>] must terminate explicitly; literal false assertion handlers satisfy that rule, while dynamic assertions still fall through for static completeness. Fallthrough is rejected before HIR. Collection [$html:<code>get</code>], [$html:<code>set</code>], [$html:<code>push</code>], and [$html:<code>remove</code>] use the same explicit fallible branch path; [$html:<code>length</code>] lowers as a plain call. The old expression-level HIR propagation helper has been removed. A temporary internal fallible carrier remains in type/HIR lowering internals for backend handoff and is not public [$html:<code>Result</code>] syntax.]
        ]

        [data:
            [: Builtin Error type]
            [: Supported with simplified surface]
            [: Constructor/default/named-argument coverage plus public [$html:<code>message</code>] and [$html:<code>code</code>] field coverage. Backend-generated collection, cast, time, and external-JS errors use the same lowered field shape. Old family names compile as user declarations.]
            [: Frontend / JS]
            [: Public fields are only [$html:<code>message</code>] and [$html:<code>code</code>]. [$html:<code>code</code>] defaults to [$html:<code>0</code>] through the general struct-default path. Hidden location/trace metadata remains backend-internal.]
        ]
    ]
]

#[section, $markdown:
    # Compiler development tooling

    [styled_table:
        [header: [: Tooling] [: Status] [: Coverage] [: Current role] [: Watch points] ]

        [data:
            [: Benchmark summaries]
            [: Supported]
            [: Targeted]
            [: Terse tracked monthly benchmark summaries for rough compiler-development trend tracking.]
            [: Do not add per-case tables or raw counter dumps. Detailed analysis belongs in local JSONL and local report commands.]
        ]

        [data:
            [: Local benchmark history]
            [: Supported]
            [: Targeted]
            [: Local-only JSONL stores per-case timings, stage timings, counters, suite kind, primary metric, system identity, and commit metadata.]
            [: Must remain ignored/untracked. Schema changes should be deliberate and documented.]
        ]

        [data:
            [: Benchmark work counters]
            [: Supported]
            [: Targeted]
            [: Stable counter transport records frontend implementation-pressure counters and the first cheap work-volume counters for input size, dependency sorting, AST, HIR, and borrow validation.]
            [: Counters are diagnostic evidence, not public results. Prefer cheap counters recorded during existing compiler work. Older local records may still contain legacy human labels.]
        ]

        [data:
            [: Local benchmark drilldown report]
            [: Supported]
            [: Targeted]
            [: Local-only [$html:<code>just bench-report</code>] command reports slowest cases, stage movement, counter movement, normalized ratios, and next investigation candidates from ignored JSONL history.]
            [: Must not write tracked summaries. Must not become a CI timing gate.]
        ]

        [data:
            [: Profiling command wrappers]
            [: Supported]
            [: Targeted]
            [: Optional local [$html:<code>just profile-build</code>] helper builds a frame-pointer, line-table profiling binary after benchmark reports identify an active investigation area.]
            [: Keep out of default validation. Avoid mandatory external tools.]
        ]

        [data:
            [: CI performance gates / public dashboard]
            [: Deferred]
            [: None]
            [: Not part of Alpha. Current benchmarks are rough local sanity checks.]
            [: Do not fail CI on timing noise until benchmarks are stable enough and dedicated infrastructure exists.]
        ]

        [data:
            [: Criterion microbenchmarks]
            [: Deferred / Targeted]
            [: None]
            [: Only for isolated hot functions after local benchmark reports identify them.]
            [: Do not replace end-to-end and frontend benchmark suites with microbenchmarks.]
        ]
    ]
]

#[section, $markdown:
    # Experimental surfaces

    These are useful and may have tests, but they are not part of the Alpha stability contract.

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
            [: Long-term backend direction. Current lowering/runtime support is partial.]
            [: Do not count Wasm support as an Alpha blocker except for clean diagnostics or no-panic guarantees on covered paths.]
        ]
    ]
]

#[section, $markdown:
    # Deferred or reserved surfaces

    These are intentionally not counted as supported Alpha features.
    If syntax exists, it should fail with a structured diagnostic rather than cause a compiler panic or lower incorrectly.

    [styled_table:
        [header: [: Surface] [: Status] [: Canonical diagnostic direction] [: Notes] ]

        [data:
            [: Trait ecosystem extensions]
            [: Deferred]
            [: Current trait diagnostics and implemented trait fixtures are listed in the core Traits row.]
            [: Default methods, associated items, trait inheritance/composition, generic traits, specialized generic instance conformance, deriving, operator traits, formatting/display coercion, hashing, ordering, iteration, serialization, downcasting/reflection, and broad standard-trait library design remain outside the Alpha trait slice.]
        ]

        [data:
            [: Async / concurrency scopes]
            [: Deferred]
            [: [$html:<code>async_block_reserved</code>], [$html:<code>async_keyword_declaration_rejected</code>]]
            [: Future async syntax is design-only. Current [$html:<code>async</code>] statement and declaration positions are rejected with structured diagnostics.]
        ]

        [data:
            [: Checked scopes]
            [: Deferred]
            [: [$html:<code>checked_block_reserved</code>], [$html:<code>checked_keyword_declaration_rejected</code>]]
            [: [$html:<code>checked</code>] is reserved and rejected cleanly until checked/effect semantics are designed.]
        ]

        [data:
            [: Labeled scopes]
            [: Rejected / Reserved]
            [: [$html:<code>labeled_scope_rejected</code>]]
            [: Bare [$html:<code>label:</code>] syntax is not supported. Use [$html:<code>block:</code>] for ordinary scoped blocks.]
        ]

        [data:
            [: Legacy / foreign-language syntax]
            [: Rejected / Reserved]
            [: [$html:<code>typescript_style_declaration_rejected</code>], [$html:<code>diagnostic_statement_keyword_fn</code>], [$html:<code>diagnostic_statement_keyword_let</code>], [$html:<code>diagnostic_statement_keyword_match</code>], [$html:<code>diagnostic_statement_comment_slash_slash</code>], [$html:<code>diagnostic_expression_double_equal</code>], [$html:<code>diagnostic_expression_bang_equal</code>], [$html:<code>diagnostic_expression_and_and</code>], [$html:<code>diagnostic_expression_or_or</code>], [$html:<code>diagnostic_expression_bang_negation</code>]]
            [: These are diagnostic-quality fixtures only. Do not treat compatibility syntax as planned Alpha surface.]
        ]

        [data:
            [: No public panic keyword/directive]
            [: Not supported]
            [: Unknown names or ordinary invalid syntax]
            [: There is no [$html:<code>panic</code>] keyword and no [$html:<code>#panic</code>] directive in current Beanstalk. Use [$html:<code>assert(false, "message")</code>] for an intentional explicit panic.]
        ]

        [data:
            [: Debug-only assertions]
            [: Deferred]
            [: No accepted syntax]
            [: There is no [$html:<code>debug_assert</code>]. [$html:<code>assert</code>] is always checked in development and release builds.]
        ]

        [data:
            [: Catchable / recoverable panics]
            [: Rejected / Deferred]
            [: [$html:<code>assert</code>] result-handling diagnostics]
            [: There is no [$html:<code>recover</code>], [$html:<code>catch_panic</code>], public panic payload, or Beanstalk [$html:<code>catch</code>] support for assertion failure. Expected failures use [$html:<code>Error!</code>] and [$html:<code>catch</code>].]
        ]

        [data:
            [: Additional explicit stop helpers]
            [: Deferred]
            [: No accepted syntax]
            [: There is no [$html:<code>todo</code>], [$html:<code>unreachable</code>], [$html:<code>fatal</code>], [$html:<code>abort</code>], or [$html:<code>precondition</code>] builtin yet.]
        ]

        [data:
            [: Runtime assertion-message expressions]
            [: Deferred]
            [: [$html:<code>invalid_builtin_call</code>]]
            [: Arbitrary runtime message expressions require lazy failure-only evaluation design. Current assertion messages must be string literals.]
        ]

        [data:
            [: Compile-time constant assertion messages]
            [: Deferred]
            [: [$html:<code>invalid_builtin_call</code>]]
            [: Compile-time string constants are not normalized as assertion messages yet. This should use existing const facts rather than a new evaluator.]
        ]

        [data:
            [: Wasm assertion messages]
            [: Deferred / Experimental]
            [: Wasm trap lowering]
            [: Wasm lowers assertion failure to a trap for now. Message materialization and trap payload support remain deferred.]
        ]

        [data:
            [: Choice unit variants]
            [: Supported]
            [: [$html:<code>choice_basic_declaration_and_use</code>], [$html:<code>choice_unit_constructor_call_rejected</code>], [$html:<code>choice_imported_unit_constructor_call_rejected</code>]]
            [: Unit variants are constructed as `Choice::Variant`. Empty constructor calls are invalid.]
        ]

        [data:
            [: Choice record payload variants]
            [: Supported]
            [: [$html:<code>choice_payload_record_declaration_success</code>], [$html:<code>choice_payload_imported_field_type_success</code>], [$html:<code>choice_payload_empty_record_rejected</code>], [$html:<code>choice_payload_duplicate_field_rejected</code>], [$html:<code>choice_payload_mutable_field_rejected</code>], [$html:<code>choice_payload_field_default_rejected</code>], [$html:<code>choice_payload_shorthand_rejected</code>]]
            [: Only `Variant | field Type, ... |` syntax. Payload shorthand is rejected.]
        ]

        [data:
            [: Choice constructor use]
            [: Supported]
            [: [$html:<code>choice_payload_constructor_positional_success</code>], [$html:<code>choice_payload_constructor_named_success</code>], [$html:<code>choice_payload_constructor_mixed_success</code>], [$html:<code>choice_payload_constructor_missing_field_rejected</code>], [$html:<code>choice_payload_constructor_unknown_field_rejected</code>], [$html:<code>choice_payload_constructor_duplicate_field_rejected</code>], [$html:<code>choice_payload_constructor_positional_after_named_rejected</code>], [$html:<code>choice_payload_constructor_type_mismatch_rejected</code>], [$html:<code>choice_payload_constructor_without_args_rejected</code>], [$html:<code>choice_unit_constructor_call_rejected</code>], [$html:<code>choice_imported_unit_constructor_call_rejected</code>], [$html:<code>choice_imported_payload_constructor_success</code>]]
            [: Unit variants use `Choice::Variant` without parentheses. Payload variants use `Choice::Variant(...)` with positional and named arguments. Empty constructor calls on unit variants are rejected. Missing constructor args on payload variants are rejected.]
        ]

        [data:
            [: Choice const values]
            [: Supported]
            [: [$html:<code>choice_const_unit_success</code>], [$html:<code>choice_const_payload_success</code>]]
            [: Unit and payload choice variants can be used in top-level `#` constant declarations.]
        ]

        [data:
            [: Choice payload matching]
            [: Supported]
            [: [$html:<code>choice_payload_match_capture_success</code>], [$html:<code>choice_payload_match_qualified_capture_success</code>], [$html:<code>choice_payload_match_guard_uses_capture_success</code>], [$html:<code>choice_payload_match_import_alias_success</code>], [$html:<code>choice_payload_match_exhaustive_tag_level_success</code>], [$html:<code>choice_payload_match_wrong_capture_name_rejected</code>], [$html:<code>choice_payload_match_too_few_captures_rejected</code>], [$html:<code>choice_payload_match_too_many_captures_rejected</code>], [$html:<code>choice_payload_match_duplicate_capture_rejected</code>], [$html:<code>choice_payload_match_capture_shadowing_rejected</code>], [$html:<code>choice_payload_match_rename_success</code>], [$html:<code>choice_payload_match_rename_guard_success</code>], [$html:<code>choice_payload_match_rename_wrong_original_rejected</code>], [$html:<code>choice_payload_match_rename_duplicate_binding_rejected</code>], [$html:<code>choice_payload_match_rename_shadowing_rejected</code>], [$html:<code>choice_payload_match_rename_missing_alias_rejected</code>], [$html:<code>choice_payload_match_rename_non_symbol_alias_rejected</code>], [$html:<code>choice_payload_match_rename_original_not_visible_rejected</code>], [$html:<code>choice_payload_match_rename_self_alias_success</code>], [$html:<code>choice_payload_match_named_assignment_rejected</code>], [$html:<code>choice_payload_match_missing_payload_rejected</code>], [$html:<code>choice_payload_match_unit_parens_rejected</code>], [$html:<code>choice_payload_match_duplicate_variant_rejected</code>], [$html:<code>choice_payload_match_guard_requires_else</code>]]
            [: Constructor-like patterns support declared payload field names and `field as local_name` aliases. Field matching remains positional and validated against declared field names. Exhaustiveness is tag-level.]
        ]

        [data:
            [: Choice payload field access]
            [: Deferred]
            [: [$html:<code>choice_payload_direct_field_access_deferred</code>]]
            [: No `value.field` until variant narrowing/refinement is designed. Use pattern matching to extract payload fields.]
        ]

        [data:
            [: Choice payload capture renames]
            [: Supported]
            [: [$html:<code>choice_payload_match_rename_success</code>], [$html:<code>choice_payload_match_rename_guard_success</code>], [$html:<code>choice_payload_match_rename_wrong_original_rejected</code>], [$html:<code>choice_payload_match_rename_duplicate_binding_rejected</code>], [$html:<code>choice_payload_match_rename_shadowing_rejected</code>], [$html:<code>choice_payload_match_rename_missing_alias_rejected</code>], [$html:<code>choice_payload_match_rename_non_symbol_alias_rejected</code>], [$html:<code>choice_payload_match_rename_original_not_visible_rejected</code>], [$html:<code>choice_payload_match_rename_self_alias_success</code>]]
            [: `Variant(original_name as local_name) =>` binds the payload field to a different local name visible in the guard and arm body.]
        ]

        [data:
            [: `as` keyword outside supported domains]
            [: Rejected / Reserved]
            [: [$html:<code>as_in_expression_rejected</code>], [$html:<code>as_in_variable_declaration_rejected</code>], [$html:<code>as_in_function_signature_rejected</code>], [$html:<code>as_in_non_choice_match_pattern_rejected</code>], [$html:<code>as_in_match_pattern_lead_rejected</code>]]
            [: `as` is only valid in three contexts: type aliases (`TypeAlias as ExistingType`), import clauses (`import @path as local_name` or `import @path { symbol as local_name }`), and choice payload captures (`Variant(field as local_name) =>`). All other uses are rejected with targeted diagnostics.]
        ]

        [data:
            [: General pattern binding renames]
            [: Deferred]
            [: No canonical case yet.]
            [: Non-choice capture renaming and nested payload pattern renaming remain deferred.]
        ]

        [data:
            [: Choice nested payload patterns / exhaustiveness]
            [: Deferred]
            [: [$html:<code>choice_nested_payload_pattern_deferred</code>]]
            [: Exhaustiveness is tag-level for now.]
        ]

        [data:
            [: Explicit facade export/re-export syntax]
            [: Partial]
            [: [$html:<code>module_facade_explicit_export_surface_success</code>], [$html:<code>module_facade_private_path_rejected</code>], [$html:<code>entry_root_facade_private_path_rejected</code>], [$html:<code>source_lib_facade_private_path_rejected</code>], [$html:<code>source_lib_facade_alias_private_path_rejected</code>], [$html:<code>module_facade_reexport_namespace_success</code>], [$html:<code>source_lib_reexport_namespace_success</code>], placement diagnostics for [$html:<code>export</code>] outside [$html:<code>#mod.bst</code>], bare namespace-export diagnostics, legacy [$html:<code>#import</code>] rejection coverage, plus explicit [$html:<code>export import</code>], grouped [$html:<code>export @path { Symbol }</code>], alias, external-symbol, module-root, source-library, and duplicate-public-name coverage.]
            [: [$html:<code>#mod.bst</code>] facades now expose authored declarations with [$html:<code>export</code>] and imported behavior with [$html:<code>export import @path { Symbol }</code>] or [$html:<code>export @path { Symbol }</code>] sugar. Unmarked facade declarations and ordinary [$html:<code>import</code>] entries remain private to the facade file, so private helpers, private facade-only types, and private imports can support public wrappers without becoming API. Re-export aliases define the public API name, source and external package targets are supported, namespace imports over re-export-only facades expose the public facade entries, and public exported APIs reject private type surfaces with [$html:<code>BST-RULE-0082</code>]. Cross-module, entry-root, and source-library importers cannot bypass facades by spelling private implementation paths, even when an exported symbol shares the same final name. Legacy [$html:<code>#import</code>] still fails with [$html:<code>BST-RULE-0025</code>]. Bare namespace exports, wildcard exports, function alias values, and automatic method re-export through [$html:<code>#mod.bst</code>] type aliases remain deferred.]
        ]

        [data:
            [: Source-library HIR caching]
            [: Deferred]
            [: No precompiled source-library cache.]
            [: Source libraries are compiled from `.bst` source each build. Benchmark counters may quantify this cost, but HIR caching is deferred until the library system is stable.]
        ]

        [data:
            [: Package manager]
            [: Deferred]
            [: No package manager, remote registries, or dependency resolution.]
            [: The library system plan explicitly defers package management, versioning, lockfiles, and remote fetching.]
        ]

        [data:
            [: External JS library expansion]
            [: Deferred / Reserved]
            [: JS parser diagnostics reject unsupported library forms.]
            [: The HTML builder supports annotated single-file [$html:<code>.js</code>] imports and the built-in [$html:<code>@web/canvas</code>] package. Deferred surfaces include JS constants, classes, arbitrary dependency graphs, default exports, [$html:<code>export { name }</code>] re-exports, CommonJS, object properties/getters/setters, callbacks/function types, async functions/promises, multi-success returns, external generic types, collections/options in JS signatures, cross-package external type unification, text/CSS/HTML imports as strings, source maps, tree-shaking, and minification.]
        ]

        [data:
            [: Namespace / wildcard imports]
            [: Partial]
            [: Source-file, module-facade, re-export-only facade, receiver-method visibility through source types, Rust-side external package, and provider-backed JS package namespace records are covered; wildcard imports remain deferred.]
            [: Namespace imports such as [$html:<code>import @./helper</code>], [$html:<code>import @core/math</code>], and [$html:<code>import @web/canvas</code>] create shallow field-access-only records. Members can be used as [$html:<code>helper.render()</code>], [$html:<code>math.sin(1.0)</code>], or [$html:<code>canvas.fill_rect(~ctx, 0.0, 0.0, 100.0, 100.0)</code>], source and external type members can be used in type position as [$html:<code>models.User</code>] or [$html:<code>canvas.Canvas2d</code>], and source receiver methods remain available only through receiver-call syntax when the receiver type is visible. Facade namespace imports use the explicit facade public export set, including import-only and re-export-only facades. Import records cannot be used as values, namespace records do not expose nested filesystem traversal, receiver methods are not namespace fields, and wildcard syntax such as [$html:<code>import @pkg.*</code>] remains unsupported.]
        ]

        [data:
            [: Choice recursive types]
            [: Deferred]
            [: [$html:<code>choice_recursive_direct_declaration_deferred</code>]]
            [: Requires layout/indirection design.]
        ]

        [data:
            [: Generic type infrastructure]
            [: Supported]
            [: Targeted frontend unit coverage for TypeId-native generic instance identity, generic binding consistency, TypeId binding/unification for generic parameters, collections, options, nominal generic instances, generic free-function templates, constructed-type substitution, type-identity compatibility keys, parameter scopes, generic parameter parsing, [$html:<code>of</code>] type applications, arity checks, collection generic substrate, generic receiver constructor lookup, recursive HIR-boundary generic-parameter rejection, and diagnostics. Integration coverage includes duplicate, invalid, builtin-colliding, and visible-type-colliding generic parameter diagnostics; generic struct and choice constructor inference; concrete generic aliases; same-module imported generic types and aliases; named argument inference; conflicting inference diagnostics; generic function declaration parsing; generic body validation; local generic function call inference and instantiation; aligned declaration-site generic receiver methods; same-module grouped-import and namespace generic function calls; grouped alias calls; cross-file and facade expected-result inference; cross-file and facade optional returns; cross-file and facade fallible error slots; generic function value diagnostics; authored facade wrapper generic calls through grouped, namespace, and grouped-alias imports; facade privacy regressions; direct [$html:<code>#mod.bst</code>] import rejection; facade import-alias non-reexport rejection; constructed collection returns; ambiguous/conflicting function inference diagnostics; recursive generic function instantiation rejection with call-site context; unsupported receiver rejection; nested [$html:<code>of</code>] rejection; collection capacity parsing; generic function and nominal generic trait bounds; and HTML-Wasm reachable/unreachable generic-runtime validation.]
            [: The current generic free-function import-hardening work is implemented through the AST generic free-function template and concrete instance model. Generic parameter metadata, [$html:<code>of</code>] type applications, concrete generic aliases, collection generic substrate, constructor inference, choice matching, AST unresolved-generic validation, concrete HIR nominal identity, HIR validation rejection for unresolved generic parameters, AST-owned generic function templates, aligned generic receiver methods, template-aware generic body validation, same-file calls, same-module grouped-import or namespace calls, authored facade-wrapper calls, constructed-type substitution coverage, imported generic optional/fallible returns, generic function trait bounds, generic struct/choice trait-bound instantiation, bound-provided receiver calls for reusable evidence, and generic diagnostic hardening are implemented. Generic receiver calls reuse the generic function inference and concrete instance path instead of a separate method-instantiation system. The generic substrate is split by owner across [$html:<code>generic_parameters</code>], [$html:<code>generic_bindings</code>], [$html:<code>generic_identity_bridge</code>], and [$html:<code>ast/generic_functions</code>]. Nested inline generic applications, recursive generic types, generic aliases with parameters, partial application, explicit generic call syntax, generic function values, non-reusable conformance dispatch, namespace/wildcard facade exports, concrete generic-instance receiver declarations, and generic external package functions remain deferred or rejected.]
        ]

        [data:
            [: Choice generic declarations]
            [: Supported]
            [: [$html:<code>generic_choice_declaration_parse_success</code>], [$html:<code>generic_choice_instantiation_success</code>], [$html:<code>generic_choice_match_success</code>], [$html:<code>generic_named_choice_payload_infer</code>], [$html:<code>generic_expected_unit_choice_success</code>], [$html:<code>generic_choice_receiver_method_success</code>], [$html:<code>generic_recursive_type_deferred</code>], [$html:<code>trait_bound_generic_choice_success</code>]]
            [: Generic choice headers and payload type parameters parse and resolve. Concrete constructor inference, named payload arguments, expected-type unit variants, matching on concrete generic instances, aligned declaration-site receiver methods, and declaration-site trait bounds requiring visible reusable evidence are supported. Recursive generic choices and concrete generic-instance receiver declarations remain rejected.]
        ]

        [data:
            [: Generic structs]
            [: Supported]
            [: [$html:<code>generic_struct_instantiation_success</code>], [$html:<code>generic_named_struct_args_infer</code>], [$html:<code>generic_conflicting_inference_rejected</code>], [$html:<code>generic_collection_element_success</code>], [$html:<code>generic_imported_struct_choice_success</code>], [$html:<code>generic_import_alias_concrete_alias_success</code>], [$html:<code>generic_receiver_method_success</code>], [$html:<code>generic_recursive_type_deferred</code>], [$html:<code>trait_bound_generic_struct_success</code>], [$html:<code>trait_bound_generic_struct_annotation_missing_evidence_rejected</code>], [$html:<code>trait_bound_generic_struct_constructor_missing_evidence_rejected</code>], [$html:<code>trait_bound_generic_alias_missing_evidence_rejected</code>]]
            [: Generic struct headers parse and resolve. Concrete constructor inference, field type substitution, named constructor arguments, imports and import aliases, collection element types, aligned declaration-site receiver methods, declaration-site trait bounds requiring visible reusable evidence, and concrete HIR lowering are supported. Recursive generic structs and concrete generic-instance receiver declarations remain rejected. HTML-Wasm generic runtime values that are reachable from the exported entry roots fail with structured unsupported-backend diagnostics before Wasm lowering; unreachable generic-runtime code in unused functions is ignored by that backend-specific validation.]
        ]

        [data:
            [: Generic choices]
            [: Supported]
            [: [$html:<code>generic_choice_instantiation_success</code>], [$html:<code>generic_named_choice_payload_infer</code>], [$html:<code>generic_choice_match_success</code>], [$html:<code>generic_choice_equality_success</code>], [$html:<code>generic_choice_equality_unsupported_payload_rejected</code>]]
            [: Concrete generic choice constructors, matching, and equality are implemented for resolved nominal instances when the substituted payload types support the operation. Wasm backend support for payload choices remains partial and is tracked by backend-specific expected failures.]
        ]

        [data:
            [: Generic functions]
            [: Partial]
            [: [$html:<code>generic_function_declaration_parse_success</code>], [$html:<code>generic_function_body_validation_success</code>], [$html:<code>generic_function_local_annotations_success</code>], [$html:<code>generic_function_local_unknown_annotation_rejected</code>], [$html:<code>generic_parameter_name_outside_scope_rejected</code>], [$html:<code>generic_function_body_arithmetic_rejected</code>], [$html:<code>generic_function_body_io_rejected</code>], [$html:<code>generic_function_body_template_rejected</code>], [$html:<code>generic_function_body_field_access_rejected</code>], [$html:<code>generic_function_identity_success</code>], [$html:<code>generic_function_wrapper_success</code>], [$html:<code>generic_function_expected_context_success</code>], [$html:<code>generic_fn_nested_expected_parameter_intermediate_success</code>], [$html:<code>generic_constructed_types_success</code>], [$html:<code>generic_function_optional_return_success</code>], [$html:<code>generic_optional_none_inference_rejected</code>], [$html:<code>generic_function_ambiguous_inference_rejected</code>], [$html:<code>generic_fn_nested_expected_parameter_rejected</code>], [$html:<code>generic_function_conflicting_inference_rejected</code>], [$html:<code>generic_function_recursive_instantiation_rejected</code>], [$html:<code>generic_fn_cross_file_grouped_success</code>], [$html:<code>generic_fn_cross_file_grouped_alias_success</code>], [$html:<code>generic_fn_cross_file_conflicting_inference_rejected</code>], [$html:<code>generic_fn_cross_file_namespace_success</code>], [$html:<code>generic_fn_cross_file_namespace_expected_result_success</code>], [$html:<code>generic_fn_cross_file_expected_result_collection_success</code>], [$html:<code>generic_fn_facade_expected_result_collection_success</code>], [$html:<code>generic_fn_cross_file_optional_return_success</code>], [$html:<code>generic_fn_facade_optional_return_success</code>], [$html:<code>generic_fn_cross_file_fallible_error_slot_success</code>], [$html:<code>generic_fn_facade_fallible_error_slot_success</code>], [$html:<code>generic_fn_uninferable_parameter_rejected</code>], [$html:<code>generic_fn_later_use_inference_rejected</code>], [$html:<code>generic_fn_multiple_substitutions_same_module_success</code>], [$html:<code>generic_fn_value_use_grouped_rejected</code>], [$html:<code>generic_fn_value_use_namespace_rejected</code>], [$html:<code>generic_fn_value_use_collection_rejected</code>], [$html:<code>generic_fn_value_use_argument_rejected</code>], [$html:<code>generic_fn_value_use_return_rejected</code>], [$html:<code>generic_fn_value_use_facade_rejected</code>], [$html:<code>generic_fn_namespace_synthetic_instance_inaccessible_rejected</code>], [$html:<code>generic_fn_synthetic_grouped_import_rejected</code>], [$html:<code>generic_fn_synthetic_direct_symbol_path_import_rejected</code>], [$html:<code>generic_fn_synthetic_facade_export_rejected</code>], [$html:<code>generic_fn_facade_grouped_success</code>], [$html:<code>generic_fn_facade_namespace_success</code>], [$html:<code>generic_fn_facade_grouped_alias_success</code>], [$html:<code>generic_fn_facade_private_impl_import_rejected</code>], [$html:<code>generic_fn_facade_direct_mod_import_rejected</code>], [$html:<code>generic_fn_facade_import_alias_not_reexported_rejected</code>], [$html:<code>generic_fn_instance_body_type_error_rejected</code>], [$html:<code>generic_fallible_function_success</code>], [$html:<code>generic_fallible_body_specialization_rejected</code>], [$html:<code>generic_unused_parameter_rejected</code>]]
            [: Generic function headers parse into AST-owned templates with canonical signature [$html:<code>TypeId</code>]s. Template-aware body validation rejects behavior-dependent operations on unconstrained generic parameters while allowing pass-through, generic-to-generic forwarding, generic wrapper construction, local annotations using generic parameters, constructed collection returns, contextual optional returns, and final generic error slots. Same-file, same-module grouped-import, same-module namespace, and authored facade-wrapper calls infer type arguments from immediate arguments and immediate expected success-result context, then emit concrete AST/HIR-bound function instances. Synthetic generic instance paths stay internal and cannot be reached through namespace records, grouped imports, direct symbol-path imports, or facade import surfaces. Facade wrapper bodies resolve through the facade file's visibility, so private implementation generic helpers remain usable by the wrapper but hidden from outside importers. Generic optional returns and generic fallible error slots work across visible imports when immediate evidence solves the type parameters. Concrete instance body-emission diagnostics make the instantiating call site primary and keep the generic body failure span, declaration site, and concrete substitutions as diagnostic context. Conflicting inference diagnostics render both concrete [$html:<code>TypeId</code>] names through the diagnostic type context, and cannot-infer diagnostics suggest ordinary receiving-site annotations. Generic fallible calls use the same postfix [$html:<code>!</code>] and [$html:<code>catch</code>] handling as ordinary fallible calls. [$html:<code>none</code>] alone cannot infer a generic optional inner type. Inference does not flow from later use or from an outer expected parameter context into a nested generic call. Generic functions must still be called and cannot be used as first-class values.]
        ]

        [data:
            [: Concrete generic aliases]
            [: Supported]
            [: [$html:<code>generic_concrete_alias_success</code>], [$html:<code>generic_import_alias_concrete_alias_success</code>], [$html:<code>generic_nested_alias_workaround</code>]]
            [: Aliases to fully concrete generic instances are supported and transparent in annotations and declaration/constructor compatibility checks, including aliases to imported generic declarations. Generic aliases with parameters are a separate deferred surface.]
        ]

        [data:
            [: Trait bounds on generics]
            [: Partial]
            [: [$html:<code>trait_bound_function_success</code>], [$html:<code>trait_bound_multiple_and_success</code>], [$html:<code>trait_bound_multiple_params_success</code>], [$html:<code>trait_bound_missing_evidence_rejected</code>], [$html:<code>trait_bound_imported_private_trait_rejected</code>], [$html:<code>public_generic_bound_public_trait_success</code>], [$html:<code>public_generic_bound_reexported_trait_success</code>], [$html:<code>public_generic_bound_private_trait_rejected</code>], [$html:<code>trait_bound_cross_file_grouped_success</code>], [$html:<code>trait_bound_cross_file_namespace_success</code>], [$html:<code>trait_bound_cross_file_grouped_alias_success</code>], [$html:<code>trait_bound_facade_grouped_wrapper_success</code>], [$html:<code>trait_bound_must_syntax_rejected</code>], [$html:<code>trait_bound_invalid_trait_name_rejected</code>], [$html:<code>generic_bound_receiver_call_success</code>], [$html:<code>generic_bound_receiver_multiple_params_success</code>], [$html:<code>generic_bound_receiver_same_concrete_params_success</code>], [$html:<code>generic_bound_receiver_ambiguous_rejected</code>], [$html:<code>generic_bound_file_local_evidence_rejected</code>], [$html:<code>generic_where_constraint_syntax_rejected</code>], [$html:<code>trait_bound_trait_name_generic_argument_rejected</code>], [$html:<code>trait_bound_generic_struct_success</code>], [$html:<code>trait_bound_generic_choice_success</code>], [$html:<code>trait_bound_generic_struct_annotation_missing_evidence_rejected</code>], [$html:<code>trait_bound_generic_struct_constructor_missing_evidence_rejected</code>], [$html:<code>trait_bound_generic_alias_missing_evidence_rejected</code>], [$html:<code>trait_bound_generic_matching_method_without_conformance_rejected</code>], [$html:<code>trait_bound_generic_file_local_evidence_rejected</code>]]
            [: Declaration-site [$html:<code>type A is TRAIT</code>] and [$html:<code>type A is TRAIT_A and TRAIT_B</code>] bounds are supported for generic functions, structs, and choices. Concrete generic function calls and concrete nominal generic instances require explicit visible reusable trait evidence for each inferred or substituted concrete type argument, and matching methods without [$html:<code>Type must TRAIT</code>] do not satisfy bounds. Public facade generic functions, structs, and choices reject private or non-exported trait bounds in their signatures, while traits re-exported through the same facade surface are accepted. Bound generic functions work across grouped imports, grouped aliases, namespace function imports, and facade grouped wrappers when the needed trait and evidence are visible. Generic function bodies can call a unique receiver method supplied by a generic bound; ordinary visible concrete receiver methods still resolve first, multiple bound methods with the same name are rejected as ambiguous, and same-concrete multi-parameter instances retain each receiver parameter's source-bound surface. Trait names cannot appear as value types or concrete generic substitutions. Commas still separate generic parameters. [$html:<code>must</code>] in a generic parameter list is rejected with guidance to use [$html:<code>is</code>], and [$html:<code>where</code>] syntax remains rejected. File-local evidence-backed generic-bound dispatch is outside scope.]
        ]

        [data:
            [: Explicit generic call-site application]
            [: Rejected / Not Alpha]
            [: [$html:<code>generic_explicit_call_of_syntax_rejected</code>], [$html:<code>generic_explicit_call_of_spacing_rejected</code>], [$html:<code>generic_explicit_call_angle_syntax_rejected</code>], [$html:<code>generic_explicit_call_square_syntax_rejected</code>], [$html:<code>generic_explicit_call_argument_type_syntax_rejected</code>], [$html:<code>generic_explicit_call_inference_annotation_success</code>]]
            [: Explicit call-site type arguments such as [$html:<code>identity of Int(42)</code>], [$html:<code>identity&lt;Int&gt;(42)</code>], [$html:<code>["identity[Int](42)"]</code>], and [$html:<code>identity(42 Int)</code>] are not Alpha syntax. Local generic function calls infer type arguments from immediate arguments and immediate expected result context only.]
        ]

        [data:
            [: Generic function values]
            [: Rejected / Deferred]
            [: [$html:<code>generic_fn_value_use_grouped_rejected</code>], [$html:<code>generic_fn_value_use_namespace_rejected</code>], [$html:<code>generic_fn_value_use_collection_rejected</code>], [$html:<code>generic_fn_value_use_argument_rejected</code>], [$html:<code>generic_fn_value_use_return_rejected</code>], [$html:<code>generic_fn_value_use_facade_rejected</code>]]
            [: Generic functions are callable declarations only. They cannot be assigned, passed as arguments, returned, collected, or used through namespace members or facade exports as first-class values.]
        ]

        [data:
            [: Higher-order polymorphism]
            [: Deferred]
            [: None]
            [: Generic function values, polymorphic function parameters, and higher-order generic calls are outside the Alpha surface.]
        ]

        [data:
            [: Generic receiver methods]
            [: Partial]
            [: [$html:<code>generic_receiver_method_success</code>], [$html:<code>generic_choice_receiver_method_success</code>], [$html:<code>generic_receiver_method_parameter_mismatch_rejected</code>], [$html:<code>generic_receiver_rejected</code>]]
            [: Aligned declaration-site generic receiver methods are supported for generic structs and choices when the method generic parameters exactly match the receiver constructor arguments. Calls on concrete generic instances reuse generic function inference and emit normal concrete function instances. Receiver methods declared directly on concrete generic instances and mismatched generic parameter order remain rejected; generic trait bounds are supported.]
        ]

        [data:
            [: Recursive generic types]
            [: Deferred]
            [: [$html:<code>generic_recursive_type_deferred</code>], [$html:<code>generic_mutual_recursive_rejected</code>]]
            [: Recursive generic structs and choices are rejected through declaration dependency or generic recursion diagnostics. Requires layout/indirection design.]
        ]

        [data:
            [: Nested generic type application]
            [: Deferred / Rejected]
            [: [$html:<code>generic_nested_alias_workaround</code>], [$html:<code>generic_nested_of_rejected</code>]]
            [: More than one [$html:<code>of</code>] application in a single annotation is rejected, except for collection element types. Name intermediate concrete aliases to compose complex types.]
        ]

        [data:
            [: Parameterized generic aliases]
            [: Deferred / Rejected]
            [: [$html:<code>generic_parameterized_alias_rejected</code>]]
            [: Alias a fully concrete generic instance for now. Alias parameter substitution is not implemented, and parameterized alias declarations are rejected during header parsing.]
        ]

        [data:
            [: Partial type application]
            [: Rejected]
            [: [$html:<code>generic_partial_type_application_rejected</code>]]
            [: Partially applied type constructors and placeholder-style type arguments are not supported. Alias a fully concrete generic instance instead.]
        ]

        [data:
            [: Generic external package functions]
            [: Deferred / Rejected]
            [: [$html:<code>js_import_generic_signature_rejected</code>], [$html:<code>js_import_generic_opaque_type_rejected</code>], [$html:<code>generic_external_opaque_type_application_rejected</code>]]
            [: External package functions and external package types are concrete only for Alpha. JS provider signatures, opaque declarations, source type annotations, and backend package metadata must expose concrete surfaces.]
        ]

        [data:
            [: Type values / type-level CTFE]
            [: Not supported]
            [: [$html:<code>generic_type_name_value_rejected</code>], [$html:<code>generic_type_application_value_rejected</code>], [$html:<code>generic_namespace_type_value_rejected</code>]]
            [: Types are not runtime or compile-time values. Type-returning functions, type-level [$html:<code>#if</code>], type inspection, and generic call-site type expressions are outside the Alpha model.]
        ]

        [data:
            [: Choice equality]
            [: Supported]
            [: [$html:<code>choice_unit_equality_success</code>], [$html:<code>choice_unit_equality_imported_variant_success</code>], [$html:<code>choice_unit_equality_aliased_import_success</code>], [$html:<code>choice_unit_equality_in_function_return</code>], [$html:<code>choice_payload_equality_same_payload_true</code>], [$html:<code>choice_payload_equality_different_payload_false</code>], [$html:<code>choice_payload_equality_unit_vs_payload_false</code>], [$html:<code>choice_payload_equality_same_choice_different_variants_false</code>], [$html:<code>choice_payload_equality_nested_choice_true</code>], [$html:<code>choice_payload_equality_nested_choice_false</code>], [$html:<code>choice_equality_different_choice_types_rejected</code>], [$html:<code>choice_diagnostic_choice_vs_non_choice_rejected</code>], [$html:<code>choice_equality_payload_unsupported_struct_rejected</code>], [$html:<code>choice_equality_payload_unsupported_collection_rejected</code>], [$html:<code>choice_equality_payload_unsupported_external_rejected</code>], [$html:<code>generic_choice_equality_success</code>], [$html:<code>generic_choice_equality_unsupported_payload_rejected</code>], [$html:<code>choice_payload_structural_equality_success</code>], [$html:<code>choice_equality_positional_constructor_success</code>], [$html:<code>choice_equality_named_constructor_success</code>], [$html:<code>choice_equality_imported_constructor_success</code>], [$html:<code>choice_equality_aliased_constructor_success</code>], [$html:<code>choice_equality_constructor_wrong_arity_rejected</code>], [$html:<code>choice_equality_constructor_wrong_field_rejected</code>]]
            [: Structural equality is supported for unit, payload, and concrete generic choices in type checking and JS lowering. Payload fields are compared recursively when all substituted fields support equality. Unsupported payload field types, different choice types, and choice-vs-non-choice comparisons are rejected with specific diagnostics.]
        ]

        [data:
            [: Choice tagged/default declarations]
            [: Deferred]
            [: No canonical tagged-declaration case yet; [$html:<code>choice_default_decl_deferred</code>] covers default declaration rejection.]
            [: These belong to the fuller tagged-union design, not the current Alpha subset.]
        ]

        [data:
            [: Choice default variant values]
            [: Deferred]
            [: [$html:<code>choice_default_decl_deferred</code>], [$html:<code>choice_variant_default_value_deferred</code>]]
            [: Struct field defaults are supported separately. Choice payload defaults and default variant values remain deferred.]
        ]

        [data:
            [: Wasm payload layout for choices]
            [: Experimental]
            [: No canonical case yet.]
            [: JS is the supported Alpha backend. Wasm backend lowering for choice payloads remains experimental.]
        ]

        [data:
            [: Negated match patterns]
            [: Deferred]
            [: [$html:<code>diagnostic_match_negated_pattern</code>]]
            [: Should wait until the match exhaustiveness model is deliberately extended.]
        ]

        [data:
            [: General capture / tagged match patterns]
            [: Supported]
            [: [$html:<code>pattern_capture_binds_scrutinee_success</code>], [$html:<code>pattern_capture_before_else_makes_else_unreachable_warning</code>], [$html:<code>pattern_capture_after_literal_warning_or_reachable</code>], [$html:<code>pattern_unreachable_after_capture_warning</code>], [$html:<code>pattern_capture_whole_choice_value_success</code>], [$html:<code>pattern_choice_payload_existing_capture_alias_success</code>], [$html:<code>option_match_patterns_success</code>]]
            [: General capture patterns (for example `captured =>`) are supported for both choice and non-choice scrutinees. Choice payload capture patterns with `field as local_name` aliases are also supported. Current option matches support [$html:<code>none =&gt;</code>], literal and relational present-value checks, and option present capture with [$html:<code>|value|</code>]. Nested payload patterns, option inner choice patterns, branch narrowing outside explicit captures, and general `|...|` tagged patterns remain deferred.]
        ]

        [data:
            [: Rich numeric redesign]
            [: Deferred]
            [: Unsupported syntax should fail cleanly.]
            [: Includes arbitrary precision compile-time arithmetic, decimal type work, and more specific numeric widths.]
        ]
    ]
]

#[section, $markdown:
    # Maintenance rule

    When a language feature changes shape:

    1. Update this page.
    2. Update `tests/cases/manifest.toml`.
    3. Add or rewrite canonical cases instead of layering temporary coverage.
    4. Mark unsupported syntax as `Deferred` or `Experimental`, not as a vague missing test.
    5. Do not count HTML-Wasm as Alpha-supported unless the roadmap explicitly changes.
    6. When external packages change, update the package registry, backend lowering, import visibility tests, and this matrix together.

    The goal is to keep the Alpha surface small, honest, and fully exercised.
]
