import @styles/docs {navbar, section, codebase_theme_head}

page_title #= "Compiler-facing language reference"
page_description #= "Routing for canonical language semantics, public explanations and related compiler design references."
page_head #= codebase_theme_head

#[navbar]

#[section, $md:
    @../ (Codebase) / Compiler-facing language reference

    # Compiler-facing language reference

    Compiler changes should start from the language contract. Canonical language references now live beside their public routes as concepts are migrated out of the consolidated overview.
]

#[section, $md:
    ## Migrated language references

    Loops are the first migrated language area.

    - @../../loops (Loops): public Basic and Advanced documentation
    - `docs/src/docs/loops/conditional-loops.bd`
    - `docs/src/docs/loops/collection-loops.bd`
    - `docs/src/docs/loops/range-loops.bd`
    - `docs/src/docs/loops/loop-control.bd`

    The unsuffixed Beandown files own exact loop semantics.
]

#[section, $md:
    ## Remaining language reference

    `docs/language-overview.md` remains canonical for every language concept that has not moved into a focused unsuffixed Beandown reference.

    The complete authority map is `docs/src/docs/codebase/language/overview.bd`.
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
