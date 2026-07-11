import @./borrow-validation {content as reference_content}
import @styles/docs {navbar, section, codebase_theme_head}

page_title #= "Stage 6: Borrow validation"
page_description #= "How Beanstalk validates exclusivity, aliasing and move safety."
page_head #= codebase_theme_head

#[navbar]

#[section, $md:
    @../../ (Compiler design) / @../ (Pipeline stages) / Stage 6: Borrow validation

    # Stage 6: Borrow validation

    Borrow validation reads HIR and writes side-table facts. HIR stays the semantic representation and borrow facts sit beside it.
]

#[section, reference_content]
