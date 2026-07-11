import @./html-project-assembly {content as reference_content}
import @styles/docs {navbar, section, codebase_theme_head}

page_title #= "HTML project assembly"
page_description #= "How the HTML builder assembles routes, fragments and project artifacts."
page_head #= codebase_theme_head

#[navbar]

#[section, $md:
    @../../ (Compiler design) / @../ (Backend handoff) / HTML project assembly

    # HTML project assembly

    The HTML builder owns route-level artifact assembly. It consumes compiled modules and returns explicit output files plus cleanup policy.
]

#[section, reference_content]
