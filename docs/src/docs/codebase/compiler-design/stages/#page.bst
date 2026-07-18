import @styles/docs {navbar, section, codebase_theme_head}

page_title #= "Compiler pipeline"
page_description #= "A high-level learning path through build orchestration and compiler stages."
page_head #= codebase_theme_head

#[navbar]

#[section, $md:
    @../ (Compiler design) / Compiler pipeline

    # Compiler pipeline

    Follow the pipeline to see which owner creates each fact and which later stage is allowed to consume it.
]

#[section, $md:
    ## Build preparation

    - @./project-structure (Build-system Stage 0): config, source ownership, graphs and deterministic scheduling

    ## Compiler stages

    - @./tokenization (Stage 1: Tokenization)
    - @./header-parsing (Stage 2: Header syntax and interface binding)
    - @./dependency-sorting (Stage 3: Local declaration ordering)
    - @./ast-construction (Stage 4: AST semantics)
    - @./hir-generation (Stage 5: HIR and validation)
    - @./borrow-validation (Stage 6: Borrow validation)

    ## Target work

    - @./backend-lowering (Target planning and backend lowering)
]
