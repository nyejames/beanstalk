import @html {center}
import @styles/docs {navbar, title, section, palette, styled_table, header, data, codeblock, codesnippet, theme_head}

# page_title = "Implementation Matrix"
# page_description = "Current implementation and coverage status for the Beanstalk compiler."
# page_head = theme_head

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
            [: Field mutation, nested field mutation, nominal mismatch, constructor named arguments, receiver borrow behavior, scalar receiver methods, and cross-file exported receiver use are covered. Keep Wasm receiver behavior experimental.]
        ]

        [data:
            [: Choices]
            [: Supported]
            [: Broad parser, declaration, constructor, match, import, assignment, return, JS carrier-shape, and payload coverage.]
            [: JS / HTML]
            [: Unit variants use `Choice::Variant`. Empty constructor calls are invalid. Record-body payload declarations are parsed and type-checked. Payload constructors support positional and named arguments. Payload match extraction uses constructor-like patterns with original field names or `field as local_name` aliases. Const choice values are supported for unit and payload variants. Payload shorthand is rejected. Payload structural equality is deferred. Generic choices, recursive choices, and direct payload field access remain deferred. Choices, Options, and Results share a unified HIR variant construction representation while retaining distinct semantic type kinds.]
        ]

        [data:
            [: Pattern matching]
            [: Partial]
            [: Broad literal, choice-match, and payload-capture coverage, including exhaustiveness and diagnostics.]
            [: JS / HTML]
            [: Literal patterns, choice variant patterns (including payload captures with `field as local_name` aliases), relational ordered-scalar patterns, guards, and else are supported. Wildcard in case arms is not supported—use else => instead. General capture/tagged patterns and nested payload patterns remain deferred. `as` in non-choice match patterns is rejected with a structured diagnostic.]
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
            [: Constants and compile-time folding]
            [: Supported]
            [: Broad constant declaration, const-only validation, cross-file soft dependency, folding, overflow, const-template, and const-record coverage.]
            [: Frontend / HTML builder]
            [: Runtime expressions and host calls in constants are rejected. Module constants remain compile-time metadata rather than runtime top-level statements.]
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
            [: Regular [codesnippet:/] returns [codesnippet:Float]. [codesnippet://] requires [codesnippet:Int]. [codesnippet:Int(...)] and [codesnippet:Float(...)] return [codesnippet:Result] values and must be handled. [codesnippet:^] exists in frontend/JS lowering but has thin coverage; Wasm exponent support remains outside Alpha.]
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
            [: Broad import syntax, relative child path, entry root fallback, source library prefix, circular dependency, multi-file module, single-symbol aliases, grouped aliases, nested grouped aliases, collision, and file-local coverage.]
            [: Frontend / HTML builder]
            [: Path rendering is golden-sensitive. Keep normalized output and platform-stable diagnostics covered. Single-symbol source import aliases, grouped source import aliases, and nested grouped source import aliases are supported. Aliases are file-local and not re-exported. Alias and visible-name collisions are hard errors across same-file declarations, source imports, external imports, type aliases, prelude symbols, builtins, and re-export public names. Prelude and external symbols cannot be redeclared as user functions or variables at top level or in body scope. Alias case mismatches warn. Grouped import aliases reject group-level trailing aliases, invalid local alias names, and duplicate alias syntax with structured diagnostics. Parent-directory imports with `..` are not supported, and imports must stay within allowed module/library/project roots.]
        ]

        [data:
            [: External platform packages]
            [: Partial]
            [: Broad package-scoped import and visibility coverage, with targeted external constant, Float/F64 ABI, core libraries, prelude, duplicate-name, non-imported-symbol, and alias diagnostics.]
            [: Frontend / JS]
            [: Functions, opaque types, and compile-time external constants are supported through Rust-side registry metadata. [$markdown:
                - User-authored external binding files are not supported. Alpha external packages are Rust-side builder/compiler metadata.
                - External constants are implemented for scalar compile-time values (Int, Float, Bool) and lower through expression parsing as ordinary literals. Non-scalar external constants are rejected in constant contexts.
                - External ABI metadata includes [codesnippet:F64], [codesnippet:I32], [codesnippet:Bool], and [codesnippet:Utf8Str] for the current core package skeletons.
                - Explicit imports are required for non-prelude external packages. [codesnippet:io] and [codesnippet:IO] are registered as external prelude symbols; builtin error symbols are compiler-owned bare names.
                - External function aliases and external constant aliases are supported. External opaque type aliases are rejected with structured diagnostics. Grouped external aliases are supported.
                - Prelude collision rejection is enforced: explicit imports cannot shadow prelude names, and prelude symbols cannot be redeclared as user declarations.
                - External receiver methods are not a recommended expansion surface yet. Prefer free functions until external methods are keyed by nominal receiver type rather than broad ABI metadata.
                - JS lowering now resolves external runtime names through per-function lowering metadata in the registry.
                - [codesnippet:InlineExpression] JS lowering is implemented for pure expression external functions. Arguments are substituted positionally using `#0`, `#1`, ... placeholders; duplicate placeholders are rejected. No runtime helper is emitted for inline-only functions.
                - Wasm lowering for external packages is experimental and should not be counted as Alpha support.
                - Unsupported external package imports for the active backend are rejected with structured [codesnippet:Rule] diagnostics before backend lowering, not during LIR transformation.
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
                - Direct imports, grouped imports, and aliased imports are all supported.
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
            [: Targeted import, JS helper, runtime smoke, arity-error, and Wasm-unsupported coverage.]
            [: JS / HTML]
            [: [codesnippet:@core/time] exposes [codesnippet:now_millis()] and [codesnippet:now_seconds()]. Full date/time/timezone objects, formatting, durations, and monotonic clocks are deferred. Wasm support is deferred; HTML-Wasm builds fail with a structured [codesnippet:Rule] diagnostic before backend lowering.]
        ]

        [data:
            [: Source library roots]
            [: Partial]
            [: Targeted coverage for config-defined source library root resolution, prefix collision detection, and `#mod.bst` facade enforcement.]
            [: Frontend / HTML]
            [: Source libraries and regular modules share the same visibility model. [codesnippet:#library_folders] defines project-local source-library scan folders; each direct child directory of a configured scan folder becomes an import prefix. The configured scan folder name itself is not an import prefix. `/lib` is the default scan folder when [codesnippet:#library_folders] is omitted. Missing default scan folders are ignored; explicitly configured missing scan folders are config errors. [codesnippet:#root_folders] has been replaced by [codesnippet:#library_folders] and is rejected with a migration diagnostic. Prefix collisions between builder-provided and project-local libraries produce hard errors. All module roots enforce facade boundaries: modules that export outside themselves must do so through `#mod.bst`; outside importers cannot bypass the facade. `#mod.bst` is facade-only: private top-level declarations, top-level runtime statements, and runtime templates are rejected.]
        ]

        [data:
            [: Builder-provided source libraries]
            [: Partial]
            [: Targeted coverage for builder-provided source library root registration and HTML builder library.]
            [: Frontend / HTML]
            [: Builders expose source libraries through `LibrarySet.source_libraries`. The HTML builder provides `@html` as a default source library with basic HTML tag helpers and formatting utilities. These libraries follow the same module visibility/facade rules as project modules. Prefix collisions between builder-provided and project-local libraries produce a hard config error.]
        ]

        [data:
            [: Project-local libraries]
            [: Partial]
            [: Targeted coverage for config-defined project library folders, import resolution, and facade gating.]
            [: Frontend / HTML]
            [: Project-local source libraries are discovered from [codesnippet:#library_folders] (default convention: `/lib` when not configured). Imports resolve through the library prefix (for example, `/lib/helper` as `@helper` or `/packages/helper` as `@helper`). Public cross-module access goes through `#mod.bst`; bypassing implementation files from outside the module is rejected. Module boundary enforcement applies to all module roots, not only project-local libraries.]
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

    [styled_table:
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
    If syntax exists, it should fail with a structured diagnostic rather than panic or lower incorrectly.

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
            [: `case Variant(original_name as local_name) =>` binds the payload field to a different local name visible in the guard and arm body.]
        ]

        [data:
            [: `as` keyword outside supported domains]
            [: Rejected / Reserved]
            [: [codesnippet:as_in_expression_rejected], [codesnippet:as_in_variable_declaration_rejected], [codesnippet:as_in_function_signature_rejected], [codesnippet:as_in_non_choice_match_pattern_rejected], [codesnippet:as_in_match_pattern_lead_rejected]]
            [: `as` is only valid in three contexts: type aliases (`TypeAlias as ExistingType`), import clauses (`import @path/symbol as local_name`), and choice payload captures (`case Variant(field as local_name)`). All other uses are rejected with targeted diagnostics.]
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
            [: Import re-exports]
            [: Supported]
            [: Targeted coverage for single, aliased, grouped, nested grouped, cross-library, external function, external type, duplicate, collision, no-local-binding, and invalid-location diagnostics.]
            [: `#import @...` is valid only in `#mod.bst` module facades (including source libraries). It re-exports source or external package symbols through the facade export map without creating a local binding in `#mod.bst` or sibling implementation files. Grouped imports and per-symbol aliases are supported; wildcard and namespace imports remain deferred.]
        ]

        [data:
            [: Source-library HIR caching]
            [: Deferred]
            [: No precompiled source-library cache.]
            [: Source libraries are compiled from `.bst` source each build. HIR caching is deferred until the library system is stable.]
        ]

        [data:
            [: Package manager]
            [: Deferred]
            [: No package manager, remote registries, or dependency resolution.]
            [: The library system plan explicitly defers package management, versioning, lockfiles, and remote fetching.]
        ]

        [data:
            [: Namespace / wildcard imports]
            [: Deferred]
            [: Unsupported syntax should fail cleanly.]
            [: No `import @pkg.*` or namespace-import syntax is planned for Alpha.]
        ]

        [data:
            [: Choice recursive types]
            [: Deferred]
            [: [codesnippet:choice_recursive_direct_declaration_deferred]]
            [: Requires layout/indirection design.]
        ]

        [data:
            [: Choice generic declarations]
            [: Deferred]
            [: [codesnippet:choice_generic_declaration_deferred]]
            [: Syntax ideas live in [codesnippet:docs/src/docs/generics.md]; do not implement here.]
        ]

        [data:
            [: Choice payload structural equality]
            [: Rejected / Reserved]
            [: [codesnippet:choice_payload_structural_equality_deferred]]
            [: Payload structural equality is deferred. Use pattern matching and compare fields inside the matching arm.]
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
