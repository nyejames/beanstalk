import @./generics-traits-and-casts {content as reference_content}
import @styles/docs {navbar, section, codebase_theme_head}

page_title #= "Generics, traits and casts"
page_description #= "How Beanstalk resolves generics, traits and cast evidence before HIR."
page_head #= codebase_theme_head

#[navbar]

#[section, $md:
    @../../ (Compiler design) / @../ (AST internals) / Generics, traits and casts

    # Generics, traits and casts

    Generics, traits and cast evidence resolve before HIR. HIR gets concrete calls and explicit runtime operations.
]

#[section, reference_content]
