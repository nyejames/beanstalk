import @styles/docs {navbar, section, codebase_theme_head}

page_title #= "Compiler-facing language reference"
page_description #= "Current language authority, focused replacement references and related compiler design documentation."
page_head #= codebase_theme_head

#[navbar]

#[section, $md:
    @../ (Codebase) / Compiler-facing language reference

    # Compiler-facing language reference

    Compiler changes should start from the language contract. `docs/language-overview.md` remains the complete reference for syntax, semantics, edge cases and deferred language surface.
]

#[section, $md:
    ## Focused references

    The language documentation is being split into smaller references without changing the active authority during the migration.

    Loops is the first completed replacement set:

    - @../../loops (Loops): public Basic and Advanced documentation
    - `docs/src/docs/loops/conditional-loops.bd`
    - `docs/src/docs/loops/collection-loops.bd`
    - `docs/src/docs/loops/range-loops.bd`
    - `docs/src/docs/loops/loop-control.bd`

    The unsuffixed files contain detailed semantic replacements under parity review. They do not supersede `docs/language-overview.md` yet.
]

#[section, $md:
    ## Reference map

    `docs/src/docs/codebase/language/overview.bd` lists each focused replacement completed so far and points back to the consolidated authority.
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
