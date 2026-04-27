import @lib/html {format, style}
import @styles/docs {navbar, title, section, palette, table, header, data, codeblock, codesnippet, theme_head}

# page_title = "Implementation Matrix"
# page_description = "Current implementation and coverage status for the Beanstalk compiler."
# page_head = theme_head

#[navbar]
#[title, style.center: IMPLEMENTATION MATRIX]

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
            [: Supported]
            [: Broad parser, declaration, constructor, match, import, assignment, return, JS carrier-shape, and payload coverage.]
            [: JS / HTML]
            [: Unit variants use `Choice::Variant`. Empty constructor calls are invalid. Record-body payload declarations are parsed and type-checked. Payload constructors support positional and named arguments. Payload match extraction uses constructor-like patterns with original field names only. Const choice values are supported for unit and payload variants. Payload shorthand is rejected. Payload structural equality is deferred. Generic choices, recursive choices, and direct payload field access remain deferred. Choices, Options, and Results share a unified HIR variant construction representation while retaining distinct semantic type kinds.]
        ]

        [data:
            [: Pattern matching]
            [: Partial]
            [: Broad literal, choice-match, and payload-capture coverage, including exhaustiveness and diagnostics.]
            [: JS / HTML]
            [: Literal patterns, choice variant patterns (including payload captures), relational ordered-scalar patterns, guards, and else are supported. Wildcard in case arms is not supported—use else => instead. General capture/tagged patterns, rename captures, and nested payload patterns remain deferred.]
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
            [: Choice unit variants]
            [: Supported]
            [: [codesnippet:choice_basic_declaration_and_use], [codesnippet:choice_unit_constructor_call_rejected], [codesnippet:choice_imported_unit_constructor_call_rejected]]
            [: Unit variants are constructed as `Choice::Variant`. Empty constructor calls are invalid.]
        ]

        [data:
            [: Choice record payload variants]
            [: Supported]
            [: [codesnippet:choice_payload_record_declaration_success], [codesnippet:choice_payload_imported_field_type_success], [codesnippet:choice_payload_empty_record_rejected], [codesnippet:choice_payload_duplicate_field_rejected], [codesnippet:choice_payload_shorthand_rejected]]
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
            [: [codesnippet:choice_payload_match_capture_success], [codesnippet:choice_payload_match_qualified_capture_success], [codesnippet:choice_payload_match_guard_uses_capture_success], [codesnippet:choice_payload_match_exhaustive_tag_level_success], [codesnippet:choice_payload_match_wrong_capture_name_rejected], [codesnippet:choice_payload_match_too_few_captures_rejected], [codesnippet:choice_payload_match_too_many_captures_rejected], [codesnippet:choice_payload_match_duplicate_capture_rejected], [codesnippet:choice_payload_match_capture_shadowing_rejected], [codesnippet:choice_payload_match_rename_syntax_deferred], [codesnippet:choice_payload_match_named_assignment_rejected], [codesnippet:choice_payload_match_missing_payload_rejected], [codesnippet:choice_payload_match_unit_parens_rejected], [codesnippet:choice_payload_match_duplicate_variant_rejected], [codesnippet:choice_payload_match_guard_requires_else]]
            [: Constructor-like patterns with original field names only. Capture names must match declared payload field names. Exhaustiveness is tag-level.]
        ]

        [data:
            [: Choice payload field access]
            [: Deferred]
            [: No canonical case yet.]
            [: No `value.field` until variant narrowing/refinement is designed. Use pattern matching to extract payload fields.]
        ]

        [data:
            [: Choice binding renames]
            [: Deferred]
            [: [codesnippet:choice_payload_match_rename_syntax_deferred]]
            [: Future syntax: `<new_name> as <original_name>`.]
        ]

        [data:
            [: Choice nested payload patterns / exhaustiveness]
            [: Deferred]
            [: No canonical case yet.]
            [: Exhaustiveness is tag-level for now.]
        ]

        [data:
            [: Choice recursive types]
            [: Deferred]
            [: No canonical case yet.]
            [: Requires layout/indirection design.]
        ]

        [data:
            [: Choice generic declarations]
            [: Deferred]
            [: No canonical case yet.]
            [: Syntax ideas live in [codesnippet:docs/src/docs/generics.md]; do not implement here.]
        ]

        [data:
            [: Choice payload structural equality]
            [: Rejected / Reserved]
            [: [codesnippet:choice_payload_structural_equality_deferred]]
            [: Equality comparison for payload choice variants is not supported. Use pattern matching and compare fields inside the matching arm.]
        ]

        [data:
            [: Choice tagged/default declarations]
            [: Deferred]
            [: [codesnippet:choice_tagged_decl_deferred], [codesnippet:choice_default_decl_deferred]]
            [: These belong to the fuller tagged-union design, not the current Alpha subset.]
        ]

        [data:
            [: Choice default variant values]
            [: Deferred]
            [: [codesnippet:choice_default_decl_deferred]]
            [: If field defaults are inherited, document separately from variant defaults.]
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
            [: Deferred]
            [: [codesnippet:diagnostic_match_deferred_capture_pattern]]
            [: Choice payload capture patterns are now supported. General capture/tagged patterns and rename captures remain deferred.]
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
