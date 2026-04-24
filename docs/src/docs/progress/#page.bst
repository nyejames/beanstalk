import @lib/html {format, style}
import @styles/docs {navbar, title, section, palette, table, data, codeblock, codesnippet, theme_head}

# page_title = "Alpha Surface Matrix"
# page_description = "Current implementation and coverage status for the Beanstalk Alpha compiler surface."
# page_head = theme_head

#[navbar]
#[title, style.center: ALPHA SURFACE MATRIX]

#[section, $markdown:
    # Alpha Surface Matrix

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

    HTML-Wasm and the Rust interpreter are not Alpha stability targets.
    They may have tests and scaffolding, but they should not be counted as supported Alpha runtime surfaces.
]

#[section, $markdown:
    # Status legend

    [table:
        [: [: Status] [: Meaning] ]

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
        [: [: Coverage] [: Meaning] ]

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
        [: [: Surface] [: Status] [: Coverage] [: Runtime target] [: Watch points] ]

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
            [: Broad function declaration, call validation, parameter passing, host-call, and return coverage.]
            [: JS / HTML]
            [: Keep named and positional call semantics strict. Host and builtin calls remain positional-only.]
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
            [: Literal patterns, choice variants, wildcard, relational ordered-scalar patterns, guards, and else are supported. Negated and capture/tagged patterns remain deferred for Alpha.]
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
    # Compiler-owned builtin surfaces

    [table:
        [: [: Surface] [: Status] [: Coverage] [: Runtime target] [: Watch points] ]

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
        [: [: Surface] [: Status] [: Current role] [: Alpha rule] ]

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

        [data:
            [: Rust interpreter]
            [: Experimental scaffolding]
            [: Future local runtime / tooling direction.]
            [: Ignore for Alpha readiness unless it breaks normal builds or tests.]
        ]
    ]
]

#[section, $markdown:
    # Deferred or reserved surfaces

    These are intentionally not counted as supported Alpha features.
    If syntax exists, it should fail with a structured diagnostic rather than panic or lower incorrectly.

    [table:
        [: [: Surface] [: Status] [: Canonical diagnostic direction] [: Notes] ]

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
            [: Relational match patterns]
            [: Deferred]
            [: [codesnippet:diagnostic_match_deferred_relational_pattern]]
            [: Adds extra grammar, type-checking, and exhaustiveness interactions.]
        ]

        [data:
            [: Negated match patterns]
            [: Deferred]
            [: Add or keep a canonical deferred-pattern diagnostic if syntax is recognized.]
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
    # Current Alpha probe gaps

    These are the remaining areas to keep visible during the pre-alpha inspection.
    They are not all blockers, but they should be deliberately accepted, fixed, or moved out of the Alpha contract.

    [table:
        [: [: Area] [: Severity] [: Action] ]

        [data:
            [: Matrix accuracy]
            [: High]
            [: Keep this page in sync with actual compiler behavior. Remove stale HTML-Wasm-as-supported wording and stale cross-platform drift notes once fixes are confirmed.]
        ]

        [data:
            [: Pattern matching scope]
            [: High]
            [: Wildcard and relational ordered-scalar patterns are now implemented and tested. Keep negated and capture/tagged patterns deferred for Alpha unless explicitly promoted.]
        ]

        [data:
            [: Choice scope]
            [: High]
            [: Decide whether unit-only choices are the Alpha contract or whether constructors / payloads need a contained pre-Alpha expansion.]
        ]

        [data:
            [: Unsupported feature diagnostics]
            [: High]
            [: Re-check all reserved/deferred syntax paths for structured diagnostics and no panics.]
        ]

        [data:
            [: Docs vs implementation]
            [: Medium]
            [: Keep aspirational language docs if useful, but make this page the current implementation tracker.]
        ]

        [data:
            [: Char diagnostics]
            [: Low]
            [: Add more malformed-char diagnostics if time allows. Existing success coverage and baseline failure coverage are enough for Alpha if tests remain green.]
        ]

        [data:
            [: Cross-platform golden stability]
            [: Medium]
            [: Treat as mostly closed once Windows fixes are verified. Keep path and template output normalized where exact bytes are not contractual.]
        ]

        [data:
            [: HTML-Wasm coverage]
            [: Low for Alpha]
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
    5. Do not count HTML-Wasm or the Rust interpreter as Alpha-supported unless the roadmap explicitly changes.

    The goal is to keep the Alpha surface small, honest, and fully exercised.
]