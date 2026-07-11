import @./diagnostics-paths-and-symbols {content as reference_content}
import @styles/docs {navbar, section, codebase_theme_head}

page_title #= "Diagnostics, paths and symbols"
page_description #= "How Beanstalk keeps user-facing diagnostics structured and durable across the whole build."
page_head #= codebase_theme_head

#[navbar]

#[section, $md:
    @../ (Compiler design) / Diagnostics, paths and symbols

    # Diagnostics, paths and symbols

    Diagnostics are part of the compiler architecture, not a final pretty-printing step. Structured facts produced early stay useful all the way to render time.
]

#[section, reference_content]
