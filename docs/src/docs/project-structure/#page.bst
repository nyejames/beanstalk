import @html {center}
import @styles/docs {navbar, title, section, palette, styled_table, header, data, codeblock, codesnippet, theme_head}

page_title #= "Projects, Modules and HTML Builder"
page_description #= "How Beanstalk projects are structured, how modules are discovered, and how the HTML project builder emits pages."
page_head #= theme_head

#[navbar]
#[title, center: PROJECTS, MODULES AND HTML BUILDER]

#[section, $markdown:
    Beanstalk projects are built around a small set of conventions:

    - a project root with a [codesnippet:#config.bst] file
    - an entry root, usually [codesnippet:src]
    - module entry files named like [codesnippet:#page.bst]
    - module API boundaries defined by [codesnippet:#mod.bst]
    - normal [codesnippet:.bst] files that provide declarations for module entries
    - builder-owned output folders such as [codesnippet:dev] and [codesnippet:release]

    [codesnippet:#mod.bst]  = module API / visibility boundary
    [codesnippet:#page.bst] = HTML builder entry/configuration convention
    [codesnippet:#config.bst]   = build/project configuration convention

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

    [styled_table:
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
    Config files use ordinary Beanstalk top-level binding syntax.

    [codeblock, $code("bst"):
        project = "html"
        entry_root = "src"
        dev_folder = "dev"
        output_folder = "release"
        page_url_style = "trailing_slash"
        redirect_index_html = true
        name = "html_project"
        version = "0.1.0"
        author = ""
        license = "MIT"

        origin = "/beanstalk"
        html_lang = "en"
        html_title_postfix = " | Beanstalk"
        library_folders = {
            "lib",
        }
    ]

    Config keys are strict about value shape. String settings accept string literals and folded templates, [codesnippet:project] currently accepts only [codesnippet:"html"], [codesnippet:library_folders] accepts one string or a collection of strings, [codesnippet:template_const_loop_iteration_limit] requires a positive folded [codesnippet:Int] no greater than [codesnippet:1_000_000], and boolean HTML settings such as [codesnippet:redirect_index_html] require real [codesnippet:true] / [codesnippet:false] values rather than strings.

    Config values must resolve to compile-time constants. Plain immutable key declarations and explicit [codesnippet:#=] key constants are accepted, and they may reference earlier compile-time config keys or constants imported from core/builder source libraries. [codesnippet:#config.bst] may contain imports, type aliases, structs, and choices as support declarations, but every authored value-producing declaration must still be a known config key. Config-local helper constants, functions, calls, runtime statements, project-local imports, relative config imports, and [codesnippet, "#[...]"] page fragments are rejected. Const-record field projection is supported for explicit [codesnippet:#=] config keys and deferred for plain [codesnippet:=] keys.

    Important settings:

    [styled_table:
        [header: [: Setting] [: Meaning] ]

        [data:
            [: [codesnippet:project]]
            [: Selects the project type. For the documentation site and current web projects, use [codesnippet:"html"].]
        ]

        [data:
            [: [codesnippet:entry_root]]
            [: The source folder that is searched for module entry files. Usually [codesnippet:"src"].]
        ]

        [data:
            [: [codesnippet:dev_folder]]
            [: Where development builds are written. The dev server serves from here.]
        ]

        [data:
            [: [codesnippet:output_folder]]
            [: Where release builds are written when [codesnippet:--release] is used.]
        ]

        [data:
            [: [codesnippet:library_folders]]
            [: Configures source-library scan roots. Each direct child directory of a scan root becomes an import prefix. `/lib` is the default when omitted.]
        ]

        [data:
            [: [codesnippet:template_const_loop_iteration_limit]]
            [: Sets the per-loop compile-time template expansion guard. The default is [codesnippet:10_000].]
        ]

        [data:
            [: HTML-specific settings]
            [: Settings such as [codesnippet:origin], [codesnippet:html_lang], [codesnippet:html_title_postfix], [codesnippet:page_url_style], and [codesnippet:redirect_index_html] are interpreted by the HTML builder.]
        ]
    ]

    Config is loaded before module discovery.
    That means [codesnippet:entry_root] and [codesnippet:library_folders] shape how the project is searched and how imports resolve.
]

#[section, $markdown:
    # Default project scaffold

    The [codesnippet:bean new html] command generates a complete default project:

    [codeblock, $code("text"):
        project-root/
        ├── #config.bst
        ├── src/
        │   └── #page.bst
        ├── lib/
        ├── dev/
        │   └── .beanstalk_manifest
        ├── release/
        │   └── .beanstalk_manifest
        └── .gitignore
    ]

    Clarifications:

    - [codesnippet:lib/] is created empty by default. It is the conventional place for project-local source libraries.
    - [codesnippet:dev/] and [codesnippet:release/] are output folders, not source folders.
    - .beanstalk_manifest files are build-system metadata used for safe stale-output cleanup.
    - [codesnippet:src/#page.bst] is the default HTML page entry and includes a starter page.
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

    [styled_table:
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

    [styled_table:
        [header: [: Form] [: Meaning] ]

        [data:
            [: [codesnippet, "#[...]"]]
            [: A top-level const template. It must fold at compile time and is emitted as static HTML.]
        ]

        [data:
            [: [codesnippet, "[...]"]]
            [: A top-level runtime template. It is evaluated by the module's implicit start function and inserted into the page at runtime.]
        ]
    ]

    Example:

    [codeblock, $code("bst"):
        import @styles/docs {navbar, section, theme_head}

        page_title #= "Home"
        page_description #= "Welcome page."
        page_head #= theme_head

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
    # Visibility and compile-time constants

    [styled_table:
        [header: [: Declaration kind] [: Import behavior] ]

        [data:
            [: Runtime top-level binding]
            [: Entry-file start-body code only. It is not importable.]
        ]

        [data:
            [: Compile-time constant]
            [: Written as [codesnippet:name #= value] or [codesnippet:name #Type = value]. It is importable as a declaration.]
        ]

        [data:
            [: Function]
            [: Importable by other files in the same module by default.]
        ]

        [data:
            [: Struct, choice, or type alias]
            [: Importable by other files in the same module by default.]
        ]

        [data:
            [: Top-level template]
            [: [codesnippet, "#[...]"] is not a normal exported value. It is an entry-file-only const page fragment.]
        ]
    ]

    [codesnippet:#] no longer means public visibility. It marks compile-time constant bindings. Cross-module public visibility is controlled by [codesnippet:#mod.bst] facades.

    Example helper file:

    [codeblock, $code("bst"):
        -- src/components/card.bst

        CardData = |
            title String,
            body String,
        |

        render_card |card CardData| -> String:
            return [:
                <article>
                    <h2>[card.title]</h2>
                    <p>[card.body]</p>
                </article>
            ]
        ;

        helper_note || -> String:
            return "This stays internal unless a facade wraps it."
        ;
    ]

    Example page entry:

    [codeblock, $code("bst"):
        -- src/cards/#page.bst

        import @components/card {CardData, render_card}

        page_title #= "Cards"

        card = CardData("Hello", "This was rendered by a helper function.")

        [:
            [render_card(card)]
        ]
    ]
]

#[section, $markdown:
    # Module boundaries and imports

    Imports are how a page entry reaches helper declarations, module facades, source libraries, core packages, and JavaScript packages.
    This page only covers the project-shape part of that model.
    For full import syntax, source libraries, core packages, JavaScript imports, and builder-provided libraries, see @../libraries (Libraries and modules).

    Project-shape rules to remember:

    - files in the same module can import ordinary sibling source files directly
    - imports that cross into another module root must go through that module's [codesnippet:#mod.bst] facade
    - source-library prefixes come from [codesnippet:library_folders] or from the builder
    - direct imports of [codesnippet:#mod.bst], [codesnippet:#page.bst], and [codesnippet:#config.bst] are rejected
    - sibling [codesnippet:name.bst] / [codesnippet:name/] import-name collisions are rejected during project discovery

    In this shape:

    [codeblock, $code("txt"):
        src/
        ├── #mod.bst
        ├── #page.bst
        ├── components/
        │   └── card.bst
        └── docs/
            └── #page.bst
    ]

    [codesnippet:src/#page.bst] can import [codesnippet:@components/card] directly because they are in the same module.
    [codesnippet:src/docs/#page.bst] is a different module entry, so it imports the root module's public surface through [codesnippet:src/#mod.bst] instead of bypassing into [codesnippet:src/components/card.bst].
]

#[section, $markdown:
    # Example: the documentation site structure

    The Beanstalk documentation website is itself a Beanstalk HTML project.
    Ignoring markdown notes and generated files, its source shape is roughly:

    [codeblock, $code("txt"):
        docs/
        ├── #config.bst
        ├── src/
        │   ├── #mod.bst
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
        ├── lib/
        │   └── local_helpers/
        │       └── #mod.bst
        ├── assets/
        │   └── ...
        ├── dev/
        └── release/
    ]

    The important part is the split:

    - [codesnippet:src/] contains the root page, the root [codesnippet:#mod.bst] facade, and ordinary source files.
    - [codesnippet:#page.bst] files define HTML pages and make their folders module boundaries.
    - [codesnippet:styles/docs.bst] is a normal source file imported by the root facade and then selectively exposed to nested page modules with [codesnippet:export] declarations in [codesnippet:src/#mod.bst].
    - [codesnippet:@html] is provided by the HTML builder.
    - [codesnippet:lib/local_helpers/#mod.bst] would expose a project-local [codesnippet:@local_helpers] source library.
    - [codesnippet:assets/] is a normal project folder for static files such as images.
    - [codesnippet:dev/] and [codesnippet:release/] are output folders, not source folders.
]

#[section, $markdown:
    # Output files

    The build command compiles the project, then writes the builder's output files under the selected output root.

    [styled_table:
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
    # Module facades at a glance

    [codesnippet:#mod.bst] is the public surface for a module root.
    It is not a shared implementation file, and sibling files do not see its declarations automatically.

    Keep facades small and intentional:

    - mark public wrapper functions, type aliases, structs, choices, traits, and constants with [codesnippet:export]
    - re-export grouped symbols with [codesnippet:export import @path { Symbol }] or [codesnippet:export @path { Symbol }]
    - keep unmarked declarations and normal imports private to the facade file
    - import private implementation details inside the facade when a wrapper needs them
    - do not put runtime top-level statements or runtime top-level templates in a facade
    - do not expect a normal import inside [codesnippet:#mod.bst] to become public by itself

    [codeblock, $code("bst"):
        -- src/#mod.bst

        import @core/text {length as private_length}

        private_prefix #= "docs"

        export page_label #= "Documentation"

        export title_length |title String| -> Int:
            return private_length(title)
        ;

        export @components/card {
            CardData as Card,
            render_card,
        }
    ]

    The same facade idea is used by project-local source libraries under [codesnippet:lib/<name>/#mod.bst].
    The detailed library, facade, core package, and JavaScript import guide is @../libraries (Libraries and modules).

    # Future extension points

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
