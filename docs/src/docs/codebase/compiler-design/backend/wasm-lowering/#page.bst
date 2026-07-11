import @./wasm-lowering {content as reference_content}
import @styles/docs {navbar, section, codebase_theme_head}

page_title #= "Wasm lowering"
page_description #= "How Beanstalk lowers HIR to Wasm and the HTML-Wasm boundary."
page_head #= codebase_theme_head

#[navbar]

#[section, $md:
    @../../ (Compiler design) / @../ (Backend handoff) / Wasm lowering

    # Wasm lowering

    The core Wasm backend owns HIR-to-Wasm-LIR lowering. The HTML-Wasm path is builder orchestration around that backend.
]

#[section, reference_content]
