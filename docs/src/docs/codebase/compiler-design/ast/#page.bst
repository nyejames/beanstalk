import @styles/docs {navbar, section, codebase_theme_head}

page_title #= "AST semantics"
page_description #= "Supplementary explanations of semantic environments, bodies, constants, templates and static evidence."
page_head #= codebase_theme_head

#[navbar]

#[section, $md:
    @../ (Compiler design) / AST semantics

    # AST semantics

    AST is the compiler stage where parsed source acquires checked semantic meaning.
]

#[section, $md:
    ## Conceptual flow

    - Environment construction resolves declarations and canonical identities.
    - Body checking validates executable expressions and control flow against that environment.
    - Finalisation folds constants and templates, validates public boundaries and produces clean module artefact lanes.

    Internal implementation steps may change. The ownership boundary is the important part.
]

#[section, $md:
    ## Explore

    - @./environment-and-type-resolution (Semantic environments and type identity)
    - @./expressions-statements-and-terminality (Expressions, statements and terminality)
    - @./constants-and-folding (Constants and folding)
    - @./templates-tir-and-reactivity (Templates, TIR and reactivity)
    - @./generics-traits-and-casts (Generics, traits and casts)
]
