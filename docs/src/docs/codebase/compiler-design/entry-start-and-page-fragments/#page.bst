import @./entry-start-and-page-fragments {content as reference_content}
import @styles/docs {navbar, section, codebase_theme_head}

page_title #= "Active-root start and page fragments"
page_description #= "How Beanstalk separates active-root runtime code from compile-time page fragments."
page_head #= codebase_theme_head

#[navbar]

#[section, $md:
    @../ (Compiler design) / Active-root start and page fragments

    # Active-root start and page fragments

    Active module roots run top-level code through an implicit `start()`. Const templates fold into fragments that builders merge later, so HIR never has to understand HTML assembly.
]

#[section, reference_content]
