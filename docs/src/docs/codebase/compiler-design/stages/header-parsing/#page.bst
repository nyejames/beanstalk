import @./header-parsing {content as reference_content}
import @styles/docs {navbar, section, codebase_theme_head}

page_title #= "Stage 2: Header parsing"
page_description #= "How Beanstalk discovers top-level declarations and builds dependency edges."
page_head #= codebase_theme_head

#[navbar]

#[section, $md:
    @../../ (Compiler design) / @../ (Pipeline stages) / Stage 2: Header parsing

    # Stage 2: Header parsing

    Header parsing parses declaration shells once so AST doesn't have to rediscover them from raw tokens. That is the whole point.
]

#[section, reference_content]
