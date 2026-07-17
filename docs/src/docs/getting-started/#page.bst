import @styles/docs {
    navbar,
    section,
    styled_table,
    header,
    data,
    codeblock,
    language_theme_head,
    doc_pager_next,
}

page_title #= "Getting Started with bean"
page_description #= "How to use the Beanstalk compiler CLI, create a project, run the dev server, and read compiler diagnostics."
page_head #= language_theme_head

#[navbar]

#[section, $md:
    @../ (Documentation) / Getting Started

    # Getting Started with bean

    `bean` is the Beanstalk compiler and project tool.
    You use it from the command-line to create new projects, check them, build them and run the hot-reloading dev server.

    It has a bunch of other tooling that is under rapid developemnt.

    If you are running the compiler from the repository instead of an installed `bean` binary, use `cargo run --` before the same commands:

    [codeblock, $code("bash"):
        cargo run -- dev .
        cargo run -- check .
        cargo run -- build .
    ]
]

#[section, $md:
    ## Commands

    [styled_table:
        [header: [: Command] [: Use] [: Notes] ]

        [data:
            [: [$html:<code>help</code>] ]
            [: Prints the current command list and supported flags.]
            [: Use this when the docs and local compiler may be out of sync.]
        ]

        [data:
            [: [$html:<code>--version</code>] ]
            [: Prints the current compiler/tool version.]
            [: [$html:<code>-v</code>] and [$html:<code>-V</code>] are also accepted.]
        ]

        [data:
            [: [$html:<code>new html</code>] ]
            [: Creates a complete HTML project scaffold.]
            [: Accepts an optional path and [$html:<code>--force</code>]. If path is omitted, bean shows the current directory and asks for confirmation before scaffolding there. It then asks for a project name, a .gitignore choice, and creates the full default scaffold including [$html:<code>src/#page.bst</code>], [$html:<code>lib/</code>], output manifests, and an optional .gitignore.]
        ]

        [data:
            [: [$html:<code>check .</code>] ]
            [: Runs project discovery, frontend compilation, type checking, HIR lowering, and borrow validation without writing build artifacts.]
            [: Add [$html:<code>--terse</code>] for compact one-line diagnostics.]
        ]

        [data:
            [: [$html:<code>dev .</code>] ]
            [: Builds the project, starts a local dev server, watches files, and reloads the browser when sources change.]
            [: This is the main day-to-day command for working on HTML projects.]
        ]

        [data:
            [: [$html:<code>build .</code>] ]
            [: Builds the project and writes output files.]
            [: Without [$html:<code>--release</code>], directory projects write to the configured dev output folder. With [$html:<code>--release</code>], they write to the configured release output folder.]
        ]

        [data:
            [: [$html:<code>tests</code>] ]
            [: Runs the compiler integration test suite. (developer tool)]
            [: Mainly useful when working on the compiler itself. Use [$html:<code>--backend html</code>] or [$html:<code>--backend html_wasm</code>] to focus one backend profile.]
        ]
    ]
]

#[section, $md:
    ## Current flags

    [styled_table:
        [header: [: Flag] [: Applies to] [: Effect] ]

        [data:
            [: [$html:<code>--release</code>] ]
            [: [$html:<code>build</code>], [$html:<code>dev</code>] ]
            [: Selects the release build profile and release output folder where supported.]
        ]

        [data:
            [: [$html:<code>--html-wasm</code>] ]
            [: HTML builds]
            [: Uses the experimental HTML-Wasm backend path. The JavaScript HTML backend is the current Alpha target.]
        ]

        [data:
            [: [$html:<code>--terse</code>] ]
            [: [$html:<code>check</code>] only]
            [: Prints compact diagnostics for tools, logs, and LLM workflows.]
        ]

        [data:
            [: [$html:<code>--force</code>] ]
            [: [$html:<code>new html</code>] only]
            [: Allows replacing existing scaffold-owned files after a confirmation warning. Never overwrites .gitignore or unrelated files.]
        ]

        [data:
            [: [$html:<code>--host</code>], [$html:<code>--port</code>], [$html:<code>--poll-interval-ms</code>] ]
            [: [$html:<code>dev</code>] only]
            [: Configures the local dev server. Defaults are [$html:<code>127.0.0.1</code>], [$html:<code>6342</code>], and [$html:<code>300</code>].]
        ]
    ]
]

#[section, $md:
    ## Create a new HTML project

    The scaffold creates a complete default project including config, a starter page, package folder, output folders with build manifests, and an optional .gitignore.
    The HTML project builder itself has more rules than this page needs to cover; see @../project-structure (the project structure guide) for page layout, routing, assets, fragments, and generated output details.

    Start a new project:

    [codeblock, $code("bash"):
        bean new html
        bean new html my-site
        bean new html ~/projects/my-site
    ]

    If you omit the path, bean asks for confirmation in the current directory:

    [codeblock, $code("bash"):
        No project path specified. Current directory: /your/path
        Create the new HTML project in this directory? y/N:
    ]

    If the target directory already exists, you are given three choices:

    [codeblock, $code("bash"):
        Target directory already exists:
        /path/to/target

        What do you want to do?
          1. Create the project inside this directory
          2. Create a new child folder for the project inside this directory
          3. Cancel

        Choose 1/2/3:
    ]

    If the path contains directories that do not exist, bean asks before creating them:

    [codeblock, $code("bash"):
        The project target contains directories that do not exist:
        /path/to/new/site

        Create the missing directories and scaffold the project there? y/N:
    ]

    After the final directory is chosen, bean asks for the project name:

    [codeblock, $code("bash"):
        Project name (press Enter to use project directory name):
    ]

    Then it asks whether to add a .gitignore with Beanstalk defaults:

    [codeblock, $code("bash"):
        Add a .gitignore with Beanstalk defaults? Y/n:
    ]

    If .gitignore already exists, bean asks whether to append missing defaults instead of overwriting.

    ### Generated project structure

    A scaffolded project looks like this:

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

    - `lib/` is created empty by default for project-local source-backed packages.
    - `dev/` and `release/` are output folders.
    - .beanstalk_manifest files are build-system metadata for safe stale-output cleanup.
    - `src/#page.bst` is the conventional module root with a starter page. The filename is cosmetic.

    ### Using --force

    --force allows replacing scaffold-owned files when they already exist:

    [codeblock, $code("bash"):
        bean new html my-site --force
    ]

    It still asks for a second confirmation and never overwrites .gitignore or unrelated files.

    After creation, check and run the project:

    [codeblock, $code("bash"):
        cd my-site
        bean check .
        bean dev .
    ]
]

#[section, $md:
    ## Run the dev server

    `dev` is currently the most useful command for normal Beanstalk project work.
    It builds the project, serves the generated files locally, watches for source changes, and sends reload events to the browser.

    [codeblock, $code("bash"):
        bean dev .
    ]

    By default, open:

    [codeblock, $code("text"):
        http://127.0.0.1:6342/
    ]

    Useful dev-server options:

    [codeblock, $code("bash"):
        bean dev . --port 8080
        bean dev . --host 0.0.0.0
        bean dev . --poll-interval-ms 100
    ]

    When a build fails, fix the reported error and save again.
    The dev server is designed around fast feedback rather than a manual rebuild loop.
]

#[section, $md:
    ## Build a project

    Use `build` when you want files written to disk without running the dev server.

    [codeblock, $code("bash"):
        bean build .
    ]

    For a release build:

    [codeblock, $code("bash"):
        bean build . --release
    ]

    For HTML projects, generated files are written under the configured output folder.
    The default scaffold currently uses `dev` for development output and `release` for release output.
]

#[section, $md:
    ## Check without building

    `check` is the fastest correctness command for editor loops and CI-style validation.
    It does not write output artifacts, but it still runs the shared frontend diagnostics used by all backends.

    [codeblock, $code("bash"):
        bean check .
    ]

    Use terse mode when another tool will read the output:

    [codeblock, $code("bash"):
        bean check . --terse
    ]

    Terse diagnostics have a stable compact shape:

    [codeblock, $code("text"):
        E|type|src/#page.bst|4:12|Expected Bool, found Int|help=Use a Bool expression here.
        W|identifier_naming_convention|src/button.bst|1:1|Import alias case mismatch
    ]
]

#[section, $md:
    ## Reading compiler errors

    Beanstalk uses the same formatted diagnostic system across the compiler front end and backend builders.
    That means syntax errors, type errors, config errors, file errors, borrow-checker errors, dev-server errors, and backend errors use the same basic shape.

    A normal diagnostic contains:

    - the error kind
    - the main message
    - the source file path, line, and column
    - the source line when available
    - a caret underline for the offending span
    - guidance lines when the compiler has a useful suggestion

    Example shape:

    [codeblock, $code("text"):
        Type Error (ಠ_ಠ) 🔥
        Declaration 'count' has incompatible initializer type. Expected 'Int', but found 'String'.

          --> src/#page.bst:6:17
            |
          6 | count Int = "wrong"
            |             ^^^^^^^
          Use an Int value here, or change the declaration type.
    ]

    Read diagnostics from top to bottom:

    1. The kind tells you which part of the compiler rejected the program.
    2. The message tells you the actual rule that failed.
    3. The file/line/column points to the primary source location.
    4. The caret shows the token or expression the compiler is complaining about.
    5. The guidance lines are suggestions, not a separate error.
]

#[section, $md:
    ## Error kinds

    [styled_table:
        [header: [: Kind] [: Meaning] [: Usual fix] ]

        [data:
            [: Syntax Error]
            [: The source text could not be parsed as Beanstalk syntax.]
            [: Check punctuation, block delimiters, template brackets, imports, and declaration shape.]
        ]

        [data:
            [: Type Error]
            [: The expression type does not match the expected type.]
            [: Check annotations, return types, function arguments, collection element types, and contextual casts.]
        ]

        [data:
            [: Language Rule Error]
            [: The syntax is valid, but it violates a language rule.]
            [: Check things like mutability, import visibility, unsupported feature surfaces, duplicate names, or invalid top-level code.]
        ]

        [data:
            [: Missing File or Directory]
            [: The project references a file, directory, import path, or output path that cannot be resolved.]
            [: Check spelling, relative paths, [$html:<code>entry_root</code>] and project config.]
        ]

        [data:
            [: Malformed Config]
            [: The project config exists but contains unsupported, missing, or invalid settings.]
            [: Check [$html:<code>config.bst</code>] and backend-specific config rules.]
        ]

        [data:
            [: Borrow Checker Violation]
            [: The program violates Beanstalk's access and ownership rules.]
            [: Check mutable/exclusive access, later uses after possible moves, and overlapping borrows.]
        ]

        [data:
            [: Dev Server Issue]
            [: The local server could not start, watch, build, or serve the project correctly.]
            [: Check port conflicts, host binding, project path, and file permissions.]
        ]

        [data:
            [: Compiler Bug / HIR / LIR / WASM Generation Bug]
            [: The compiler hit an internal invariant or unsupported backend path.]
            [: Reduce the input if practical and report it. User code should receive structured errors instead of compiler bugs.]
        ]
    ]
]

#[section, $md:
    ## Warnings and future suggestions

    Warnings are for code that can still compile but deserves attention.
    Current warnings include things like malformed CSS/HTML template content, large tracked assets, naming convention issues, and import alias case mismatches.

    Beanstalk deliberately separates correctness from style pressure.
    The future `suggest` command is planned to become the pedantic style and cleanup pass: closer to a full Clippy-style assistant for Beanstalk code.
    That is where noisy suggestions such as unused variables, unused imports, unused declarations, and more specific style improvements should live.

    The intended workflow will be:

    [codeblock, $code("bash"):
        bean check .
        bean suggest .
        bean fmt .
    ]

    For now, use `check` and the compiler's normal warnings as the practical baseline.
]

#[section, $md:
    ## Planned bean extensions

    `bean` is expected to become the main Beanstalk project tool, not just a thin compiler launcher.
    Planned additions include:

    - a package manager for Beanstalk packages and project dependencies
    - a formatter for stable project-wide formatting
    - `suggest`, a pedantic style and cleanup command to use alongside `check`
    - richer `new` project options, including more complete config templates
    - stronger project maintenance commands as the build system matures

    The short-term rule is simple: `dev` for fast browser feedback, `check` for correctness, and `build` when you want output files.
]

#[section:
    [doc_pager_next:
        [$insert("next_path"):../language-overview/]
        [$insert("next_title"):Language basics]
    ]
]
