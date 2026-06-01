import @html {center}
import @styles/docs {navbar, title, section, palette, styled_table, header, data, codeblock, codesnippet, theme_head}

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

    Use this together with [codesnippet:tests/cases/manifest.toml] when changing language features.

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
            [: Only the module entry file runs top-level runtime code. Non-entry top-level executable code and runtime templates are rejected. The implicit [codesnippet:start] function is build-system-owned and is not an importable/callable user declaration.]
        ]

        [data:
            [: Structured diagnostics]
            [: Supported]
            [: Import, import path resolution, compile-time path literal validation, name-resolution, semantic-rule, type, borrow, deferred-feature, unsupported external-package/backend-feature validation, Stage 0 config parsing/value validation, Stage 0 project-structure validation, HTML project config validation, and AST expression/template diagnostics use typed CompilerDiagnostic payloads with stable codes. Infrastructure failures use a dedicated stable infrastructure code, and warnings share the same ordered diagnostic stream.]
            [: Frontend]
            [: Do not add legacy user-diagnostic macros, rendered-string payloads, or user-facing CompilerError routes. AST and borrow source failures use [codesnippet:CompilerDiagnostic]; HIR keeps [codesnippet:CompilerError] / [codesnippet:ErrorType::HirTransformation] for invariant failures only. Local fallible compiler-stage helpers and diagnostic boundary enums may box large [codesnippet:CompilerDiagnostic] / [codesnippet:CompilerError] payloads to keep Result boundaries small, but they unbox only at [codesnippet:DiagnosticBag], [codesnippet:CompilerMessages], or render boundaries so this remains an internal transport detail rather than a user-facing diagnostic-shape change. Executable AST and HIR call/result typing is [codesnippet:TypeId]-first, with [codesnippet:DataType] retained only for parse and diagnostic spelling. Frontend boundary cleanup requires manual review of AST/HIR ownership, diagnostic routing, and type-representation paths. Remaining future work is limited to JSON diagnostic output, LSP mapping, localization, and any new diagnostics required by future language features.]
        ]

        [data:
            [: Variables, assignment, and scoped blocks]
            [: Supported]
            [: Broad declaration, reassignment, mutability, scope, shadowing, undefined-mutation, block-scope, and diagnostics coverage.]
            [: JS / HTML]
            [: Mutable writes require mutable places. [codesnippet:block:] creates a lexical/control-flow scope. Bare labeled scopes remain rejected.]
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
            [: [codesnippet:assert] is a language-owned statement keyword and cannot be shadowed, imported, assigned, passed, or used in expression position. [codesnippet:assert(condition)] and [codesnippet, "assert(condition, \"message\")"] are always checked. Failed assertions are unrecoverable and cannot be caught. [codesnippet, "assert(false, \"message\")"] is the only current explicit source-level panic path and is statically terminal for value-required functions and [codesnippet:catch] handlers. Expected failures should use [codesnippet:Error!] and [codesnippet:catch]. Messages are optional string literals; runtime and compile-time constant message expressions remain deferred.]
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
            [: Function parameter defaults, struct field defaults, named-call default skipping, constructor default skipping, all-default struct constructors, builtin [codesnippet:Error.code] defaulting, and default diagnostics are covered.]
            [: Frontend / JS / HTML]
            [: Function parameters and struct fields can define defaults with [codesnippet, "="]. Named calls and constructors can omit defaulted slots, including earlier defaulted slots when later required slots are supplied by name. Choice payload defaults, choice default variants, and broad default-argument semantics outside function/struct construction remain deferred.]
        ]

        [data:
            [: Templates and style directives]
            [: Supported]
            [: Broad template parsing, slot, directive, const-fragment, and runtime-fragment coverage, with targeted unit coverage for template head suffix control-flow parsing and diagnostics, const Bool [codesnippet:if] folding including dependency-sorted source const references, runtime Bool [codesnippet:if] HIR lowering including expression-level [codesnippet:!] propagation, const option-present [codesnippet:if] folding for already-decidable option values and source const scrutinees, runtime option-present [codesnippet:if] HIR lowering, [codesnippet, "[else]"], [codesnippet, "[else if ...]"], grammar-boundary diagnostics for unsupported match-style template heads, [codesnippet:case] as an ordinary identifier, const conditional loops, const range loops, const collection loops, runtime conditional / range / collection loop HIR lowering, and runtime slot applications with branch, loop, loop-control in append contexts, repeated slot single-evaluation/source-site behavior, nested slot sites, mixed static/runtime ordering, [codesnippet:$children(...)], and [codesnippet:$fresh] coverage.]
            [: HTML / JS]
            [: This is one of the main Alpha surfaces. Template head suffix control flow is supported for source-authored Bool [codesnippet:if] and option-present [codesnippet:if] branching, conditional [codesnippet:loop], range [codesnippet:loop], collection [codesnippet:loop], standalone [codesnippet, "[else]"], standalone [codesnippet, "[else if ...]"] branch chains, and structural [codesnippet, "[break]"] / [codesnippet, "[continue]"] loop-control sentinels in runtime-capable contexts. Template heads do not support full pattern-match branch chains; use ordinary statement/value [codesnippet, "if value is:"] blocks outside templates for that shape. [codesnippet:case] is not a keyword or template sentinel and remains valid as an ordinary identifier. Const-required Bool [codesnippet:if], already-decidable option-present [codesnippet:if], [codesnippet, "[else if ...]"] branch chains, conditional [codesnippet:loop] with a compile-time false condition, range [codesnippet:loop], collection [codesnippet:loop], and structural loop-control sentinels inside const range/collection loop bodies fold during AST construction. Const-required template [codesnippet:if] conditions, option scrutinees and conditional-loop conditions can reference dependency-sorted source constants, with same-file forward references still rejected by the header dependency pass. Runtime slot applications support default, named, positional, loose contribution routing, repeated slots, missing slots as empty, [codesnippet:$insert(...)], runtime branch/loop contributions, loop-control propagation inside template loop append contexts, source evaluation once with slot-site replay, site-local [codesnippet:$children(...)] / [codesnippet:$fresh] wrapper behavior, and mixed static/runtime contribution ordering. Supported paths preserve structural no-output semantics, lazy runtime branch/loop lowering, nearest-template-loop [codesnippet, "[break]"] / [codesnippet, "[continue]"] targeting, output-before-loop-control preservation, expression-level [codesnippet:!] propagation, runtime slot/insert leakage diagnostics for invalid helper artifacts, literal-body [codesnippet, "[else]"] / [codesnippet, "[else if ...]"] / loop-control rejection, malformed/orphan/inline sentinel diagnostics, and the [codesnippet:template_const_loop_iteration_limit] const expansion guard with a default of [codesnippet:10_000] iterations and a hard maximum of [codesnippet:1_000_000].]
        ]

        [data:
            [: Structs, records, and receiver methods]
            [: Supported]
            [: Broad integration coverage with targeted parser, struct-default, constructor-default, and receiver-catalog unit coverage.]
            [: JS / HTML]
            [: Field mutation, nested field mutation, nominal mismatch, constructor named arguments, struct field defaults, receiver borrow behavior, scalar receiver methods, and cross-file receiver use are covered. Keep Wasm receiver behavior experimental.]
        ]

        [data:
            [: Choices]
            [: Supported]
            [: Broad parser, declaration, constructor, match, import, assignment, return, JS carrier-shape, payload coverage, unit equality, and payload structural equality coverage.]
            [: JS / HTML]
            [: Unit variants, payload variants, constructor calls, imports, assignment, return, JS carrier shape, payload matching, and structural equality are supported. Structural equality compares variant identity and payload fields recursively when every payload field supports equality. Payload fields are immutable. Payload shorthand, generic choices, recursive choices, nested payload patterns, direct payload field access, and variant defaults remain deferred or rejected with diagnostics.]
        ]

        [data:
            [: Pattern matching]
            [: Partial]
            [: Broad literal, choice-match, and payload-capture coverage, including exhaustiveness and diagnostics.]
            [: JS / HTML]
            [: Literal patterns, choice variant patterns, payload extraction, relational patterns including strings, guards, [codesnippet:else =>] arms, empty [codesnippet:else =>] no-op fallback arms, and general capture patterns are supported. Wildcard patterns using [codesnippet:_] and no-op bodies such as [codesnippet:else => _] are permanently rejected; use [codesnippet:else =>] for fallback and an empty [codesnippet:else =>] to do nothing. Obvious unreachable arms warn. Full relational overlap analysis and nested choice payload patterns remain deferred.]
        ]

        [data:
            [: Collections]
            [: Supported]
            [: Broad collection literal, mutation, builtin method, indexing, loop, result, and JS helper coverage. New capacity parsing coverage.]
            [: JS / HTML]
            [: Collections internally use the generic type infrastructure. Public syntax remains [codesnippet:{T}] and [codesnippet:{T capacity}]. Capacity is allocation metadata, not part of type identity. Collection [codesnippet:get], [codesnippet:set], and [codesnippet:remove] are fallible; [codesnippet:push] and [codesnippet:length] are infallible. Collection helper runtime contracts should remain JS-owned for Alpha. HTML-Wasm collection behavior is experimental only.]
        ]

        [data:
            [: Collection generic substrate]
            [: Supported]
            [: Broad existing collection tests plus new capacity parsing tests.]
            [: JS / HTML]
            [: Internally represented through canonical [codesnippet:TypeEnvironment] constructed collection types. HIR carries frontend [codesnippet:TypeId] values and backends classify collection types through [codesnippet:TypeEnvironment] queries. Capacity parsed into declaration metadata but not wired to codegen yet.]
        ]

        [data:
            [: Results, options, multiple returns, and multi-bind]
            [: Partial]
            [: Broad current result, option, block and inline [codesnippet:catch:] fallback forms with shared [codesnippet:then] value production, [codesnippet, "catch |err|:"] handler blocks, value-producing [codesnippet:if] and full [codesnippet:if value is:] match blocks at declaration initializers, simple assignment RHS sites, [codesnippet:return if] sites including multi-return, and multi-bind RHS sites, inline non-option choice-pattern value [codesnippet:if] predicates, inline option unwrap value [codesnippet:if] with present capture [codesnippet, "|value|"], postfix result propagation including [codesnippet:E -> E?] error-slot wrapping, postfix option propagation [codesnippet:?], fallible external calls, cast, multi-bind, nested-option, contextual option wrapping, option-aware equality, option present capture [codesnippet, "|value|"], limited option match patterns, and public [codesnippet:Option of T] / [codesnippet:Result of T, E] deferred-syntax diagnostic coverage. Boundary-only [codesnippet:catch] coverage includes function-call arguments, collection items, loop headers, template interpolation, parenthesized arithmetic expressions, and field-access bases. Catch diagnostics cover empty and multiple handler bindings, handler fallthrough, fallback arity, fallback type mismatch, multiline inline fallback rejection, and same-line inline catch chaining before HIR. Invalid-form diagnostics now include empty [codesnippet:then], blocked [codesnippet:then], non-receiving value blocks, direct option fallback syntax, and unsupported optional [codesnippet:none] / literal single-predicate value [codesnippet:if] forms. Lower-level coverage includes value-production completeness, HIR value-block CFG/result locals, option-present capture extraction, JS option-present match conditions, and hidden-result materialization.]
            [: JS / HTML]
            [: Multi-bind supports explicit multi-return function calls plus value-producing [codesnippet:if condition then a else b], inline non-option choice-pattern predicates such as [codesnippet:if status is Ready then a else b], inline option present-capture unwraps such as [codesnippet, "if maybe is |value| then value else fallback"], block [codesnippet:if condition:] / [codesnippet:else], full [codesnippet:if value is:] match, and block/inline [codesnippet:catch] recovery forms as RHS expressions. These value-producing forms are also accepted at declaration initializers, simple assignment RHS receivers, and return receivers; multi-return [codesnippet:return if] lowers through internal tuple construction. Inline non-option choice predicates and inline option unwraps reuse the full-match pattern parser and lower as one-arm value matches with an inline default. They lower through HIR CFG blocks with hidden result locals rather than a new backend expression form; produced places are materialized into result locals so value-block merges do not alias arm-local captures. Built-in option nesting is rejected, including aliases that expand to an optional type. Contextual [codesnippet:T -> T?] wrapping is supported at explicit typed boundaries including declarations, calls, constructors, collections, and return slots. Fallible calls use postfix [codesnippet:!] for propagation or boundary [codesnippet:catch:] / [codesnippet, "catch |err|:"] blocks for recovery; value-producing catch recovery keeps [codesnippet:ThenValue] nodes in the handler body and lowers through shared value-block result locals. Inline catch is single-line expression/value-list sugar for the same model. Postfix error compatibility is exact [codesnippet:E -> E] or one-level optional wrapping [codesnippet:E -> E?] only. Postfix option propagation [codesnippet:?] unwraps [codesnippet:T?] on the present path and returns [codesnippet:none] from the current function when absent; it works in optional-returning and fallible optional-success functions, and rejects non-option operands, non-optional return targets, and [codesnippet:? catch]. [codesnippet:catch] is rejected in nested expression positions and loop headers, including function-call arguments, collection literal items, template interpolation, parenthesized arithmetic expressions, and field-access bases. Assignment targets are unavailable inside catch recovery subtrees for the same assignment. Empty [codesnippet:then], [codesnippet:then] across blocked constructs, non-receiving value blocks, fallback-bang expressions, postfix handler syntax, direct option fallback syntax, and optional [codesnippet:none] / literal single-predicate value [codesnippet:if] forms are rejected with structured diagnostics. Option-aware [codesnippet:is] / [codesnippet:is not] supports presence checks and value comparisons when the inner type supports equality; current option matches support [codesnippet:none =>] presence checks, literal and relational present-value patterns, and present capture [codesnippet, "|value| =>"] with exhaustiveness. Block-form option unwrap value blocks, arbitrary nested value blocks, value-producing loops, user tuple values, option inner choice patterns, and polished diagnostics remain deferred. [codesnippet:none is none] remains invalid. Public [codesnippet:Option of T] and [codesnippet:Result of T, E] remain deferred; use [codesnippet:T?] and final [codesnippet:E!] return slots instead.]
        ]

        [data:
            [: Constants and compile-time folding]
            [: Supported]
            [: Broad constant declaration, new top-level and body-local [codesnippet:#=] / [codesnippet:#Type] binding-mode syntax, const-only validation, explicit dependency ordering, same-file source-order rejection, cross-file imports, folding, overflow, const-template, const-record, and nested const-record field projection coverage.]
            [: Frontend / HTML builder]
            [: Runtime expressions and host calls in constants are rejected. Same-file constants must be declared before use; imported constants are resolved through header-stage dependency sorting. Module constants remain compile-time metadata rather than runtime top-level statements. New binding-mode syntax is supported for top-level constants, including inferred, explicit, collection, and optional annotations. Body-local [codesnippet:#] constants are supported in declaration scopes when their initializer fully folds; runtime locals and function parameters remain invalid in compile-time constant initializers. Const records are field-access-only: [codesnippet:palette.red] and fully projected nested fields such as [codesnippet:palette.primary.label] are supported, but const-record values cannot be assigned to runtime values, passed, returned, collected, or materialized as HIR runtime struct constructs. Private inferred const facts are tracked internally during AST finalization and carried into HIR as advisory metadata; config validation consumes these facts to allow references to earlier compile-time constants while rejecting runtime expressions. Borrow checking, temporary-local reduction, and backend lowering do not consume HIR const metadata yet.]
        ]

        [data:
            [: Type aliases]
            [: Supported]
            [: Broad alias declaration, collection/option/import target, visibility, cycle, duplicate, value-misuse, constructor-misuse, and import-alias interaction coverage.]
            [: Frontend-owned]
            [: Type aliases are transparent and annotation-only. Imported aliases are file-local; exporting an imported type under a public name requires declaring a real exported type alias. Aliases to external opaque types are rejected with structured diagnostics.]
        ]

        [data:
            [: Arithmetic, numeric casts, and explicit copy]
            [: Supported]
            [: Broad arithmetic, precedence, integer division, contextual numeric promotion, compound assignment, builtin cast, runtime fallback, explicit copy, and JS helper coverage.]
            [: JS / HTML]
            [: Regular [codesnippet:/] returns [codesnippet:Float]. [codesnippet://] requires [codesnippet:Int]. [codesnippet:Int(...)] and [codesnippet:Float(...)] are fallible casts that must be handled immediately with postfix [codesnippet:!] or boundary [codesnippet:catch]. [codesnippet:^] exists in frontend/JS lowering but has thin coverage; Wasm exponent support remains outside Alpha.]
        ]

        [data:
            [: Type checking and coercion]
            [: Supported]
            [: Broad type compatibility, contextual promotion, strict expression typing, call argument, and declaration coverage.]
            [: Frontend-owned]
            [: Keep strict expression typing separate from contextual declaration / return coercion.]
        ]

        [data:
            [: String coercion and IO/template boundaries]
            [: Supported]
            [: Broad valid string-content coercion plus IO/template rejection coverage for structs, collections, options, results, and multiple-return values.]
            [: Frontend / JS]
            [: Template heads and [codesnippet:io] accept only supported string-content values. Complex values must be handled, extracted, or converted before string-boundary use.]
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
            [: Path rendering is golden-sensitive. Keep normalized output and platform-stable diagnostics covered. Source and external package namespace imports are supported as shallow field-access-only records: value/function/constant members are accessed as [codesnippet:namespace.member], type members are valid in type position as [codesnippet:namespace.TypeName], and record values cannot be passed, returned, assigned, or traversed recursively. Specific imports use grouped syntax; direct symbol-path imports are rejected with [codesnippet:BST-IMPORT-0017], explicit [codesnippet:.bst] import extensions are rejected with [codesnippet:BST-IMPORT-0020], and explicit external file extensions without an active provider are rejected with [codesnippet:BST-IMPORT-0021]. Grouped source import aliases, nested grouped source import aliases, and grouped imports for virtual external packages are supported. Grouped virtual external package imports resolve through external package metadata before source/module facade enforcement, preventing root/module facades from misclassifying imports such as [codesnippet:import @web/canvas { get_canvas }] while preserving strict direct symbol-path rejection and source facade privacy. Same-module ordinary top-level declarations are importable by default without the old [codesnippet:#] visibility prefix. Directories containing non-config [codesnippet:#*.bst] files are module boundaries; [codesnippet:#mod.bst] is the only public facade. Cross-module and source-library imports go through [codesnippet:#mod.bst] facades, and modules without a facade cannot be imported from outside. Sibling [codesnippet:name.bst] / [codesnippet:name/] collisions are rejected during Stage 0 project discovery, while [codesnippet:.js] files are excluded from that specific collision rule. Authored declarations in [codesnippet:#mod.bst] are exported automatically, while normal imports inside [codesnippet:#mod.bst] stay local unless wrapped by authored facade declarations. Import aliases are file-local and do not create public facade names. Alias and visible-name collisions are hard errors across same-file declarations, source imports, external imports, namespace records, type aliases, prelude symbols, builtins, and facade public names. Direct imports of special files such as [codesnippet:#mod.bst], [codesnippet:#page.bst], and [codesnippet:#config.bst] are rejected with [codesnippet:BST-IMPORT-0008]. Prelude and external symbols cannot be redeclared as user functions or variables at top level or in body scope. Alias case mismatches warn. Grouped import aliases reject group-level trailing aliases, invalid local alias names, and duplicate alias syntax with structured diagnostics. Parent-directory imports with `..` are not supported, and imports must stay within allowed module/library/project roots.]
        ]

        [data:
            [: External platform packages]
            [: Partial]
            [: Broad package-scoped import and visibility coverage, with targeted external constant, fallible external signature, Float/F64/Char ABI, core libraries, prelude, duplicate-name, non-imported-symbol, alias diagnostics, JS runtime-import validation, and project-local JS provider frontend coverage.]
            [: Frontend / JS]
            [: Functions, opaque types, and compile-time external constants are supported through Rust-side registry metadata. [$markdown:
                - General user-authored external binding files are not supported. The HTML builder now supports the first provider-backed external files: annotated single-file [codesnippet:.js] imports with [codesnippet:@bst.opaque] and [codesnippet:@bst.sig] comments.
                - Project-local, source-library, and builder-owned JS-backed packages can create frontend-visible package-scoped opaque types, free functions, and receiver methods. Namespace imports, grouped imports, grouped aliases for functions/types/receiver methods, receiver-call lookup, fallible [codesnippet:Error!] signatures, parser rejection for malformed [codesnippet:this] receiver parameters, provider cache deduplication, parsed registered-runtime imports, block-bodied arrow export support, expression-bodied arrow export rejection, and emitted JS runtime-asset metadata are covered.
                - Direct [codesnippet:.js] imports obey the same module/source-library privacy boundaries as Beanstalk source imports: private JS implementation files cannot be imported across [codesnippet:#mod.bst] facade boundaries. Dev-server watch scopes include direct [codesnippet:.js] files under the entry root and configured library folders while ignoring generated output.
                - The HTML JS path now generates glue modules for provider-created ES module exports, imports emitted JS assets by relative module specifier, and maps stable external function IDs to generated wrapper names. Fallible JS wrappers convert [codesnippet:bstOk] / [codesnippet:bstErr] results into the backend's internal carrier shape, debug glue rejects invalid wrapper shapes, and release glue returns an error carrier instead of treating arbitrary JS values as success.
                - Registered core JS runtime modules such as [codesnippet:@beanstalk/runtime] are emitted once per build and resolved from generated pages through an import map when accepted JS runtime imports require them. Runtime imports are parsed from the JS source, may use multiline named import formatting, validated against registered exported names, and are not inferred from fallibility. Fallible JS functions may use [codesnippet:bstOk] / [codesnippet:bstErr] helpers or return the wrapper shape manually. Runtime JS assets, generated glue, registered runtime modules, import-map entries, and unsupported-backend checks are driven by external function calls reachable from entry [codesnippet:start], not by unused source-library/provider declarations. Unused [codesnippet:@web/canvas] / [codesnippet:@html] wrappers do not emit canvas assets or [codesnippet:@beanstalk/runtime]. The HTML builder provides the first built-in JS-backed package, [codesnippet:@web/canvas], with explicit imports, opaque [codesnippet:Canvas], [codesnippet:Canvas2d], [codesnippet:CanvasGradient], [codesnippet:CanvasPattern], [codesnippet:CanvasImage], [codesnippet:CanvasImageData], and [codesnippet:CanvasTextMetrics] types, fallible canvas/context/export/image-data/image-drawing helpers, and a broad common Canvas 2D receiver-method surface for styles, paths, transforms, text, gradients, patterns, image drawing, and pixel editing.
                - External constants are implemented for scalar compile-time values (Int, Float, Bool) and lower through expression parsing as ordinary literals. Non-scalar external constants are rejected in constant contexts.
                - External ABI metadata includes [codesnippet:F64], [codesnippet:I32], [codesnippet:Bool], [codesnippet:Char], and [codesnippet:Utf8Str] for the current core package skeletons and JS provider signatures.
                - External function metadata supports frontend-visible success slots plus an optional final concrete error slot. Fallible external calls must use postfix [codesnippet:!] or boundary [codesnippet:catch:] / [codesnippet, "catch |err|:"] handling, and they lower through the same explicit fallible HIR control-flow path as source functions.
                - External custom error slots must reference registered frontend-visible external types. Unknown external signature types are rejected instead of being smuggled through [codesnippet:T!].
                - Explicit imports are required for non-prelude external packages. [codesnippet:io] and [codesnippet:IO] are registered as external prelude symbols; builtin error symbols are compiler-owned bare names.
                - External function aliases and external constant aliases are supported. External opaque type aliases are rejected with structured diagnostics. Grouped external aliases and shallow external package namespace imports are supported.
                - Prelude collision rejection is enforced: explicit imports cannot shadow prelude names, and prelude symbols cannot be redeclared as user declarations.
                - External receiver methods are supported for explicitly imported JS surfaces and are keyed by exact package-scoped opaque receiver types when those types come from JS annotations. Keep this surface narrow; broader external-method APIs beyond [codesnippet:@web/canvas] remain future design work.
                - JS lowering now resolves external runtime names through per-function lowering metadata in the registry.
                - [codesnippet:InlineExpression] JS lowering is implemented for pure expression external functions. Arguments are substituted positionally using `#0`, `#1`, ... placeholders; duplicate placeholders are rejected. No runtime helper is emitted for inline-only functions.
                - Provider-created JS functions carry [codesnippet:ExternalModuleExport] metadata with the original ES module export name for the generated-glue phase. They are intentionally not treated as direct inline runtime helpers.
                - Wasm lowering for external packages is experimental and should not be counted as Alpha support. Reachable [codesnippet:@web/canvas] calls remain JS-backed and are rejected for HTML-Wasm with structured unsupported-backend diagnostics.
                - Unsupported external package function calls for the active backend are rejected with structured [codesnippet:BST-RULE-0058] diagnostics before backend lowering, not during LIR transformation. Validation is reachable-call-based so unused wrappers do not fail builds for backends that would reject the call if reached.
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
            [: Targeted success and diagnostic coverage for direct, grouped, and aliased imports; aliased constants such as [codesnippet:PI as pi]; aliased functions such as [codesnippet:sin as sine]; const-context constants; arity errors; type errors; missing symbols; and non-imported access.]
            [: JS / HTML]
            [: [codesnippet:@core/math] exposes [codesnippet:PI], [codesnippet:TAU], [codesnippet:E], and Float functions backed by JavaScript [codesnippet:Math]. [$markdown:
                - Implemented functions: [codesnippet:sin], [codesnippet:cos], [codesnippet:tan], [codesnippet:atan2], [codesnippet:log], [codesnippet:log2], [codesnippet:log10], [codesnippet:exp], [codesnippet:pow], [codesnippet:sqrt], [codesnippet:abs], [codesnippet:floor], [codesnippet:ceil], [codesnippet:round], [codesnippet:trunc], [codesnippet:min], [codesnippet:max], and [codesnippet:clamp].
                - All functions currently accept and return [codesnippet:Float].
                - The package is not prelude-imported. Users must explicitly import the required symbols from [codesnippet:@core/math].
                - Grouped imports, aliased imports, and namespace imports are supported. Direct symbol-path imports are rejected.
                - JS helpers are emitted only when the matching external function is referenced.
                - Wasm support for this package remains deferred/experimental. Imports used in HTML-Wasm builds fail with a structured [codesnippet:Rule] diagnostic before backend lowering.
            ]]
        ]

        [data:
            [: Core text package]
            [: Partial]
            [: Targeted runtime, JS helper, arity, type-error, edge-case, unsupported-builder, and Wasm-unsupported coverage.]
            [: JS / HTML]
            [: [codesnippet:@core/text] exposes [codesnippet:length], [codesnippet:is_empty], [codesnippet:contains], [codesnippet:starts_with], and [codesnippet:ends_with]. It is JS-backed, explicit-import only, and optional by builder. [codesnippet:length] returns backend-defined string length (JS UTF-16 code units for the JS backend). [codesnippet:is_empty] is true only for the exactly empty string. Empty needle/prefix/suffix behavior follows backend-defined JS semantics. Receiver-method forms remain deferred. Wasm support is deferred; HTML-Wasm builds fail with a structured [codesnippet:Rule] diagnostic before backend lowering.]
        ]

        [data:
            [: Core random package]
            [: Partial]
            [: Targeted import, JS helper, runtime smoke, arity, type-error, and Wasm-unsupported coverage.]
            [: JS / HTML]
            [: [codesnippet:@core/random] exposes [codesnippet:random_float()] and [codesnippet:random_int(min, max)]. [codesnippet:random_float()] returns a value in the half-open range 0.0 to 1.0. [codesnippet:random_int(min, max)] is inclusive at both ends. When [codesnippet:min > max] the bounds are swapped automatically; when [codesnippet:min == max] the result is that value. Seeded random remains deferred. Wasm support is deferred; HTML-Wasm builds fail with a structured [codesnippet:Rule] diagnostic before backend lowering.]
        ]

        [data:
            [: Core time package]
            [: Partial]
            [: Targeted import, grouped alias, namespace, receiver-method visibility, runtime smoke, fallible parse/catch, arity/type diagnostics, old-name rejection, opaque-field rejection, reachable Wasm-unsupported, and unused-wrapper reachability coverage.]
            [: JS / HTML]
            [: [codesnippet:@core/time] exposes opaque [codesnippet:Duration], [codesnippet:TimeMark], and [codesnippet:Timestamp] types plus the first JS-backed time helper slice. [$markdown:
                - Monotonic elapsed-time helpers: [codesnippet:mark_now()], [codesnippet:elapsed_since(start)], and [codesnippet:duration_between(start, end)].
                - Wall-clock timestamp helpers: [codesnippet:timestamp_now()], [codesnippet:timestamp_from_unix_seconds(seconds)], [codesnippet:timestamp_from_unix_milliseconds(milliseconds)], and fallible [codesnippet:timestamp_from_iso_string(text)].
                - Duration helpers: [codesnippet:duration_from_seconds(seconds)], [codesnippet:duration_from_milliseconds(milliseconds)], [codesnippet:duration.as_seconds()], [codesnippet:duration.as_milliseconds()], [codesnippet:duration.is_negative()], [codesnippet:duration.abs()], and [codesnippet:duration.clamp(min, max)].
                - Timestamp receiver helpers: [codesnippet:timestamp.unix_seconds()], [codesnippet:timestamp.unix_milliseconds()], and [codesnippet:timestamp.to_iso_string()].
                - Use [codesnippet:TimeMark], not [codesnippet:Timestamp], for frame deltas and profiling. [codesnippet:Timestamp] is UTC wall-clock time and can move with system clock changes.
                - Current JS representation is opaque to Beanstalk code but numeric internally: durations and marks are milliseconds, timestamps are Unix epoch milliseconds.
                - Full calendar date/time/time-zone APIs, local time-zone lookup, locale-aware formatting/parsing, timers/sleep/intervals, browser animation scheduling, and Wasm/native lowerings remain deferred. Reachable HTML-Wasm usage fails with a structured unsupported-backend diagnostic before backend lowering.
            ]]
        ]

        [data:
            [: Source library roots]
            [: Partial]
            [: Targeted coverage for AST-backed config loading, known-key enforcement, config value-shape enforcement, plain immutable and explicit compile-time config bindings, folded template settings, config-local type/struct/choice support declarations, core/builder source-library config imports, project-local and relative config import rejection, unsupported config functions/mutable bindings/standalone templates/runtime-only values, config local const references between declarations, config-defined source library root resolution, missing/non-directory explicit scan roots, prefix collision detection, empty entry roots, `.bst` file/folder collision rejection, and `#mod.bst` facade enforcement.]
            [: Frontend / HTML]
            [: Source libraries and regular modules share the same visibility model. Config loading now runs `#config.bst` and reachable core/builder source-library support files through tokenizer, header parsing, dependency sorting, and AST construction before extracting folded immutable known-key declarations authored in the config file. Config declarations must use known core or builder-declared keys, and each known key's registered value shape is enforced before core fields are applied or backend settings are stored. String settings accept string literals and folded templates, [codesnippet:project] is a closed string set currently accepting only [codesnippet:"html"], boolean settings require folded [codesnippet:Bool] values, [codesnippet:library_folders] accepts one string or a collection of strings, and [codesnippet:template_const_loop_iteration_limit] requires a positive folded [codesnippet:Int] no greater than [codesnippet:1_000_000]. Preferred source-library scan syntax is [codesnippet:library_folders = {"lib", "packages"}]; explicit compile-time syntax such as [codesnippet:library_folders #= {"lib"}] is also supported. Folded template initializers are accepted for string settings. Const-record field projection is supported for explicit [codesnippet:#=] config keys and deferred for plain [codesnippet:=] config keys pending shared const-context or resolver support. Authored config may also contain imports, type aliases, structs, and choices as compile-time support declarations. Config values may reference earlier compile-time constants (both explicit [codesnippet:#=] and plain immutable declarations) through shared AST const facts, including constants imported from core/builder source libraries; values that do not resolve to compile-time constants are rejected with structured diagnostics. Authored config imports may only target core/builder libraries, while relative and project-local config imports are rejected by design. Path-token folder entries such as [codesnippet:@lib] are no longer accepted in config values; use string folder names instead. Each direct child directory of a configured scan folder becomes an import prefix. The configured scan folder name itself is not an import prefix. `/lib` is the default scan folder when `library_folders` is omitted. Missing default scan folders are ignored; explicitly configured missing scan folders are config errors. [codesnippet:#root_folders] has been replaced by [codesnippet:library_folders] and is rejected with a migration diagnostic. Prefix collisions between builder-provided and project-local libraries produce hard errors. Sibling [codesnippet:name.bst] / [codesnippet:name/] collisions under the entry root or source-library roots are hard project-structure errors. All module roots enforce facade boundaries: any directory with a non-config [codesnippet:#*.bst] file is a boundary, modules that export outside themselves must do so through `#mod.bst`, and outside importers cannot bypass the facade. `#mod.bst` is facade-only: private top-level declarations, top-level runtime statements, and runtime templates are rejected.]
        ]

        [data:
            [: Builder-provided source libraries]
            [: Partial]
            [: Targeted coverage for builder-provided source library root registration and HTML builder library.]
            [: Frontend / HTML]
            [: Builders expose source libraries through `LibrarySet.source_libraries`. The HTML builder provides `@html` as a default source library with basic HTML tag helpers, formatting utilities, and authored canvas convenience helpers backed by [codesnippet:@web/canvas]. Imports inside [codesnippet:libraries/html/#mod.bst] remain local implementation details; raw [codesnippet:@web/canvas] symbols are not re-exported through [codesnippet:@html] unless [codesnippet:@html] authors real wrapper declarations. These libraries follow the same module visibility/facade rules as project modules. Prefix collisions between builder-provided and project-local libraries produce a hard config error.]
        ]

        [data:
            [: Project-local libraries]
            [: Partial]
            [: Targeted coverage for config-defined project library folders, import resolution, and facade gating.]
            [: Frontend / HTML]
            [: Project-local source libraries are discovered from [codesnippet:library_folders] (default convention: `/lib` when not configured). Imports resolve through the library prefix (for example, `/lib/helper` as `@helper` or `/packages/helper` as `@helper`). Public cross-module access goes through `#mod.bst`; bypassing implementation files from outside the module is rejected. Module boundary enforcement applies to all module roots, not only project-local libraries.]
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
            [: Broad condition validation, nested condition, Bool requirement, same-line [codesnippet, "else if"] rejection, and JS lowering coverage.]
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
            [: Fresh rvalues can satisfy mutable parameters without [codesnippet:~]. Existing places still require explicit [codesnippet:~].]
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
            [: Broad JS helper, fallible [codesnippet:get] / [codesnippet:set] / [codesnippet:remove], infallible [codesnippet:push] / [codesnippet:length], removed indexed-write, out-of-bounds, and explicit-mutable-receiver coverage.]
            [: JS / HTML]
            [: [codesnippet:get], [codesnippet:set], [codesnippet:push], [codesnippet:remove], and [codesnippet:length] are compiler-owned surfaces. Indexed assignment through [codesnippet:get] has been removed; use handled [codesnippet:~items.set(index, value)] calls instead.]
        ]

        [data:
            [: Error helper methods]
            [: Removed from public surface]
            [: Removal coverage rejects public helper calls such as [codesnippet:err.bubble()]. Hidden JS metadata helper coverage remains backend-internal.]
            [: Frontend / JS runtime internal]
            [: Do not re-expose [codesnippet:with_location], [codesnippet:push_trace], [codesnippet:bubble], or a public [codesnippet:@core/error] helper package before the Result/Error refactor explicitly designs it.]
        ]

        [data:
            [: Receiver methods]
            [: Supported]
            [: Broad method call, import, namespace import, grouped alias, receiver-method alias collision, grouped receiver-type visibility, same-file/imported same-name receiver dispatch, type auto-import, facade privacy, same-file restriction, unsupported receiver, collision, missing receiver-type, and mutable receiver diagnostics coverage.]
            [: JS / HTML]
            [: User-defined struct methods stay statically resolved and same-file with the struct for Alpha. Receiver methods import through the receiver catalog, not as free functions or namespace-record fields. Receiver-method names may be shared by receiver methods because dispatch includes the receiver type, but they still collide with ordinary value/type/namespace/prelude/builtin names. Explicit grouped receiver-method aliases reserve local names for collision checks, and grouped receiver-method imports require the receiver type to be visible through a type import, namespace import, same grouped import, or exact transparent alias.]
        ]

        [data:
            [: Fallible suffix handling]
            [: Supported]
            [: Broad propagation, shared [codesnippet:then] catch recovery, catch handler, handler fallthrough rejection, template-boundary, explicit HIR fallible-branch, direct alias-return forwarding, nested call-argument propagation, runtime-operator propagation, receiver-base propagation, match-guard propagation, and obsolete HIR propagation-expression removal coverage.]
            [: JS / HTML]
            [: Statement, direct-return, declaration, assignment, multi-bind, call-argument, runtime-operator, compound-child, condition, loop-input, template-chunk, receiver-base, and match-guard propagation now lower through explicit success/error HIR edges. Value-producing catch recovery lowers [codesnippet:then] values through shared value-block result locals; nested ordinary branch paths may produce for the same catch block. Success-producing catch handlers without [codesnippet:then] must terminate explicitly; literal [codesnippet:assert(false)] / [codesnippet, "assert(false, \"message\")"] handlers satisfy that rule, while dynamic assertions still fall through for static completeness. Fallthrough is rejected before HIR. Collection [codesnippet:get], [codesnippet:set], and [codesnippet:remove] use the same explicit fallible branch path; [codesnippet:push] and [codesnippet:length] lower as plain calls. The old expression-level HIR propagation helper has been removed. A temporary internal fallible carrier remains in type/HIR lowering internals for backend handoff and is not public [codesnippet:Result] syntax.]
        ]

        [data:
            [: Builtin Error type]
            [: Supported with simplified surface]
            [: Constructor/default/named-argument coverage plus public [codesnippet:message] and [codesnippet:code] field coverage. Old family names compile as user declarations.]
            [: Frontend / JS]
            [: Public fields are only [codesnippet:message] and [codesnippet:code]. [codesnippet:code] defaults to [codesnippet:0] through the general struct-default path. Hidden location/trace metadata remains backend-internal.]
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
            [: Local-only [codesnippet:just bench-report] command reports slowest cases, stage movement, counter movement, normalized ratios, and next investigation candidates from ignored JSONL history.]
            [: Must not write tracked summaries. Must not become a CI timing gate.]
        ]

        [data:
            [: Profiling command wrappers]
            [: Supported]
            [: Targeted]
            [: Optional local [codesnippet:just profile-build] helper builds a frame-pointer, line-table profiling binary after benchmark reports identify an active investigation area.]
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
            [: Keep behind [codesnippet:--html-wasm]. Tests may pin current behavior, but Alpha support is JS / HTML.]
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
            [: Traits / interfaces]
            [: Deferred]
            [: [codesnippet:trait_declaration_reserved], [codesnippet:trait_this_reserved]]
            [: Syntax is reserved for future design work.]
        ]

        [data:
            [: Async / concurrency scopes]
            [: Deferred]
            [: [codesnippet:async_block_reserved], [codesnippet:async_keyword_declaration_rejected]]
            [: Future async syntax is design-only. Current [codesnippet:async] statement and declaration positions are rejected with structured diagnostics.]
        ]

        [data:
            [: Checked scopes]
            [: Deferred]
            [: [codesnippet:checked_block_reserved], [codesnippet:checked_keyword_declaration_rejected]]
            [: [codesnippet:checked] is reserved and rejected cleanly until checked/effect semantics are designed.]
        ]

        [data:
            [: Labeled scopes]
            [: Rejected / Reserved]
            [: [codesnippet:labeled_scope_rejected]]
            [: Bare [codesnippet:label:] syntax is not supported. Use [codesnippet:block:] for ordinary scoped blocks.]
        ]

        [data:
            [: Legacy / foreign-language syntax]
            [: Rejected / Reserved]
            [: [codesnippet:typescript_style_declaration_rejected], [codesnippet:diagnostic_statement_keyword_fn], [codesnippet:diagnostic_statement_keyword_let], [codesnippet:diagnostic_statement_keyword_match], [codesnippet:diagnostic_statement_comment_slash_slash], [codesnippet:diagnostic_expression_double_equal], [codesnippet:diagnostic_expression_bang_equal], [codesnippet:diagnostic_expression_and_and], [codesnippet:diagnostic_expression_or_or], [codesnippet:diagnostic_expression_bang_negation]]
            [: These are diagnostic-quality fixtures only. Do not treat compatibility syntax as planned Alpha surface.]
        ]

        [data:
            [: No public panic keyword/directive]
            [: Not supported]
            [: Unknown names or ordinary invalid syntax]
            [: There is no [codesnippet:panic] keyword and no [codesnippet:#panic] directive in current Beanstalk. Use [codesnippet, "assert(false, \"message\")"] for an intentional explicit panic.]
        ]

        [data:
            [: Debug-only assertions]
            [: Deferred]
            [: No accepted syntax]
            [: There is no [codesnippet:debug_assert]. [codesnippet:assert] is always checked in development and release builds.]
        ]

        [data:
            [: Catchable / recoverable panics]
            [: Rejected / Deferred]
            [: [codesnippet:assert] result-handling diagnostics]
            [: There is no [codesnippet:recover], [codesnippet:catch_panic], public panic payload, or Beanstalk [codesnippet:catch] support for assertion failure. Expected failures use [codesnippet:Error!] and [codesnippet:catch].]
        ]

        [data:
            [: Additional explicit stop helpers]
            [: Deferred]
            [: No accepted syntax]
            [: There is no [codesnippet:todo], [codesnippet:unreachable], [codesnippet:fatal], [codesnippet:abort], or [codesnippet:precondition] builtin yet.]
        ]

        [data:
            [: Runtime assertion-message expressions]
            [: Deferred]
            [: [codesnippet:invalid_builtin_call]]
            [: Arbitrary runtime message expressions require lazy failure-only evaluation design. Current assertion messages must be string literals.]
        ]

        [data:
            [: Compile-time constant assertion messages]
            [: Deferred]
            [: [codesnippet:invalid_builtin_call]]
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
            [: [codesnippet:choice_basic_declaration_and_use], [codesnippet:choice_unit_constructor_call_rejected], [codesnippet:choice_imported_unit_constructor_call_rejected]]
            [: Unit variants are constructed as `Choice::Variant`. Empty constructor calls are invalid.]
        ]

        [data:
            [: Choice record payload variants]
            [: Supported]
            [: [codesnippet:choice_payload_record_declaration_success], [codesnippet:choice_payload_imported_field_type_success], [codesnippet:choice_payload_empty_record_rejected], [codesnippet:choice_payload_duplicate_field_rejected], [codesnippet:choice_payload_mutable_field_rejected], [codesnippet:choice_payload_field_default_rejected], [codesnippet:choice_payload_shorthand_rejected]]
            [: Only `Variant | field Type, ... |` syntax. Payload shorthand is rejected.]
        ]

        [data:
            [: Choice constructor use]
            [: Supported]
            [: [codesnippet:choice_payload_constructor_positional_success], [codesnippet:choice_payload_constructor_named_success], [codesnippet:choice_payload_constructor_mixed_success], [codesnippet:choice_payload_constructor_missing_field_rejected], [codesnippet:choice_payload_constructor_unknown_field_rejected], [codesnippet:choice_payload_constructor_duplicate_field_rejected], [codesnippet:choice_payload_constructor_positional_after_named_rejected], [codesnippet:choice_payload_constructor_type_mismatch_rejected], [codesnippet:choice_payload_constructor_without_args_rejected], [codesnippet:choice_unit_constructor_call_rejected], [codesnippet:choice_imported_unit_constructor_call_rejected], [codesnippet:choice_imported_payload_constructor_success]]
            [: Unit variants use `Choice::Variant` without parentheses. Payload variants use `Choice::Variant(...)` with positional and named arguments. Empty constructor calls on unit variants are rejected. Missing constructor args on payload variants are rejected.]
        ]

        [data:
            [: Choice const values]
            [: Supported]
            [: [codesnippet:choice_const_unit_success], [codesnippet:choice_const_payload_success]]
            [: Unit and payload choice variants can be used in top-level `#` constant declarations.]
        ]

        [data:
            [: Choice payload matching]
            [: Supported]
            [: [codesnippet:choice_payload_match_capture_success], [codesnippet:choice_payload_match_qualified_capture_success], [codesnippet:choice_payload_match_guard_uses_capture_success], [codesnippet:choice_payload_match_import_alias_success], [codesnippet:choice_payload_match_exhaustive_tag_level_success], [codesnippet:choice_payload_match_wrong_capture_name_rejected], [codesnippet:choice_payload_match_too_few_captures_rejected], [codesnippet:choice_payload_match_too_many_captures_rejected], [codesnippet:choice_payload_match_duplicate_capture_rejected], [codesnippet:choice_payload_match_capture_shadowing_rejected], [codesnippet:choice_payload_match_rename_success], [codesnippet:choice_payload_match_rename_guard_success], [codesnippet:choice_payload_match_rename_wrong_original_rejected], [codesnippet:choice_payload_match_rename_duplicate_binding_rejected], [codesnippet:choice_payload_match_rename_shadowing_rejected], [codesnippet:choice_payload_match_rename_missing_alias_rejected], [codesnippet:choice_payload_match_rename_non_symbol_alias_rejected], [codesnippet:choice_payload_match_rename_original_not_visible_rejected], [codesnippet:choice_payload_match_rename_self_alias_success], [codesnippet:choice_payload_match_named_assignment_rejected], [codesnippet:choice_payload_match_missing_payload_rejected], [codesnippet:choice_payload_match_unit_parens_rejected], [codesnippet:choice_payload_match_duplicate_variant_rejected], [codesnippet:choice_payload_match_guard_requires_else]]
            [: Constructor-like patterns support declared payload field names and `field as local_name` aliases. Field matching remains positional and validated against declared field names. Exhaustiveness is tag-level.]
        ]

        [data:
            [: Choice payload field access]
            [: Deferred]
            [: [codesnippet:choice_payload_direct_field_access_deferred]]
            [: No `value.field` until variant narrowing/refinement is designed. Use pattern matching to extract payload fields.]
        ]

        [data:
            [: Choice payload capture renames]
            [: Supported]
            [: [codesnippet:choice_payload_match_rename_success], [codesnippet:choice_payload_match_rename_guard_success], [codesnippet:choice_payload_match_rename_wrong_original_rejected], [codesnippet:choice_payload_match_rename_duplicate_binding_rejected], [codesnippet:choice_payload_match_rename_shadowing_rejected], [codesnippet:choice_payload_match_rename_missing_alias_rejected], [codesnippet:choice_payload_match_rename_non_symbol_alias_rejected], [codesnippet:choice_payload_match_rename_original_not_visible_rejected], [codesnippet:choice_payload_match_rename_self_alias_success]]
            [: `Variant(original_name as local_name) =>` binds the payload field to a different local name visible in the guard and arm body.]
        ]

        [data:
            [: `as` keyword outside supported domains]
            [: Rejected / Reserved]
            [: [codesnippet:as_in_expression_rejected], [codesnippet:as_in_variable_declaration_rejected], [codesnippet:as_in_function_signature_rejected], [codesnippet:as_in_non_choice_match_pattern_rejected], [codesnippet:as_in_match_pattern_lead_rejected]]
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
            [: [codesnippet:choice_nested_payload_pattern_deferred]]
            [: Exhaustiveness is tag-level for now.]
        ]

        [data:
            [: Direct facade re-export syntax]
            [: Deferred]
            [: Legacy [codesnippet:#import] rejection coverage plus wrapper-declaration facade coverage for source and external functions.]
            [: Direct facade re-export syntax has been removed from the Alpha surface. Legacy [codesnippet:#import] now fails with [codesnippet:BST-RULE-0025]. Until a new direct export syntax is designed, [codesnippet:#mod.bst] facades expose imported behavior through real authored declarations: compile-time bindings for constants, type aliases for types, and wrapper functions for callable values. Function alias exports and automatic method re-export through [codesnippet:#mod.bst] type aliases remain deferred. Normal imports inside [codesnippet:#mod.bst] remain local implementation details.]
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
            [: The HTML builder supports annotated single-file [codesnippet:.js] imports and the built-in [codesnippet:@web/canvas] package. Deferred surfaces include JS constants, classes, arbitrary dependency graphs, default exports, [codesnippet:export { name }] re-exports, CommonJS, object properties/getters/setters, callbacks/function types, async functions/promises, multi-success returns, external generic types, collections/options in JS signatures, cross-package external type unification, text/CSS/HTML imports as strings, source maps, tree-shaking, and minification.]
        ]

        [data:
            [: Namespace / wildcard imports]
            [: Partial]
            [: Source-file, module-facade, receiver-method, Rust-side external package, and provider-backed JS package namespace records are covered; wildcard imports remain deferred.]
            [: Namespace imports such as [codesnippet:import @./helper], [codesnippet:import @core/math], and [codesnippet:import @web/canvas] create shallow field-access-only records. Members can be used as [codesnippet:helper.render()], [codesnippet:math.sin(1.0)], or [codesnippet:canvas.get_canvas("game")], source and external type members can be used in type position as [codesnippet:models.User] or [codesnippet:canvas.Canvas2d], and receiver methods from the imported surface are available only through receiver-call syntax. Import records cannot be used as values, namespace records do not expose nested filesystem traversal, receiver methods are not namespace fields, and wildcard syntax such as [codesnippet:import @pkg.*] remains unsupported.]
        ]

        [data:
            [: Choice recursive types]
            [: Deferred]
            [: [codesnippet:choice_recursive_direct_declaration_deferred]]
            [: Requires layout/indirection design.]
        ]

        [data:
            [: Generic type infrastructure]
            [: Supported]
            [: Targeted frontend unit coverage for TypeId-native generic instance identity, generic binding consistency, TypeId binding/unification for generic parameters, collections, options, nominal generic instances, generic free-function templates, constructed-type substitution, type-identity compatibility keys, parameter scopes, generic parameter parsing, [codesnippet:of] type applications, arity checks, collection generic substrate, recursive HIR-boundary generic-parameter rejection, and diagnostics. Integration coverage includes duplicate, invalid, builtin-colliding, and visible-type-colliding generic parameter diagnostics; generic struct and choice constructor inference; concrete generic aliases; same-module imported generic types and aliases; named argument inference; conflicting inference diagnostics; generic function declaration parsing; generic body validation; local generic function call inference and instantiation; same-module grouped-import and namespace generic function calls; grouped alias calls; cross-file and facade expected-result inference; cross-file and facade optional returns; cross-file and facade fallible error slots; generic function value diagnostics; authored facade wrapper generic calls through grouped, namespace, and grouped-alias imports; facade privacy regressions; direct [codesnippet:#mod.bst] import rejection; facade import-alias non-reexport rejection; constructed collection returns; ambiguous/conflicting function inference diagnostics; recursive generic function instantiation rejection with call-site context; receiver rejection; nested [codesnippet:of] rejection; collection capacity parsing; and HTML-Wasm reachable/unreachable generic-runtime validation.]
            [: The current generic free-function import-hardening work is implemented through the AST generic free-function template and concrete instance model. Generic parameter metadata, [codesnippet:of] type applications, concrete generic aliases, collection generic substrate, constructor inference, choice matching, AST unresolved-generic validation, concrete HIR nominal identity, HIR validation rejection for unresolved generic parameters, AST-owned generic function templates, template-aware generic body validation, same-file calls, same-module grouped-import or namespace calls, authored facade-wrapper calls, constructed-type substitution coverage, imported generic optional/fallible returns, and generic diagnostic hardening are implemented. The generic substrate is split by owner across [codesnippet:generic_parameters], [codesnippet:generic_bindings], [codesnippet:generic_identity_bridge], and [codesnippet:ast/generic_functions]. Nested inline generic applications, recursive generic types, generic aliases with parameters, partial application, explicit generic call syntax, generic function values, generic receiver methods, trait bounds, direct facade re-export syntax, and generic external package functions remain deferred or rejected.]
        ]

        [data:
            [: Choice generic declarations]
            [: Supported]
            [: [codesnippet:generic_choice_declaration_parse_success], [codesnippet:generic_choice_instantiation_success], [codesnippet:generic_choice_match_success], [codesnippet:generic_named_choice_payload_infer], [codesnippet:generic_expected_unit_choice_success], [codesnippet:generic_recursive_type_deferred]]
            [: Generic choice headers and payload type parameters parse and resolve. Concrete constructor inference, named payload arguments, expected-type unit variants, and matching on concrete generic instances are supported. Recursive generic choices and generic receiver methods remain rejected.]
        ]

        [data:
            [: Generic structs]
            [: Supported]
            [: [codesnippet:generic_struct_instantiation_success], [codesnippet:generic_named_struct_args_infer], [codesnippet:generic_conflicting_inference_rejected], [codesnippet:generic_collection_element_success], [codesnippet:generic_imported_struct_choice_success], [codesnippet:generic_import_alias_concrete_alias_success], [codesnippet:generic_recursive_type_deferred]]
            [: Generic struct headers parse and resolve. Concrete constructor inference, field type substitution, named constructor arguments, imports and import aliases, collection element types, and concrete HIR lowering are supported. Recursive generic structs and generic receiver methods remain rejected. HTML-Wasm generic runtime values that are reachable from the exported entry roots fail with structured unsupported-backend diagnostics before Wasm lowering; unreachable generic-runtime code in unused functions is ignored by that backend-specific validation.]
        ]

        [data:
            [: Generic choices]
            [: Supported]
            [: [codesnippet:generic_choice_instantiation_success], [codesnippet:generic_named_choice_payload_infer], [codesnippet:generic_choice_match_success], [codesnippet:generic_choice_equality_success], [codesnippet:generic_choice_equality_unsupported_payload_rejected]]
            [: Concrete generic choice constructors, matching, and equality are implemented for resolved nominal instances when the substituted payload types support the operation. Wasm backend support for payload choices remains partial and is tracked by backend-specific expected failures.]
        ]

        [data:
            [: Generic functions]
            [: Partial]
            [: [codesnippet:generic_function_declaration_parse_success], [codesnippet:generic_function_body_validation_success], [codesnippet:generic_function_local_annotations_success], [codesnippet:generic_function_local_unknown_annotation_rejected], [codesnippet:generic_parameter_name_outside_scope_rejected], [codesnippet:generic_function_body_arithmetic_rejected], [codesnippet:generic_function_body_io_rejected], [codesnippet:generic_function_body_template_rejected], [codesnippet:generic_function_body_field_access_rejected], [codesnippet:generic_function_identity_success], [codesnippet:generic_function_wrapper_success], [codesnippet:generic_function_expected_context_success], [codesnippet:generic_fn_nested_expected_parameter_intermediate_success], [codesnippet:generic_constructed_types_success], [codesnippet:generic_function_optional_return_success], [codesnippet:generic_optional_none_inference_rejected], [codesnippet:generic_function_ambiguous_inference_rejected], [codesnippet:generic_fn_nested_expected_parameter_rejected], [codesnippet:generic_function_conflicting_inference_rejected], [codesnippet:generic_function_recursive_instantiation_rejected], [codesnippet:generic_fn_cross_file_grouped_success], [codesnippet:generic_fn_cross_file_grouped_alias_success], [codesnippet:generic_fn_cross_file_conflicting_inference_rejected], [codesnippet:generic_fn_cross_file_namespace_success], [codesnippet:generic_fn_cross_file_namespace_expected_result_success], [codesnippet:generic_fn_cross_file_expected_result_collection_success], [codesnippet:generic_fn_facade_expected_result_collection_success], [codesnippet:generic_fn_cross_file_optional_return_success], [codesnippet:generic_fn_facade_optional_return_success], [codesnippet:generic_fn_cross_file_fallible_error_slot_success], [codesnippet:generic_fn_facade_fallible_error_slot_success], [codesnippet:generic_fn_uninferable_parameter_rejected], [codesnippet:generic_fn_later_use_inference_rejected], [codesnippet:generic_fn_multiple_substitutions_same_module_success], [codesnippet:generic_fn_value_use_grouped_rejected], [codesnippet:generic_fn_value_use_namespace_rejected], [codesnippet:generic_fn_value_use_collection_rejected], [codesnippet:generic_fn_value_use_argument_rejected], [codesnippet:generic_fn_value_use_return_rejected], [codesnippet:generic_fn_value_use_facade_rejected], [codesnippet:generic_fn_namespace_synthetic_instance_inaccessible_rejected], [codesnippet:generic_fn_synthetic_grouped_import_rejected], [codesnippet:generic_fn_synthetic_direct_symbol_path_import_rejected], [codesnippet:generic_fn_synthetic_facade_export_rejected], [codesnippet:generic_fn_facade_grouped_success], [codesnippet:generic_fn_facade_namespace_success], [codesnippet:generic_fn_facade_grouped_alias_success], [codesnippet:generic_fn_facade_private_impl_import_rejected], [codesnippet:generic_fn_facade_direct_mod_import_rejected], [codesnippet:generic_fn_facade_import_alias_not_reexported_rejected], [codesnippet:generic_fn_instance_body_type_error_rejected], [codesnippet:generic_fallible_function_success], [codesnippet:generic_fallible_body_specialization_rejected], [codesnippet:generic_unused_parameter_rejected]]
            [: Generic function headers parse into AST-owned templates with canonical signature [codesnippet:TypeId]s. Template-aware body validation rejects behavior-dependent operations on unconstrained generic parameters while allowing pass-through, generic-to-generic forwarding, generic wrapper construction, local annotations using generic parameters, constructed collection returns, contextual optional returns, and final generic error slots. Same-file, same-module grouped-import, same-module namespace, and authored facade-wrapper calls infer type arguments from immediate arguments and immediate expected success-result context, then emit concrete AST/HIR-bound function instances. Synthetic generic instance paths stay internal and cannot be reached through namespace records, grouped imports, direct symbol-path imports, or facade import surfaces. Facade wrapper bodies resolve through the facade file's visibility, so private implementation generic helpers remain usable by the wrapper but hidden from outside importers. Generic optional returns and generic fallible error slots work across visible imports when immediate evidence solves the type parameters. Concrete instance body-emission diagnostics make the instantiating call site primary and keep the generic body failure span, declaration site, and concrete substitutions as diagnostic context. Conflicting inference diagnostics render both concrete [codesnippet:TypeId] names through the diagnostic type context, and cannot-infer diagnostics suggest ordinary receiving-site annotations. Generic fallible calls use the same postfix [codesnippet:!] and [codesnippet:catch] handling as ordinary fallible calls. [codesnippet:none] alone cannot infer a generic optional inner type. Inference does not flow from later use or from an outer expected parameter context into a nested generic call. Generic functions must still be called and cannot be used as first-class values.]
        ]

        [data:
            [: Concrete generic aliases]
            [: Supported]
            [: [codesnippet:generic_concrete_alias_success], [codesnippet:generic_import_alias_concrete_alias_success], [codesnippet:generic_nested_alias_workaround]]
            [: Aliases to fully concrete generic instances are supported and transparent in annotations and declaration/constructor compatibility checks, including aliases to imported generic declarations. Generic aliases with parameters are a separate deferred surface.]
        ]

        [data:
            [: Trait bounds on generics]
            [: Deferred]
            [: [codesnippet:generic_constraint_syntax_deferred], [codesnippet:generic_where_constraint_syntax_rejected]]
            [: Required for operators, equality, methods, and behavior-dependent operations on unconstrained generic parameters. Declaration-site [codesnippet:type A is Trait] constraints now produce a structured deferred diagnostic until traits/interfaces exist; [codesnippet:where] syntax is rejected because it is not part of the planned Alpha direction.]
        ]

        [data:
            [: Explicit generic call-site application]
            [: Rejected / Not Alpha]
            [: [codesnippet:generic_explicit_call_of_syntax_rejected], [codesnippet:generic_explicit_call_of_spacing_rejected], [codesnippet:generic_explicit_call_angle_syntax_rejected], [codesnippet:generic_explicit_call_square_syntax_rejected], [codesnippet:generic_explicit_call_argument_type_syntax_rejected], [codesnippet:generic_explicit_call_inference_annotation_success]]
            [: Explicit call-site type arguments such as [codesnippet, "identity of Int(42)"], [codesnippet, "identity<Int>(42)"], [codesnippet, "identity[Int](42)"], and [codesnippet, "identity(42 Int)"] are not Alpha syntax. Local generic function calls infer type arguments from immediate arguments and immediate expected result context only.]
        ]

        [data:
            [: Generic function values]
            [: Rejected / Deferred]
            [: [codesnippet:generic_fn_value_use_grouped_rejected], [codesnippet:generic_fn_value_use_namespace_rejected], [codesnippet:generic_fn_value_use_collection_rejected], [codesnippet:generic_fn_value_use_argument_rejected], [codesnippet:generic_fn_value_use_return_rejected], [codesnippet:generic_fn_value_use_facade_rejected]]
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
            [: Rejected / Deferred]
            [: [codesnippet:generic_receiver_rejected], [codesnippet:generic_receiver_method_declaration_rejected]]
            [: Declaration-site generic receiver methods and receiver methods on concrete generic instantiated types are not supported. Use free functions instead; broader method behavior waits for trait/constraint design.]
        ]

        [data:
            [: Recursive generic types]
            [: Deferred]
            [: [codesnippet:generic_recursive_type_deferred], [codesnippet:generic_mutual_recursive_rejected]]
            [: Recursive generic structs and choices are rejected through declaration dependency or generic recursion diagnostics. Requires layout/indirection design.]
        ]

        [data:
            [: Nested generic type application]
            [: Deferred / Rejected]
            [: [codesnippet:generic_nested_alias_workaround], [codesnippet:generic_nested_of_rejected]]
            [: More than one [codesnippet:of] application in a single annotation is rejected, except for collection element types. Name intermediate concrete aliases to compose complex types.]
        ]

        [data:
            [: Parameterized generic aliases]
            [: Deferred / Rejected]
            [: [codesnippet:generic_parameterized_alias_rejected]]
            [: Alias a fully concrete generic instance for now. Alias parameter substitution is not implemented, and parameterized alias declarations are rejected during header parsing.]
        ]

        [data:
            [: Partial type application]
            [: Rejected]
            [: [codesnippet:generic_partial_type_application_rejected]]
            [: Partially applied type constructors and placeholder-style type arguments are not supported. Alias a fully concrete generic instance instead.]
        ]

        [data:
            [: Generic external package functions]
            [: Deferred / Rejected]
            [: [codesnippet:js_import_generic_signature_rejected], [codesnippet:js_import_generic_opaque_type_rejected], [codesnippet:generic_external_opaque_type_application_rejected]]
            [: External package functions and external package types are concrete only for Alpha. JS provider signatures, opaque declarations, source type annotations, and backend package metadata must expose concrete surfaces.]
        ]

        [data:
            [: Type values / type-level CTFE]
            [: Not supported]
            [: [codesnippet:generic_type_name_value_rejected], [codesnippet:generic_type_application_value_rejected], [codesnippet:generic_namespace_type_value_rejected]]
            [: Types are not runtime or compile-time values. Type-returning functions, type-level [codesnippet:#if], type inspection, and generic call-site type expressions are outside the Alpha model.]
        ]

        [data:
            [: Choice equality]
            [: Supported]
            [: [codesnippet:choice_unit_equality_success], [codesnippet:choice_unit_equality_imported_variant_success], [codesnippet:choice_unit_equality_aliased_import_success], [codesnippet:choice_unit_equality_in_function_return], [codesnippet:choice_payload_equality_same_payload_true], [codesnippet:choice_payload_equality_different_payload_false], [codesnippet:choice_payload_equality_unit_vs_payload_false], [codesnippet:choice_payload_equality_same_choice_different_variants_false], [codesnippet:choice_payload_equality_nested_choice_true], [codesnippet:choice_payload_equality_nested_choice_false], [codesnippet:choice_equality_different_choice_types_rejected], [codesnippet:choice_diagnostic_choice_vs_non_choice_rejected], [codesnippet:choice_equality_payload_unsupported_struct_rejected], [codesnippet:choice_equality_payload_unsupported_collection_rejected], [codesnippet:choice_equality_payload_unsupported_external_rejected], [codesnippet:generic_choice_equality_success], [codesnippet:generic_choice_equality_unsupported_payload_rejected], [codesnippet:choice_payload_structural_equality_success], [codesnippet:choice_equality_positional_constructor_success], [codesnippet:choice_equality_named_constructor_success], [codesnippet:choice_equality_imported_constructor_success], [codesnippet:choice_equality_aliased_constructor_success], [codesnippet:choice_equality_constructor_wrong_arity_rejected], [codesnippet:choice_equality_constructor_wrong_field_rejected]]
            [: Structural equality is supported for unit, payload, and concrete generic choices in type checking and JS lowering. Payload fields are compared recursively when all substituted fields support equality. Unsupported payload field types, different choice types, and choice-vs-non-choice comparisons are rejected with specific diagnostics.]
        ]

        [data:
            [: Choice tagged/default declarations]
            [: Deferred]
            [: No canonical tagged-declaration case yet; [codesnippet:choice_default_decl_deferred] covers default declaration rejection.]
            [: These belong to the fuller tagged-union design, not the current Alpha subset.]
        ]

        [data:
            [: Choice default variant values]
            [: Deferred]
            [: [codesnippet:choice_default_decl_deferred], [codesnippet:choice_variant_default_value_deferred]]
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
            [: [codesnippet:diagnostic_match_negated_pattern]]
            [: Should wait until the match exhaustiveness model is deliberately extended.]
        ]

        [data:
            [: General capture / tagged match patterns]
            [: Supported]
            [: [codesnippet:pattern_capture_binds_scrutinee_success], [codesnippet:pattern_capture_before_else_makes_else_unreachable_warning], [codesnippet:pattern_capture_after_literal_warning_or_reachable], [codesnippet:pattern_unreachable_after_capture_warning], [codesnippet:pattern_capture_whole_choice_value_success], [codesnippet:pattern_choice_payload_existing_capture_alias_success], [codesnippet:option_match_patterns_success]]
            [: General capture patterns (for example `captured =>`) are supported for both choice and non-choice scrutinees. Choice payload capture patterns with `field as local_name` aliases are also supported. Current option matches support [codesnippet:none =>], literal and relational present-value checks, and option present capture with [codesnippet, "|value|"]. Nested payload patterns, option inner choice patterns, branch narrowing outside explicit captures, and general `|...|` tagged patterns remain deferred.]
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
    2. Update [codesnippet:tests/cases/manifest.toml].
    3. Add or rewrite canonical cases instead of layering temporary coverage.
    4. Mark unsupported syntax as [codesnippet:Deferred] or [codesnippet:Experimental], not as a vague missing test.
    5. Do not count HTML-Wasm as Alpha-supported unless the roadmap explicitly changes.
    6. When external packages change, update the package registry, backend lowering, import visibility tests, and this matrix together.

    The goal is to keep the Alpha surface small, honest, and fully exercised.
]
