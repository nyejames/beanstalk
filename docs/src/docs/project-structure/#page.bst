import @html {center}
import @styles/docs {navbar, title, section, palette, styled_table, header, data, codeblock, theme_head}

page_title #= "Projects, Modules and HTML Builder"
page_description #= "How Beanstalk projects are structured, how modules are discovered, and how the HTML project builder emits pages."
page_head #= theme_head

#[navbar]
#[title, center: PROJECTS, MODULES AND HTML BUILDER]

#[section, $md:
    Beanstalk projects are built around a small set of conventions:

    - a project root with a `config.bst` file
    - an entry root, usually `src`
    - module entry files named like `#page.bst`
    - module API boundaries defined by `#mod.bst`
    - normal `.bst` files that provide declarations for module entries
    - builder-owned output folders such as `dev` and `release`

    `#mod.bst`  = module API / visibility boundary
    `#page.bst` = HTML builder entry/configuration convention
    `config.bst`    = build/project configuration convention

    At this stage of development, this is the only supported project structure to rely on.
    The rules are deliberately narrow so the compiler, dev server, and HTML builder can stay predictable while the language is still evolving.
]

#[section, $md:
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

#[section, $md:
    # Project root and config.bst

    A normal HTML project has a `config.bst` file at the project root.
    Config entries are known top-level `#` constants whose values fold at compile time.

    [codeblock, $code("bst"):
        project #= "html"
        entry_root #= "src"
        dev_folder #= "dev"
        output_folder #= "release"
        page_url_style #= "trailing_slash"
        redirect_index_html #= true
        name #= "html_project"
        version #= "0.1.0"
        author #= ""
        license #= "MIT"

        origin #= "/beanstalk"
        html_lang #= "en"
        html_title_postfix #= " | Beanstalk"
        library_folders #= {
            "lib",
        }
    ]

    Config keys are strict about value shape. String settings accept string literals and folded templates, `project` currently accepts only `"html"`, `library_folders` accepts one string or a collection of strings, `template_const_loop_iteration_limit` requires a positive folded `Int` no greater than `1_000_000`, and boolean HTML settings such as `redirect_index_html` require real `true` / `false` values rather than strings.

    Config values must resolve to compile-time constants. Config-key constants may reference earlier compile-time config keys or constants imported from core/builder source libraries. `config.bst` may contain imports, type aliases, structs, and choices as support declarations, but every authored value-producing declaration must still be a known config key. Plain top-level bindings are runtime/start-body syntax and are rejected in config. Config-local helper constants, traits, conformance metadata, functions, calls, runtime statements, project-local imports, relative config imports, and `["#[...]"]` page fragments are rejected. Const-record field projection is supported when the `#=` config expression fully folds.

    Important settings:

    [styled_table:
        [header: [: Setting] [: Meaning] ]

        [data:
            [: [$html:<code>project</code>]]
            [: Selects the project type. For the documentation site and current web projects, use [$html:<code>"html"</code>].]
        ]

        [data:
            [: [$html:<code>entry_root</code>]]
            [: The source folder that is searched for module entry files. Usually [$html:<code>"src"</code>].]
        ]

        [data:
            [: [$html:<code>dev_folder</code>]]
            [: Where development builds are written. The dev server serves from here.]
        ]

        [data:
            [: [$html:<code>output_folder</code>]]
            [: Where release builds are written when [$html:<code>--release</code>] is used.]
        ]

        [data:
            [: [$html:<code>library_folders</code>]]
            [: Configures source-library scan roots. Each direct child directory of a scan root becomes an import prefix. `/lib` is the default when omitted.]
        ]

        [data:
            [: [$html:<code>template_const_loop_iteration_limit</code>]]
            [: Sets the per-loop compile-time template expansion guard. The default is [$html:<code>10_000</code>].]
        ]

        [data:
            [: HTML-specific settings]
            [: Settings such as [$html:<code>origin</code>], [$html:<code>html_lang</code>], [$html:<code>html_title_postfix</code>], [$html:<code>page_url_style</code>], and [$html:<code>redirect_index_html</code>] are interpreted by the HTML builder.]
        ]
    ]

    Config is loaded before module discovery.
    That means [$html:<code>entry_root</code>] and [$html:<code>library_folders</code>] shape how the project is searched and how imports resolve.
]

#[section, $md:
    # Default project scaffold

    The `bean new html` command generates a complete default project:

    [codeblock, $code("text"):
        project-root/
        ├── config.bst
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

    - `lib/` is created empty by default. It is the conventional place for project-local source libraries.
    - `dev/` and `release/` are output folders, not source folders.
    - .beanstalk_manifest files are build-system metadata used for safe stale-output cleanup.
    - `src/#page.bst` is the default HTML page entry and includes a starter page.
]

#[section, $md:
    # Module entry files

    A module entry file is a Beanstalk file whose file name starts with `#`.
    In directory projects, the compiler searches the configured entry root for `#*.bst` files.
    `config.bst` is not module source.

    Examples:

    [codeblock, $code("txt"):
        src/#page.bst              -> module entry
        src/docs/#page.bst         -> module entry
        src/docs/basics/#page.bst  -> module entry
        src/styles/docs.bst        -> normal source file
        config.bst                 -> project config, not a module entry
    ]

    Only module entry files have top-level runtime execution.
    The compiler treats the entry file's top-level runtime code as an implicit `start` function for that module.

    Normal source files can contain imports, functions, structs, choices, type aliases, and constants.
    They should not rely on top-level runtime statements being executed by themselves.
]

#[section, $md:
    # Builder-owned meaning of # file names

    The compiler discovers `#*.bst` files as module entries.
    The project builder decides what those entries mean.

    The HTML project builder currently treats them as page entries.
    The file name and folder decide the output route.

    [styled_table:
        [header: [: Entry file] [: HTML output path] ]

        [data:
            [: [$html:<code>src/#page.bst</code>]]
            [: [$html:<code>index.html</code>]]
        ]

        [data:
            [: [$html:<code>src/docs/#page.bst</code>]]
            [: [$html:<code>docs/index.html</code>]]
        ]

        [data:
            [: [$html:<code>src/docs/basics/#page.bst</code>]]
            [: [$html:<code>docs/basics/index.html</code>]]
        ]

        [data:
            [: [$html:<code>src/#about.bst</code>]]
            [: [$html:<code>about/index.html</code>]]
        ]
    ]

    This gives pages stable folder-backed routes.
    A `#page.bst` file means "the index page for this folder".
    Other `#name.bst` files become named routes under the same folder.
]

#[section, $md:
    # Top-level templates in page entries

    Top-level templates are special.
    In the HTML project builder, direct top-level templates in a module entry file become page fragments.

    There are two forms:

    [styled_table:
        [header: [: Form] [: Meaning] ]

        [data:
            [: [$html:<code>["#[...]"]</code>]]
            [: A top-level const template. It must fold at compile time and is emitted as static HTML.]
        ]

        [data:
            [: [$html:<code>["[...]"]</code>]]
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

        #[section, $md:
            # Welcome
            This part is static.
        ]

        name = "Beanstalk"

        -- Runtime fragments are produced by the implicit start function.
        [section, $md:
            Runtime hello from [name].
        ]
    ]

    The HTML builder preserves source order between static and runtime fragments.
    Static fragments are emitted immediately.
    Runtime fragments use generated slots in the HTML document, then the embedded JavaScript calls the module start function and fills those slots.

    Do not put page-producing top-level templates in helper files.
    Helper files should export reusable declarations, then page entries should import and use them.
]

#[section, $md:
    # Visibility and compile-time constants

    [styled_table:
        [header: [: Declaration kind] [: Import behavior] ]

        [data:
            [: Runtime top-level binding]
            [: Entry-file start-body code only. It is not importable.]
        ]

        [data:
            [: Compile-time constant]
            [: Written as [$html:<code>name #= value</code>] or [$html:<code>name #Type = value</code>]. It is importable as a declaration.]
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
            [: [$html:<code>["#[...]"]</code>] is not a normal exported value. It is an entry-file-only const page fragment.]
        ]
    ]

    `#` no longer means public visibility. It marks compile-time constant bindings. Cross-module public visibility is controlled by `#mod.bst` facades.

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

#[section, $md:
    # Module boundaries and imports

    Imports are how a page entry reaches helper declarations, module facades, source libraries, core packages, and JavaScript packages.
    This page only covers the project-shape part of that model.
    For full import syntax, source libraries, core packages, JavaScript imports, and builder-provided libraries, see @../libraries (Libraries and modules).

    Project-shape rules to remember:

    - files in the same module can import ordinary sibling source files directly
    - imports that cross into another module root must go through that module's `#mod.bst` facade
    - source-library prefixes come from `library_folders` or from the builder
    - direct imports of `#mod.bst`, `#page.bst`, and `config.bst` are rejected
    - sibling `name.bst` / `name/` import-name collisions are rejected during project discovery

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

    `src/#page.bst` can import `@components/card` directly because they are in the same module.
    `src/docs/#page.bst` is a different module entry, so it imports the root module's public surface through `src/#mod.bst` instead of bypassing into `src/components/card.bst`.
]

#[section, $md:
    # Example: the documentation site structure

    The Beanstalk documentation website is itself a Beanstalk HTML project.
    Ignoring markdown notes and generated files, its source shape is roughly:

    [codeblock, $code("txt"):
        docs/
        ├── config.bst
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

    - `src/` contains the root page, the root `#mod.bst` facade, and ordinary source files.
    - `#page.bst` files define HTML pages and make their folders module boundaries.
    - `styles/docs.bst` is a normal source file imported by the root facade and then selectively exposed to nested page modules with `export` declarations in `src/#mod.bst`.
    - `@html` is provided by the HTML builder.
    - `lib/local_helpers/#mod.bst` would expose a project-local `@local_helpers` source library.
    - `assets/` is a normal project folder for static files such as images.
    - `dev/` and `release/` are output folders, not source folders.
]

#[section, $md:
    # Output files

    The build command compiles the project, then writes the builder's output files under the selected output root.

    [styled_table:
        [header: [: Build mode] [: Output root] ]

        [data:
            [: Default / dev]
            [: [$html:<code>#dev_folder</code>], usually [$html:<code>dev</code>].]
        ]

        [data:
            [: [$html:<code>--release</code>]]
            [: [$html:<code>#output_folder</code>], usually [$html:<code>release</code>].]
        ]
    ]

    In the default JS-backed HTML mode, each page module produces an HTML file.
    The generated JavaScript is embedded into that HTML file.

    In the experimental `--html-wasm` mode, the builder can instead emit a route folder containing:

    - `index.html`
    - `page.js`
    - `page.wasm`

    The HTML builder owns stale-output cleanup for generated HTML artifacts.
    Generated output folders should be treated as build artifacts, not source.
]

#[section, $md:
    # Module facades at a glance

    `#mod.bst` is the public surface for a module root.
    It is not a shared implementation file, and sibling files do not see its declarations automatically.

    Keep facades small and intentional:

    - mark public wrapper functions, type aliases, structs, choices, traits, and constants with `export`
    - re-export grouped symbols with `export import @path { Symbol }` or `export @path { Symbol }`
    - keep unmarked declarations and normal imports private to the facade file
    - import private implementation details inside the facade when a wrapper needs them
    - do not put runtime top-level statements or runtime top-level templates in a facade
    - do not expect a normal import inside `#mod.bst` to become public by itself

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

    The same facade idea is used by project-local source libraries under `lib/<name>/#mod.bst`.
    The detailed library, facade, core package, and JavaScript import guide is @../libraries (Libraries and modules).

    # Future extension points

    ## #test.bst test modules

    A future `#test.bst` file can hold project tests.
    Normal builds would ignore it.
    A planned test command could discover and run those files, similar to how `cargo test` discovers Rust tests.

    This is separate from the current compiler-internal integration test runner.

    ## $doc documentation templates

    The `$doc` style directive can eventually make documentation part of Beanstalk source.
    Top-level or nested doc templates could be stripped from normal output code, then collected by a documentation command.

    This would make documentation generation feel closer to Rust doc comments, but with Beanstalk's template system instead of comment syntax.
]
