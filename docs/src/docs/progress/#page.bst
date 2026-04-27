import @lib/html {format, style}
import @styles/docs {navbar, title, section, palette, table, header, data, codeblock, codesnippet, theme_head}

# page_title = "Implementation Matrix"
# page_description = "Current implementation and coverage status for the Beanstalk compiler."
# page_head = theme_head

#[navbar]
#[title, style.center: IMPLEMENTATION MATRIX]

#[section, $markdown:
    This page tracks the compiler surface intended for the first Beanstalk Alpha.

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

    [table:
        [header: [: Status] [: Meaning] ]

        [data:
            [: Supported]
            [: Implemented for the Alpha surface and expected to compile, type check, lower, and run through the JS / HTML pipeline.]
        ]

        [data:
            [: Partial]
            [: A useful subset is implemented, but some sub-surfaces are intentionally deferred or still being shaped before Alpha.]
        ]

        [data:
            [: Experimental]
            [: Present in the repo, but not part of the Alpha stability promise.]
        ]

        [data:
            [: Deferred]
            [: Not part of Alpha. Syntax should either be reserved or rejected with structured diagnostics.]
        ]
    ]

    # Coverage legend

    [table:
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

    [table:
        [header: [: Surface] [: Status] [: Coverage] [: Runtime target] [: Watch points] ]

        [data:
            [: Control flow]
            [: Supported]
            [: Broad parser, HIR, integration, dispatcher, and structured-lowering coverage.]
            [: JS / HTML]
            [: Keep loops, break / continue, match lowering, and branch-heavy CFG cases covered.]
        ]

        [data:
            [: Functions and calls]
            [: Supported]
            [: Broad function declaration, call validation, parameter passing, external-call, and return coverage.]
            [: JS / HTML]
            [: Keep named and positional call semantics strict. External and builtin calls remain positional-only.]
        ]

        [data:
            [: Templates and style directives]
            [: Supported]
            [: Broad template parsing, slot, directive, const-fragment, and runtime-fragment coverage.]
            [: HTML / JS]
            [: This is one of the main Alpha surfaces. Keep fragment ordering and slot composition heavily tested.]
        ]

        [data:
            [: Structs, records, and receiver methods]
            [: Supported]
            [: Broad integration coverage with targeted parser and receiver-catalog unit coverage.]
            [: JS / HTML]
            [: More field-mutation and receiver runtime regressions are useful, but this is not an Alpha blocker if current tests stay green.]
        ]

        [data:
            [: Choices]
            [: Partial]
            [: Targeted parser, declaration, match, import, assignment, return, and JS carrier-shape coverage.]
            [: JS / HTML]
            [: Unit variants are the current supported subset. Payloads, tagged/default declarations, and constructor use remain deferred unless deliberately promoted before Alpha.]
        ]

        [data:
            [: Pattern matching]
            [: Partial]
            [: Broad literal and choice-match coverage, including exhaustiveness and diagnostics.]
            [: JS / HTML]
            [: Literal patterns, choice variants, relational ordered-scalar patterns, guards, and else are supported. Wildcard in case arms is not supported—use else => instead. Negated and capture/tagged patterns remain deferred for Alpha.]
        ]

        [data:
            [: Collections]
            [: Supported]
            [: Broad collection literal, mutation, builtin method, indexing, loop, result, and JS helper coverage.]
            [: JS / HTML]
            [: Collection helper runtime contracts should remain JS-owned for Alpha. HTML-Wasm collection behavior is experimental only.]
        ]

        [data:
            [: Results, options, multiple returns, and multi-bind]
            [: Supported]
            [: Broad result, option, fallback, propagation, handler, cast, and multi-bind coverage.]
            [: JS / HTML]
            [: Multi-bind is limited to explicit multi-return function calls. It is not general destructuring.]
        ]

        [data:
            [: Type checking and coercion]
            [: Supported]
            [: Broad type compatibility, contextual promotion, strict expression typing, call argument, and declaration coverage.]
            [: Frontend-owned]
            [: Keep strict expression typing separate from contextual declaration / return coercion.]
        ]

        [data:
            [: Paths and imports]
            [: Supported]
            [: Broad import syntax, relative path, root folder, circular dependency, and multi-file module coverage.]
            [: Frontend / HTML builder]
            [: Path rendering is golden-sensitive. Keep normalized output and platform-stable diagnostics covered.]
        ]

        [data:
            [: External platform packages]
            [: Partial]
            [: Broad package-scoped import and visibility coverage, with targeted external constant, Float/F64 ABI, std/math, prelude, duplicate-name, and non-imported-symbol diagnostics.]
            [: Frontend / JS]
            [: Functions, opaque types, and compile-time external constants are supported through Rust-side registry metadata. [$markdown:
                - User-authored external binding files are not supported. Alpha external packages are Rust-side builder/compiler metadata.
                - External constants are implemented for scalar compile-time values and lower through expression parsing as ordinary literals.
                - External ABI metadata includes [codesnippet:F64], which enables a proper Float-based [codesnippet:@std/math] package.
                - Explicit imports are required for non-prelude external packages. [codesnippet:io] and [codesnippet:IO] remain the only current prelude symbols.
                - External receiver methods are not a recommended expansion surface yet. Prefer free functions until external methods are keyed by nominal receiver type rather than broad ABI metadata.
                - JS lowering now resolves external runtime names through per-function lowering metadata in the registry.
                - [codesnippet:InlineExpression] JS lowering metadata exists but is not implemented yet.
                - Wasm lowering for external packages is experimental and should not be counted as Alpha support.
            ]]
        ]

        [data:
            [: Standard math package]
            [: Supported]
            [: Targeted success and diagnostic coverage for direct, grouped, and aliased imports; constants; const-context constants; arity errors; type errors; missing symbols; and non-imported access.]
            [: JS / HTML]
            [: [codesnippet:@std/math] exposes [codesnippet:PI], [codesnippet:TAU], [codesnippet:E], and Float functions backed by JavaScript [codesnippet:Math]. [$markdown:
                - Implemented functions: [codesnippet:sin], [codesnippet:cos], [codesnippet:tan], [codesnippet:atan2], [codesnippet:log], [codesnippet:log2], [codesnippet:log10], [codesnippet:exp], [codesnippet:pow], [codesnippet:sqrt], [codesnippet:abs], [codesnippet:floor], [codesnippet:ceil], [codesnippet:round], [codesnippet:trunc], [codesnippet:min], [codesnippet:max], and [codesnippet:clamp].
                - All functions currently accept and return [codesnippet:Float].
                - The package is not prelude-imported. Users must explicitly import the required symbols from [codesnippet:@std/math].
                - JS helpers are emitted only when the matching external function is referenced.
                - Wasm support for this package remains deferred/experimental.
            ]]
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
            [: Broad condition validation, nested condition, Bool requirement, and JS lowering coverage.]
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

    [table:
        [header: [: Surface] [: Status] [: Coverage] [: Runtime target] [: Watch points] ]

        [data:
            [: Collection methods]
            [: Supported]
            [: Broad JS helper, mutation, indexed write, out-of-bounds, and explicit-mutable-receiver coverage.]
            [: JS / HTML]
            [: [codesnippet:get], [codesnippet:set], [codesnippet:push], [codesnippet:remove], and [codesnippet:length] are compiler-owned surfaces.]
        ]

        [data:
            [: Error helper methods]
            [: Supported]
            [: Targeted coverage for [codesnippet:with_location], [codesnippet:push_trace], and [codesnippet:bubble].]
            [: JS / HTML]
            [: Keep emitted helper contracts pinned through JS backend tests.]
        ]

        [data:
            [: Receiver methods]
            [: Supported]
            [: Broad method call, export, same-file restriction, unsupported receiver, and mutable receiver diagnostics coverage.]
            [: JS / HTML]
            [: User-defined struct methods must stay statically resolved and same-file with the struct for Alpha.]
        ]

        [data:
            [: Result suffix handling]
            [: Supported]
            [: Broad propagation, fallback, named handler, handler fallthrough, and template-boundary coverage.]
            [: JS / HTML]
            [: Keep [codesnippet:__bs_result_propagate] and [codesnippet:__bs_result_fallback] contracts covered by artifact assertions.]
        ]

        [data:
            [: Builtin Error type]
            [: Supported for current runtime surface]
            [: Reserved symbol diagnostics and helper runtime coverage exist.]
            [: Frontend / JS]
            [: Further Error enrichment is deferred. Do not expand this before Alpha unless required by supported result handling.]
        ]
    ]
]

#[section, $markdown:
    # Experimental surfaces

    These are useful and may have tests, but they are not part of the Alpha stability contract.

    [table:
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
    If syntax exists, it should fail with a structured diagnostic rather than panic or lower incorrectly.

    [table:
        [header: [: Surface] [: Status] [: Canonical diagnostic direction] [: Notes] ]

        [data:
            [: Traits / interfaces]
            [: Deferred]
            [: [codesnippet:trait_declaration_reserved], [codesnippet:trait_this_reserved]]
            [: Syntax is reserved for future design work.]
        ]

        [data:
            [: Choice payload declarations]
            [: Deferred unless promoted before Alpha]
            [: [codesnippet:choice_payload_decl_deferred]]
            [: Keep rejected until the carrier representation, constructors, matching, and JS lowering are deliberately implemented together.]
        ]

        [data:
            [: Choice tagged/default declarations]
            [: Deferred]
            [: [codesnippet:choice_tagged_decl_deferred], [codesnippet:choice_default_decl_deferred]]
            [: These belong to the fuller tagged-union design, not the current Alpha subset.]
        ]

        [data:
            [: Choice constructor use]
            [: Deferred unless choices are expanded before Alpha]
            [: [codesnippet:choice_constructor_use_deferred]]
            [: Current unit variant flow is enough for the existing Alpha subset.]
        ]

        [data:
            [: Negated match patterns]
            [: Deferred]
            [: [codesnippet:diagnostic_match_negated_pattern]]
            [: Should wait until the match exhaustiveness model is deliberately extended.]
        ]

        [data:
            [: Capture / tagged match patterns]
            [: Deferred]
            [: [codesnippet:diagnostic_match_deferred_capture_pattern]]
            [: Adds bindings and scope rules inside match arms.]
        ]

        [data:
            [: Full tagged unions]
            [: Deferred]
            [: Choice deferred diagnostics should cover the unsupported entry points.]
            [: Explicitly post-Alpha unless required by a narrowed choice expansion.]
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
    # Final sweep notes

    These are the scoped follow-ups after the final pre-alpha sweep.
    They belong to Alpha cleanup or post-Alpha planning unless they become release blockers.

    [table:
        [header: [: Area] [: Priority] [: Action] ]

        [data:
            [: CI platform coverage]
            [: Alpha cleanup]
            [: Add or run a Windows/macOS validation path before tagging Alpha. The repo currently has a docs deploy workflow, not a cross-platform validation workflow.]
        ]

        [data:
            [: Pattern matching scope]
            [: Post-Alpha unless promoted]
            [: Relational ordered-scalar patterns are now implemented and tested. Keep negated and capture/tagged patterns deferred for Alpha unless explicitly promoted.]
        ]

        [data:
            [: Choice scope]
            [: Post-Alpha unless promoted]
            [: Unit-only choices are the current Alpha contract. Constructors, payloads, tagged/default declarations, and full tagged unions stay deferred.]
        ]

        [data:
            [: External package cleanup]
            [: Alpha cleanup]
            [: Keep adding small standard-package fixtures as the registry grows. Prioritise collision checks, explicit import visibility, constant folding boundaries, and backend lowering contracts over broad API expansion.]
        ]

        [data:
            [: Docs vs implementation]
            [: Ongoing]
            [: Keep aspirational design docs clearly labelled as future-facing. The progress page remains the implementation tracker.]
        ]

        [data:
            [: Char diagnostics]
            [: Cleanup]
            [: Add more malformed-char diagnostics if time allows. Existing success coverage and baseline failure coverage are enough for Alpha if tests remain green.]
        ]

        [data:
            [: Cross-platform golden stability]
            [: Ongoing]
            [: Keep path and template output normalized where exact bytes are not contractual.]
        ]

        [data:
            [: HTML-Wasm coverage]
            [: Post-Alpha]
            [: Keep experimental tests useful, but do not promote successful Wasm collection/runtime behavior to an Alpha requirement.]
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
