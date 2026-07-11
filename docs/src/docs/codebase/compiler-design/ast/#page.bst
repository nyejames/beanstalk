import @styles/docs {navbar, section, codebase_theme_head}

page_title #= "AST internals"
page_description #= "How AST construction resolves semantics and prepares clean HIR input."
page_head #= codebase_theme_head

#[navbar]

#[section, $md:
    @../ (Compiler design) / AST internals

    # AST internals

    AST construction is the semantic centre of the frontend. Headers have already discovered and ordered top-level declarations. AST gives those declarations meaning, checks executable bodies and finishes source-level work before HIR.
]

#[section, $md:
    ## Three jobs

    - `build_ast_environment` resolves declarations and builds the shared semantic environment
    - `emit_ast_nodes` parses and checks executable bodies against that completed environment
    - `finalize_ast` prepares constants, templates and metadata for the HIR boundary

    By the end of AST construction, types are canonical, generics and trait evidence are resolved, template semantics are complete and compile-time page fragments have been separated from runtime code.
]

#[section, $md:
    ## Explore AST internals

    - @./environment-and-type-resolution (AST environment and type resolution): scopes, declarations, type identity and contextual coercion
    - @./expressions-statements-and-terminality (Expressions, statements and terminality): executable bodies, value flow and valid exits
    - @./constants-and-folding (Constants and folding): compile-time values, const facts and runtime-aligned folding
    - @./templates-tir-and-reactivity (Templates, TIR and reactivity): template composition, runtime handoff and reactive metadata
    - @./generics-traits-and-casts (Generics, traits and casts): static evidence, concrete instances and explicit conversion
]
