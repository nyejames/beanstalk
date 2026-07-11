import @./borrow-validation {content as reference_content}
import @styles/docs {navbar, section, codebase_theme_head}

page_title #= "Borrow validation"
page_description #= "How Beanstalk checks exclusivity, aliasing and move safety."
page_head #= codebase_theme_head

#[navbar]

#[section, $md:
    @../ (Memory management) / Borrow validation

    # Borrow validation

    Borrow validation runs after HIR generation. It writes analysis facts beside HIR without changing it.
]

#[section, reference_content]
