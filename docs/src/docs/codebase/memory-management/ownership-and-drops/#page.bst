import @./ownership-and-drops {content as reference_content}
import @styles/docs {navbar, section, codebase_theme_head}

page_title #= "Ownership and drops"
page_description #= "How Beanstalk infers ownership and lowers conditional destruction."
page_head #= codebase_theme_head

#[navbar]

#[section, $md:
    @../ (Memory management) / Ownership and drops

    # Ownership and drops

    Ownership is an optimisation state layered over GC-safe semantics. The compiler transfers destruction responsibility only where static analysis proves it's safe.
]

#[section, reference_content]
