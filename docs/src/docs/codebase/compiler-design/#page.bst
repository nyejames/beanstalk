import @styles/docs {navbar, section, codebase_theme_head, codeblock}

page_title #= "Compiler design overview"
page_description #= "Staged compiler architecture, pipeline map and cross-cutting design references."
page_head #= codebase_theme_head

#[navbar]

#[section, $md:
    @../ (Codebase) / Compiler design overview

    # Compiler design overview

    Beanstalk's compiler moves through a staged frontend, then crosses a strict handoff into project assembly and target lowering. Each stage narrows the data it receives so later stages can rely on clear, checked contracts.
]

#[section, $md:
    ## Compiler flow

    [codeblock, $code("text"):
        project files
            -> Stage 0: project structure and source discovery
            -> Stage 1: tokenization
            -> Stage 2: header parsing
            -> Stage 3: dependency sorting
            -> Stage 4: AST construction
            -> Stage 5: HIR generation
            -> Stage 6: borrow validation
            -> Stage 7: backend lowering
            -> output files
    ]
]

#[section, $md:
    ## Explore by area

    - @./stages (Pipeline stages): follow source from project discovery through backend lowering
    - @./ast (AST internals): see where declarations, expressions, constants, templates, generics, traits and casts gain semantic meaning
    - @./backend (Backend handoff): see how validated HIR becomes project artifacts and target code
]

#[section, $md:
    ## Stage pages

    - @./stages/project-structure (Stage 0: Project structure)
    - @./stages/tokenization (Stage 1: Tokenization)
    - @./stages/header-parsing (Stage 2: Header parsing)
    - @./stages/dependency-sorting (Stage 3: Dependency sorting)
    - @./stages/ast-construction (Stage 4: AST construction)
    - @./stages/hir-generation (Stage 5: HIR generation)
    - @./stages/borrow-validation (Stage 6: Borrow validation)
    - @./stages/backend-lowering (Stage 7: Backend lowering)
]

#[section, $md:
    ## AST pages

    - @./ast/environment-and-type-resolution (AST environment and type resolution)
    - @./ast/expressions-statements-and-terminality (Expressions, statements and terminality)
    - @./ast/constants-and-folding (Constants and folding)
    - @./ast/templates-tir-and-reactivity (Templates, TIR and reactivity)
    - @./ast/generics-traits-and-casts (Generics, traits and casts)
]

#[section, $md:
    ## Backend pages

    - @./backend/validation-and-output-writing (Backend validation and output writing)
    - @./backend/html-project-assembly (HTML project assembly)
    - @./backend/js-lowering (JavaScript lowering)
    - @./backend/wasm-lowering (Wasm lowering)
    - @./backend/external-js-and-runtime-assets (External JavaScript and runtime assets)
    - @./backend/tracked-assets (Tracked assets)
]

#[section, $md:
    ## Cross-cutting design

    - @./build-system-and-frontend-boundary (Build-system and frontend boundary)
    - @./diagnostics-paths-and-symbols (Diagnostics, paths and symbols)
    - @./type-identity-and-coercion (Type identity and coercion)
    - @./imports-libraries-and-external-packages (Imports, libraries and external packages)
    - @./entry-start-and-page-fragments (Active-root start and page fragments)
    - @./parallelism-and-determinism (Parallelism and determinism)
    - @../memory-management (Memory management)
]

#[section, $md:
    ## Current implementation

    See @../../progress (Progress matrix) for current support, partial coverage, experimental paths and major gaps.
]
