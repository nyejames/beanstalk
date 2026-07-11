import @./tracked-assets {content as reference_content}
import @styles/docs {navbar, section, codebase_theme_head}

page_title #= "Tracked assets"
page_description #= "How rendered path usages become emitted project assets."
page_head #= codebase_theme_head

#[navbar]

#[section, $md:
    @../../ (Compiler design) / @../ (Backend handoff) / Tracked assets

    # Tracked assets

    Tracked assets are a builder policy over frontend-rendered path usages. The frontend records facts. The HTML builder decides what becomes an emitted asset.
]

#[section, reference_content]
