import @./overview {content as reference_content}
import @styles/docs {navbar, section, codebase_theme_head}

page_title #= "Compiler design: source to page"
page_description #= "A chronological educational outline of Beanstalk's compiler and build pipeline."
page_head #= codebase_theme_head

#[navbar]

#[section, $md:
    @../ (Codebase) / Compiler design

    # Compiler design: source to page

    One chronological route follows a Beanstalk project from source files to generated HTML. The outline marks accepted architecture, current implementation and roadmap gaps for the later writing pass.
]

#[section, reference_content]
