import @styles/docs {navbar, section, codebase_theme_head}

page_title #= "Compiler-facing language reference"
page_description #= "Language semantics and the compiler design references that implement them."
page_head #= codebase_theme_head

#[navbar]

#[section, $md:
    @../ (Codebase) / Compiler-facing language reference

    # Compiler-facing language reference

    Compiler changes should start from the language contract. This page connects user-visible syntax and semantics to the compiler, memory model and design boundaries that implement them.
]

#[section, $md:
    ## Language semantics

    The canonical compiler-facing reference is `docs/language-overview.md`.

    Read @../../language-overview (Language overview) for syntax, type rules, templates, collections, imports, traits, generics, errors and other user-visible behaviour.
]

#[section, $md:
    ## Related design

    - @../design-scope (Design scope): accepted mechanisms and intentionally excluded complexity
    - @../memory-management/access-and-aliasing (Access and aliasing): shared reads, explicit copies and exclusive access
    - @../compiler-design (Compiler design overview): stage ownership and cross-stage data flow
    - @../../progress (Progress matrix): current implementation support and known gaps
]

#[section, $md:
    ## More documentation

    See @../../ (All documentation) for tutorials, examples and feature-focused language pages.
]
