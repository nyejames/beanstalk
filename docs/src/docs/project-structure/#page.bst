import @lib/html {format, style}
import @styles/docs {navbar, title, section, palette, table, header, data, codeblock, codesnippet, theme_head}

# page_title = "Projects, Modules and HTML Builder"
# page_description = "How Beanstalk projects are structured, how modules are discovered, and how the HTML project builder emits pages."
# page_head = theme_head

#[navbar]
#[title, style.center: PROJECTS, MODULES AND HTML BUILDER]

#[section, $markdown:
    Beanstalk projects are built around a small set of conventions:

    - a project root with a [codesnippet:#config.bst] file
    - an entry root, usually [codesnippet:src]
    - module entry files named like [codesnippet:#page.bst]
    - normal [codesnippet:.bst] files that provide declarations for module entries
    - builder-owned output folders such as [codesnippet:dev] and [codesnippet:release]

    At this stage of development, this is the only supported project structure to rely on.
    The rules are deliberately narrow so the compiler, dev server, and HTML builder can stay predictable while the language is still evolving.
]

#[section, $markdown:
    # The modular build system

    The Beanstalk compiler frontend is shared by every project builder.
    It discovers modules, parses files, type checks code, lowers to HIR, and runs borrow validation.
    A project builder then decides how those compiled modules become real output files.

    The current user-facing project builder is the HTML project builder.
    It consumes each compiled module and emits one HTML page per module entry file.

    This split matters because Beanstalk is not designed as a single hard-coded web compiler.
    The same frontend can later feed other builders, such as a Wasm module builder, a documentation builder, or project-specific tooling.

    [table:
        [header: [: Layer] [: Responsibility] ]

        [data:
            [: Compiler frontend]
            [: Discovers modules, resolves imports, checks types, lowers code, validates borrowing, and produces backend-ready module data.]
        ]

        [data:
            [: Project builder]
            [: Interprets compiled modules for a specific project type and decides which output files to generate.]
        ]

        [data:
            [: HTML project builder]
            [: Treats each module entry as a page and emits HTML files, with runtime code embedded as JavaScript by default.]
        ]
    ]
]

#[section, $markdown:
    # Project root and #config.bst

    A normal HTML project has a [codesnippet:#config.bst] file at the project root.
    Config files use ordinary Beanstalk top-level constant syntax.

    [codeblock, $code("bst"):
        # project = "html"
        # entry_root = "src"
        # dev_folder = "dev"
        # output_folder = "release"
        # page_url_style = "trailing_slash"
        # redirect_index_html = true
        # name = "html_project"
        # version = "0.1.0"
        # author = ""
        # license = "MIT"

        # origin = "/beanstalk"
        # html_lang = "en"
        # html_title_postfix = " | Beanstalk"
        # root_folders = {
            @lib,
            @assets,
        }
    ]

    Important settings:

    [table:
        [header: [: Setting] [: Meaning] ]

        [data:
            [: [codesnippet:#project]]
            [: Selects the project type. For the documentation site and current web projects, use [codesnippet:"html"].]
        ]

        [data:
            [: [codesnippet:#entry_root]]
            [: The source folder that is searched for module entry files. Usually [codesnippet:"src"].]
        ]

        [data:
            [: [codesnippet:#dev_folder]]
            [: Where development builds are written. The dev server serves from here.]
        ]

        [data:
            [: [codesnippet:#output_folder]]
            [: Where release builds are written when [codesnippet:--release] is used.]
        ]

        [data:
            [: [codesnippet:#root_folders]]
            [: Extra top-level project folders that non-relative imports can resolve through before falling back to the entry root.]
        ]

        [data:
            [: HTML-specific settings]
            [: Settings such as [codesnippet:#origin], [codesnippet:#html_lang], [codesnippet:#html_title_postfix], [codesnippet:#page_url_style], and [codesnippet:#redirect_index_html] are interpreted by the HTML builder.]
        ]
    ]

    Config is loaded before module discovery.
    That means [codesnippet:#entry_root] and [codesnippet:#root_folders] shape how the project is searched and how imports resolve.
]

#[section, $markdown:
    # Module entry files

    A module entry file is a Beanstalk file whose file name starts with [codesnippet:#].
    In directory projects, the compiler searches the configured entry root for [codesnippet:#*.bst] files.
    [codesnippet:#config.bst] is not a module entry.

    Examples:

    [codeblock, $code("txt"):
        src/#page.bst              -> module entry
        src/docs/#page.bst         -> module entry
        src/docs/basics/#page.bst  -> module entry
        src/styles/docs.bst        -> normal source file
        #config.bst                -> project config, not a module entry
    ]

    Only module entry files have top-level runtime execution.
    The compiler treats the entry file's top-level runtime code as an implicit [codesnippet:start] function for that module.

    Normal source files can contain imports, functions, structs, choices, type aliases, and constants.
    They should not rely on top-level runtime statements being executed by themselves.
]

#[section, $markdown:
    # Builder-owned meaning of # file names

    The compiler discovers [codesnippet:#*.bst] files as module entries.
    The project builder decides what those entries mean.

    The HTML project builder currently treats them as page entries.
    The file name and folder decide the output route.

    [table:
        [header: [: Entry file] [: HTML output path] ]

        [data:
            [: [codesnippet:src/#page.bst]]
            [: [codesnippet:index.html]]
        ]

        [data:
            [: [codesnippet:src/docs/#page.bst]]
            [: [codesnippet:docs/index.html]]
        ]

        [data:
            [: [codesnippet:src/docs/basics/#page.bst]]
            [: [codesnippet:docs/basics/index.html]]
        ]

        [data:
            [: [codesnippet:src/#about.bst]]
            [: [codesnippet:about/index.html]]
        ]
    ]

    This gives pages stable folder-backed routes.
    A [codesnippet:#page.bst] file means "the index page for this folder".
    Other [codesnippet:#name.bst] files become named routes under the same folder.
]

#[section, $markdown:
    # Top-level templates in page entries

    Top-level templates are special.
    In the HTML project builder, direct top-level templates in a module entry file become page fragments.

    There are two forms:

    [table:
        [header: [: Form] [: Meaning] ]

        [data:
            [: [codesnippet:#[...]]]
            [: A top-level const template. It must fold at compile time and is emitted as static HTML.]
        ]

        [data:
            [: [codesnippet:[...]]]
            [: A top-level runtime template. It is evaluated by the module's implicit start function and inserted into the page at runtime.]
        ]
    ]

    Example:

    [codeblock, $code("bst"):
        import @styles/docs {navbar, section, theme_head}

        # page_title = "Home"
        # page_description = "Welcome page."
        # page_head = theme_head

        -- Const fragments are folded by the compiler and emitted directly.
        #[navbar]

        #[section, $markdown:
            # Welcome
            This part is static.
        ]

        name = "Beanstalk"

        -- Runtime fragments are produced by the implicit start function.
        [section, $markdown:
            Runtime hello from [name].
        ]
    ]

    The HTML builder preserves source order between static and runtime fragments.
    Static fragments are emitted immediately.
    Runtime fragments use generated slots in the HTML document, then the embedded JavaScript calls the module start function and fills those slots.

    Do not put page-producing top-level templates in helper files.
    Helper files should export reusable declarations, then page entries should import and use them.
]

#[section, $markdown:
    # Visibility and exports with #

    The [codesnippet:#] prefix on top-level declarations controls module visibility.

    [table:
        [header: [: Declaration kind] [: Meaning of #] ]

        [data:
            [: Variable declaration]
            [: Creates an exported compile-time constant.]
        ]

        [data:
            [: Function]
            [: Exports the function so other files in the module can import it.]
        ]

        [data:
            [: Struct or choice]
            [: Exports the type.]
        ]

        [data:
            [: Type alias]
            [: Exports the alias.]
        ]

        [data:
            [: Top-level template]
            [: [codesnippet:#[...]] is not a normal exported value. It is an entry-file-only const page fragment.]
        ]
    ]

    Regular top-level declarations without [codesnippet:#] are private to their file/module context and cannot be imported from other files.

    Example helper file:

    [codeblock, $code("bst"):
        -- src/components/card.bst

        # CardData = |
            title String,
            body String,
        |

        # render_card |card CardData| -> String:
            return [:
                <article>
                    <h2>[card.title]</h2>
                    <p>[card.body]</p>
                </article>
            ]
        ;

        private_note || -> String:
            return "This is not exported."
        ;
    ]

    Example page entry:

    [codeblock, $code("bst"):
        -- src/cards/#page.bst

        import @components/card {CardData, render_card}

        # page_title = "Cards"

        card = CardData("Hello", "This was rendered by a helper function.")

        [:
            [render_card(card)]
        ]
    ]
]

#[section, $markdown:
    # Imports and path resolution

    Imports target exported symbols, not whole files.

    [codeblock, $code("bst"):
        -- Single exported symbol
        import @components/card/render_card

        -- Grouped exported symbols
        import @components/card {CardData, render_card}

        -- Local alias
        import @components/card/render_card as card

        -- Relative import from the current file's folder
        import @./shared/title/render_title

        -- Parent-folder relative import
        import @../components/card {CardData, render_card}
    ]

    Path resolution is project-aware:

    - [codesnippet:@./] and [codesnippet:@../] resolve relative to the importing file.
    - Non-relative imports first check configured [codesnippet:#root_folders] from the project root.
    - If no root folder matches, the import resolves from [codesnippet:#entry_root].

    For the docs project, [codesnippet:#root_folders] includes [codesnippet:@lib] and [codesnippet:@assets].
    That means [codesnippet:import @lib/html ...] resolves through the project root, while [codesnippet:import @styles/docs ...] resolves through [codesnippet:docs/src/styles/docs.bst].
]

#[section, $markdown:
    # Example: the documentation site structure

    The Beanstalk documentation website is itself a Beanstalk HTML project.
    Ignoring markdown notes and generated files, its source shape is roughly:

    [codeblock, $code("txt"):
        docs/
        ├── #config.bst
        ├── src/
        │   ├── #page.bst
        │   ├── docs/
        │   │   ├── #page.bst
        │   │   ├── basics/#page.bst
        │   │   ├── templates/#page.bst
        │   │   ├── functions/#page.bst
        │   │   ├── if-statements/#page.bst
        │   │   ├── loops/#page.bst
        │   │   ├── collections/#page.bst
        │   │   ├── structs/#page.bst
        │   │   ├── choices/#page.bst
        │   │   ├── pattern-matching/#page.bst
        │   │   ├── aliases/#page.bst
        │   │   └── progress/#page.bst
        │   └── styles/
        │       └── docs.bst
        ├── @lib/
        │   └── html.bst
        ├── @assets/
        │   └── ...
        ├── dev/
        └── release/
    ]

    The important part is the split:

    - [codesnippet:src/] contains page entries and ordinary source files.
    - [codesnippet:#page.bst] files define HTML pages.
    - [codesnippet:styles/docs.bst] is a normal source file imported by pages.
    - [codesnippet:@lib] and [codesnippet:@assets] are project-root import folders configured by [codesnippet:#root_folders].
    - [codesnippet:dev/] and [codesnippet:release/] are output folders, not source folders.
]

#[section, $markdown:
    # Output files

    The build command compiles the project, then writes the builder's output files under the selected output root.

    [table:
        [header: [: Build mode] [: Output root] ]

        [data:
            [: Default / dev]
            [: [codesnippet:#dev_folder], usually [codesnippet:dev].]
        ]

        [data:
            [: [codesnippet:--release]]
            [: [codesnippet:#output_folder], usually [codesnippet:release].]
        ]
    ]

    In the default JS-backed HTML mode, each page module produces an HTML file.
    The generated JavaScript is embedded into that HTML file.

    In the experimental [codesnippet:--html-wasm] mode, the builder can instead emit a route folder containing:

    - [codesnippet:index.html]
    - [codesnippet:page.js]
    - [codesnippet:page.wasm]

    The HTML builder owns stale-output cleanup for generated HTML artifacts.
    Generated output folders should be treated as build artifacts, not source.
]

#[section, $markdown:
    # Future extension points

    The [codesnippet:#file.bst] module-entry convention is intentionally extensible.
    Future builders and commands can attach special meaning to new [codesnippet:#] file names without changing the basic project shape.

    ## #mod.bst library modules

    A future [codesnippet:#mod.bst] file can act as the visibility gate for a library module.
    Instead of every [codesnippet:#*.bst] file being a page, [codesnippet:#mod.bst] would define what the module exports to other modules.

    This would make module boundaries explicit and would fit future parallel compilation:

    - compile each module independently
    - expose only declarations exported from [codesnippet:#mod.bst]
    - emit either final Wasm exports or intermediate module metadata
    - allow other modules to depend on that stable export surface

    ## #test.bst test modules

    A future [codesnippet:#test.bst] file can hold project tests.
    Normal builds would ignore it.
    A planned test command could discover and run those files, similar to how [codesnippet:cargo test] discovers Rust tests.

    This is separate from the current compiler-internal integration test runner.

    ## $doc documentation templates

    The [codesnippet:$doc] style directive can eventually make documentation part of Beanstalk source.
    Top-level or nested doc templates could be stripped from normal output code, then collected by a documentation command.

    This would make documentation generation feel closer to Rust doc comments, but with Beanstalk's template system instead of comment syntax.
]