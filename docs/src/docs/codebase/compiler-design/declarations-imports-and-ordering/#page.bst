import @./declarations-imports-and-ordering {content as reference_content}
import @styles/docs {navbar, section, codebase_theme_head}

page_title #= "06. Declarations, imports and ordering"
page_description #= "How retained syntax becomes bound visibility and ordered declarations."
page_head #= codebase_theme_head

#[navbar]

#[section, $md:
    @../ (Compiler design) / 06. Declarations, imports and ordering

    # Finding the program's shape

    Prepared shells, bound imports and local declaration order give each module its top-level silhouette before bodies type-check.
]

#[section, reference_content]

#[section, $md:
    @../ (Compiler design) / @../tokenization (Previous) / @../ast-semantics-and-types (Next)
]
