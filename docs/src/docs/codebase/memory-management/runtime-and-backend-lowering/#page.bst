import @./runtime-and-backend-lowering {content as reference_content}
import @styles/docs {navbar, section, codebase_theme_head}

page_title #= "Runtime and backend memory lowering"
page_description #= "How GC baseline and ownership-aware lowering meet backend targets."
page_head #= codebase_theme_head

#[navbar]

#[section, $md:
    @../ (Memory management) / Runtime and backend memory lowering

    # Runtime and backend memory lowering

    Backends may choose different allocation and GC representations. They may not choose different language semantics.
]

#[section, reference_content]
