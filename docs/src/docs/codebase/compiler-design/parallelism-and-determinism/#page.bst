import @./parallelism-and-determinism {content as reference_content}
import @styles/docs {navbar, section, codebase_theme_head}

page_title #= "Parallelism and determinism"
page_description #= "Where the compiler can parallelize work without losing deterministic identity."
page_head #= codebase_theme_head

#[navbar]

#[section, $md:
    @../ (Compiler design) / Parallelism and determinism

    # Parallelism and determinism

    Fast is good. Repeatable is required. Parallel passes must merge interned identities in deterministic order before later stages see them.
]

#[section, reference_content]
