import @./what-a-compiler-does {content as reference_content}
import @styles/docs {navbar, section, codebase_theme_head}

page_title #= "01. What a compiler does"
page_description #= "How compiler responsibilities and intermediate representations fit together."
page_head #= codebase_theme_head

#[navbar]

#[section, $md:
    @../ (Compiler design) / 01. What a compiler does

    # What a compiler actually does

    Outline for the later educational writing pass. The canonical architecture remains in the linked authority documents.
]

#[section, reference_content]

#[section, $md:
    @../ (Compiler design) / @../beanstalk-design-choices (Next)
]

