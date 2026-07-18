import @./header-parsing {content as reference_content}
import @styles/docs {navbar, section, codebase_theme_head}

page_title #= "Stage 2: Header syntax and interface binding"
page_description #= "How syntax is retained once, then bound to completed provider interfaces."
page_head #= codebase_theme_head

#[navbar]

#[section, $md:
    @../../ (Compiler design) / @../ (Compiler pipeline) / Stage 2: Header syntax and interface binding

    # Stage 2: Header syntax and interface binding

    Syntax preparation records declaration and import shells once. Interface binding later resolves those shells against immutable provider interfaces.
]

#[section, reference_content]
