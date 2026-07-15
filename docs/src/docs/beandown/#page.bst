import @./beandown-files {
    content as beandown_files,
}

import @./beandown-files-basic {
    content as beandown_files_basic,
}

import @./implicit-markdown {
    content as implicit_markdown,
}

import @./implicit-markdown-basic {
    content as implicit_markdown_basic,
}

import @./content-imports {
    content as content_imports,
}

import @./content-imports-basic {
    content as content_imports_basic,
}

import @./template-scope {
    content as template_scope,
}

import @./template-scope-basic {
    content as template_scope_basic,
}

import @./beandown-limits {
    content as beandown_limits,
}

import @./beandown-limits-basic {
    content as beandown_limits_basic,
}

import @styles/docs {
    navbar,
    section,
    language_theme_head,
    doc_level,
    doc_pager,
}

page_title #= "Beandown"
page_description #= "Beanstalk-aware Markdown content files for HTML projects."
page_head #= language_theme_head

#[navbar]

#[section, $md:
    @../ (Documentation) / Beandown

    # Beandown

    Beandown is Markdown that already lives inside Beanstalk's template world.

    Write the content directly. Import the folded result as `content`. Reach for a
    normal `.bst` file when the job stops being content-shaped.
]

#[section:
    [doc_level:
        [$insert("key"):beandown-files]
        [$insert("title"):Beandown files]
        [$insert("advanced"):[beandown_files]]

        [beandown_files_basic]
    ]
]

#[section:
    [doc_level:
        [$insert("key"):implicit-markdown]
        [$insert("title"):Implicit Markdown]
        [$insert("advanced"):[implicit_markdown]]

        [implicit_markdown_basic]
    ]
]

#[section:
    [doc_level:
        [$insert("key"):content-imports]
        [$insert("title"):Content imports]
        [$insert("advanced"):[content_imports]]

        [content_imports_basic]
    ]
]

#[section:
    [doc_level:
        [$insert("key"):template-scope]
        [$insert("title"):Template scope]
        [$insert("advanced"):[template_scope]]

        [template_scope_basic]
    ]
]

#[section:
    [doc_level:
        [$insert("key"):beandown-limits]
        [$insert("title"):Beandown limits]
        [$insert("advanced"):[beandown_limits]]

        [beandown_limits_basic]
    ]
]

#[section:
    [doc_pager:
        [$insert("previous_path"):../packages/]
        [$insert("previous_title"):Packages and imports]
        [$insert("next_path"):../markdown/]
        [$insert("next_title"):Plain Markdown]
    ]
]
