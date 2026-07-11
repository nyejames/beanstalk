import @./external-js-and-runtime-assets {content as reference_content}
import @styles/docs {navbar, section, codebase_theme_head}

page_title #= "External JavaScript and runtime assets"
page_description #= "How Beanstalk turns non-Beanstalk imports into typed packages and runtime glue."
page_head #= codebase_theme_head

#[navbar]

#[section, $md:
    @../../ (Compiler design) / @../ (Backend handoff) / External JavaScript and runtime assets

    # External JavaScript and runtime assets

    Provider-backed external JavaScript becomes a typed package surface before AST. The backend emits runtime assets and glue only when reachable compiled code needs them.
]

#[section, reference_content]
