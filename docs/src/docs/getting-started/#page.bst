import @html {center}
import @styles/docs {navbar, title, section, styled_table, header, data, codeblock, codesnippet, theme_head}

# page_title = "Getting Started with bean"
# page_description = "How to use the Beanstalk compiler CLI, create a project, run the dev server, and read compiler diagnostics."
# page_head = theme_head

#[navbar]
#[title, center: GETTING STARTED WITH BEAN]

#[section, $markdown:
    [codesnippet:bean] is the Beanstalk compiler and project tool.
    It is the command-line entry point launched by the compiler binary, and it will eventually grow into the Cargo-like tool for Beanstalk projects.

    The tooling is still early.
    Command names, generated project structure, config options, and backend flags may change at this stage in development.
    
    it's main purpose right now is to: create a project, check it, build it, and run the hot-reloading dev server.

    If you are running the compiler from the repository instead of an installed [codesnippet:bean] binary, use [codesnippet:cargo run --] before the same commands:

    [codeblock, $code("bash"):
        cargo run -- dev .
        cargo run -- check .
        cargo run -- build .
    ]
]

#[section, $markdown:
    # Current commands

    [styled_table:
        [header: [: Command] [: Use] [: Notes] ]

        [data:
            [: [codesnippet:bean help] ]
            [: Prints the current command list and supported flags.]
            [: Use this when the docs and local compiler may be out of sync.]
        ]

        [data:
            [: [codesnippet:bean --version] ]
            [: Prints the current compiler/tool version.]
            [: [codesnippet:-v] and [codesnippet:-V] are also accepted.]
        ]

        [data:
            [: [codesnippet:bean new html] ]
            [: Creates a minimal HTML project scaffold.]
            [: Currently prompts for a project path and project name. More project options are planned.]
        ]

        [data:
            [: [codesnippet:bean check path] ]
            [: Runs project discovery, frontend compilation, type checking, HIR lowering, and borrow validation without writing build artifacts.]
            [: Add [codesnippet:--terse] for compact one-line diagnostics.]
        ]

        [data:
            [: [codesnippet:bean dev path] ]
            [: Builds the project, starts a local dev server, watches files, and reloads the browser when sources change.]
            [: This is the main day-to-day command for working on HTML projects.]
        ]

        [data:
            [: [codesnippet:bean build path] ]
            [: Builds the project and writes output files.]
            [: Without [codesnippet:--release], directory projects write to the configured dev output folder. With [codesnippet:--release], they write to the configured release output folder.]
        ]

        [data:
            [: [codesnippet:bean tests] ]
            [: Runs the compiler integration test suite.]
            [: Mainly useful when working on the compiler itself. Use [codesnippet:--backend html] or [codesnippet:--backend html_wasm] to focus one backend profile.]
        ]
    ]
]

#[section, $markdown:
    # Current flags

    [styled_table:
        [header: [: Flag] [: Applies to] [: Effect] ]

        [data:
            [: [codesnippet:--release] ]
            [: [codesnippet:build], [codesnippet:dev] ]
            [: Selects the release build profile and release output folder where supported.]
        ]

        [data:
            [: [codesnippet:--hide-warnings] ]
            [: Build-style commands]
            [: Hides warning output. Use sparingly while developing.]
        ]

        [data:
            [: [codesnippet:--hide-timers] ]
            [: Build-style commands]
            [: Hides timing output.]
        ]

        [data:
            [: [codesnippet:--html-wasm] ]
            [: HTML builds]
            [: Uses the experimental HTML-Wasm backend path. The JavaScript HTML backend is the current Alpha target.]
        ]

        [data:
            [: [codesnippet:--terse] ]
            [: [codesnippet:check] only]
            [: Prints compact diagnostics for tools, logs, and LLM workflows.]
        ]

        [data:
            [: [codesnippet:--host], [codesnippet:--port], [codesnippet:--poll-interval-ms] ]
            [: [codesnippet:dev] only]
            [: Configures the local dev server. Defaults are [codesnippet:127.0.0.1], [codesnippet:6342], and [codesnippet:300].]
        ]
    ]
]

#[section, $markdown:
    # Create a new HTML project

    The current project scaffold is intentionally small.
    It creates the project config and output folders, then you can add a page entry file.
    The HTML project builder itself has more rules than this page needs to cover; see @../html-project-builder (the HTML project builder guide) for page layout, routing, assets, fragments, and generated output details.

    Start a new project:

    [codeblock, $code("bash"):
        bean new html
    ]

    The command asks for:

    [codeblock, $code("text"):
        Enter project path:
        Project name:
    ]

    After the project exists, add an entry page if the scaffold has not created one yet:

    [codeblock, $code("bash"):
        cd path/to/your-project
        mkdir -p src
        nano src/#page.bst
    ]

    A minimal first page can be:

    [codeblock, $code("bst"):
        # page_title = "Hello Beanstalk"

        [$markdown:
            # Hello from Beanstalk

            Edit this file and save to trigger hot reload.
        ]
    ]

    Then check it:

    [codeblock, $code("bash"):
        bean check .
    ]
]

#[section, $markdown:
    # Run the dev server

    [codesnippet:dev] is currently the most useful command for normal Beanstalk project work.
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

#[section, $markdown:
    # Build a project

    Use [codesnippet:build] when you want files written to disk without running the dev server.

    [codeblock, $code("bash"):
        bean build .
    ]

    For a release build:

    [codeblock, $code("bash"):
        bean build . --release
    ]

    For HTML projects, generated files are written under the configured output folder.
    The default scaffold currently uses [codesnippet:dev] for development output and [codesnippet:release] for release output.
]

#[section, $markdown:
    # Check without building

    [codesnippet:check] is the fastest correctness command for editor loops and CI-style validation.
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

#[section, $markdown:
    # Reading compiler errors

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

#[section, $markdown:
    # Error kinds

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
            [: Check spelling, relative paths, root folders, and project config.]
        ]

        [data:
            [: Malformed Config]
            [: The project config exists but contains unsupported, missing, or invalid settings.]
            [: Check [codesnippet:#config.bst] and backend-specific config rules.]
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

#[section, $markdown:
    # Warnings and future suggestions

    Warnings are for code that can still compile but deserves attention.
    Current warnings include things like malformed CSS/HTML template content, large tracked assets, naming convention issues, and import alias case mismatches.

    Beanstalk deliberately separates correctness from style pressure.
    The future [codesnippet:suggest] command is planned to become the pedantic style and cleanup pass: closer to a full Clippy-style assistant for Beanstalk code.
    That is where noisy suggestions such as unused variables, unused imports, unused declarations, and more specific style improvements should live.

    The intended workflow will be:

    [codeblock, $code("bash"):
        bean check .
        bean suggest .
        bean fmt .
    ]

    For now, use [codesnippet:check] and the compiler's normal warnings as the practical baseline.
]

#[section, $markdown:
    # Planned bean extensions

    [codesnippet:bean] is expected to become the main Beanstalk project tool, not just a thin compiler launcher.
    Planned additions include:

    - a package manager for Beanstalk libraries and project dependencies
    - a formatter for stable project-wide formatting
    - [codesnippet:suggest], a pedantic style and cleanup command to use alongside [codesnippet:check]
    - richer [codesnippet:new] project options, including more complete config templates
    - stronger project maintenance commands as the build system matures

    The short-term rule is simple: [codesnippet:dev] for fast browser feedback, [codesnippet:check] for correctness, and [codesnippet:build] when you want output files.
]
