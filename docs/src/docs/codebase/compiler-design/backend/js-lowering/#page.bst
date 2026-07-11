import @./js-lowering {content as reference_content}
import @styles/docs {navbar, section, codebase_theme_head}

page_title #= "JavaScript lowering"
page_description #= "How Beanstalk lowers HIR to JavaScript and runtime helpers."
page_head #= codebase_theme_head

#[navbar]

#[section, $md:
    @../../ (Compiler design) / @../ (Backend handoff) / JavaScript lowering

    # JavaScript lowering

    The JS backend owns HIR-to-JavaScript lowering and JS runtime helper emission. The HTML-JS path uses it to build page bundles.
]

#[section, reference_content]
