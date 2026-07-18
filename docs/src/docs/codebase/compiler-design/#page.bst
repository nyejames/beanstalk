import @styles/docs {navbar, section, codebase_theme_head, codeblock}

page_title #= "Compiler design overview"
page_description #= "Supplementary teaching outlines for Beanstalk's compiler and build pipeline."
page_head #= codebase_theme_head

#[navbar]

#[section, $md:
    @../ (Codebase) / Compiler design overview

    # Compiler design overview

    These pages are supplementary teaching scaffolding. The canonical architecture remains in `docs/compiler-design-overview.md` and `docs/build-system-design.md`.
]

#[section, $md:
    ## Pipeline

    [codeblock, $code("text"):
        command and Stage 0 build orchestration
            -> Stage 1: tokenization
            -> Stage 2: header syntax and interface binding
            -> Stage 3: local declaration ordering
            -> Stage 4: AST semantics
            -> Stage 5: HIR and validation
            -> Stage 6: borrow validation
            -> build-owned target planning and backend lowering
            -> output artefacts
    ]
]

#[section, $md:
    ## Explore by area

    - @./stages (Pipeline stages): learn how source and retained metadata move through the pipeline
    - @./ast (AST semantics): learn where source meaning, constants, templates, generics, traits and casts are resolved
    - @./backend (Target and backend concepts): learn how validated semantic artefacts become target output
]

#[section, $md:
    ## Cross-cutting concepts

    - @./build-system-and-frontend-boundary (Build-system and compiler boundary)
    - @./diagnostics-paths-and-symbols (Diagnostics, paths and symbols)
    - @./type-identity-and-coercion (Type identity and coercion)
    - @./imports-packages-and-bindings (Imports, packages and bindings)
    - @./entry-start-and-page-fragments (Entry activation and page fragments)
    - @./parallelism-and-determinism (Parallelism and determinism)
    - @../memory-management (Memory management)
]

#[section, $md:
    ## Authority

    These pages explain concepts. They do not override the architecture authorities, language authorities, memory authorities or @../../progress (Progress matrix).
]
