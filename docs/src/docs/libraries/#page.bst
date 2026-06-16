import @html {center}
import @styles/docs {navbar, title, section, styled_table, header, data, codeblock, theme_head}

page_title #= "Libraries"
page_description #= "How Beanstalk libraries, source libraries, core packages, and JavaScript imports work."
page_head #= theme_head

#[navbar]
#[title, center: LIBRARIES]

#[section, $markdown:
    Libraries are named import surfaces.
    They let a Beanstalk file pull declarations from another place without depending on that place's private implementation layout.

    Beanstalk currently has four library shapes:

    [styled_table:
        [header: [: Library kind] [: Example import] [: What it exposes] ]

        [data:
            [: Source library]
            [: [$html:<code>import @helper {double}</code>] ]
            [: Normal [$html:<code>.bst</code>] source behind a [$html:<code>#mod.bst</code>] facade.]
        ]

        [data:
            [: Builder source library]
            [: [$html:<code>import @html {center}</code>] ]
            [: Builder-provided [$html:<code>.bst</code>] source. The HTML builder provides [$html:<code>@html</code>].]
        ]

        [data:
            [: Core external package]
            [: [$html:<code>import @core/math {sin, PI}</code>] ]
            [: Typed functions and constants registered by the builder. These do not have to come from [$html:<code>.bst</code>] files.]
        ]

        [data:
            [: JavaScript library]
            [: [$html:<code>import @./drawing.js {draw}</code>] ]
            [: An annotated [$html:<code>.js</code>] file parsed into Beanstalk-visible types and functions.]
        ]

        [data:
            [: Builder runtime library]
            [: [$html:<code>import @web/canvas {get_canvas}</code>] ]
            [: A builder-owned JavaScript library bundled with the project builder.]
        ]
    ]

    The current canvas library is exposed as `@web/canvas`.
    This page calls it the canvas library, but examples use the current full import path.
]

#[section, $markdown:
    # Core library pages

    Core libraries are builder-provided packages for common language-adjacent work.
    Most core packages require explicit imports, while the prelude keeps only the smallest always-available surface bare.

    - @./core (Core library overview)
    - @./core/prelude (Prelude)
    - @./core/io (Core IO)
    - @./core/collections (Collections)
    - @./core/math (Math)
    - @./core/text (Text)
    - @./core/random (Random)
    - @./core/time (Time)
]

#[section, $markdown:
    # Import forms

    Imports target a declaration surface, not a raw folder dump.

    [codeblock, $code("bst"):
        -- Import one or more named declarations.
        import @core/math {sin, PI}
        import @html {center}
        import @web/canvas {get_canvas, Canvas2d, context_2d}

        -- Import a whole surface as a namespace.
        import @web/canvas

        -- Import a project-local JavaScript file.
        -- JavaScript imports keep the .js extension.
        import @./drawing.js {draw}

        -- Import a project-local source library.
        import @helper {double}
    ]

    General rules:

    - `@./` resolves relative to the importing file.
    - parent-directory imports with `..` are not supported.
    - `.bst` imports are extensionless.
    - direct `.js` imports keep the `.js` extension.
    - source-library imports use their library prefix directly, such as `@helper` or `@html`.
    - non-relative imports whose first path segment matches a source-library prefix resolve from that library root.
    - other non-relative source imports resolve from the project's configured `entry_root`.
    - core and builder runtime packages use their registered package path, such as `@core/math` or `@web/canvas`.
    - specific symbol imports use grouped syntax. Direct symbol-path imports such as `import @core/math/sin` are rejected.
    - direct imports of special files such as `#mod.bst`, `#page.bst`, and `#config.bst` are rejected.
    - folder imports across module boundaries require a `#mod.bst` facade. Import concrete same-module files directly when no facade is intended.

    Namespace imports create a shallow namespace for ordinary fields.
    Use `math.sin(1.0)` or `canvas.fill_rect(~ctx, 0.0, 0.0, 100.0, 100.0)` for values and `canvas.Canvas2d` in type position, but do not pass the namespace record itself as a value.
    Source-authored receiver methods are called through receiver-call syntax rather than as namespace fields.
]

#[section, $markdown:
    # Source libraries

    Source libraries are normal Beanstalk source roots.
    A project-local source library usually lives under a configured `library_folders` folder.
    Each direct child directory of that folder becomes an import prefix.

    [codeblock, $code("text"):
        project-root/
        ├── #config.bst
        ├── src/
        │   └── #page.bst
        └── lib/
            └── helper/
                ├── #mod.bst
                └── utils.bst
    ]

    In config:

    [codeblock, $code("bst"):
        project = "html"
        library_folders = {
            "lib",
        }
    ]

    The direct child folder `lib/helper` becomes the `@helper` library prefix.
    The library's public surface is controlled by `lib/helper/#mod.bst`.
    Ordinary project source under `entry_root` is not automatically a source-library prefix.
    If a nested page module needs declarations from another module, expose them through that module's `#mod.bst` facade instead of bypassing into private files.

    A minimal library facade can define declarations directly:

    [codeblock, $code("bst"):
        -- lib/helper/#mod.bst

        export double |x Int| -> Int:
            return x * 2
        ;
    ]

    A page can then import from that source library:

    [codeblock, $code("bst"):
        -- src/#page.bst

        import @helper {double}

        value = double(5)
        io.line([: [value]])
    ]

    Facades can also wrap declarations imported from elsewhere.
    This is how a library exposes a stable public name without exposing the implementation path:

    [codeblock, $code("bst"):
        -- lib/helper/#mod.bst

        import @core/math {sin as internal_sin}

        export sine |x Float| -> Float:
            return internal_sin(x)
        ;
    ]

    Facades can also re-export a grouped public surface directly:

    [codeblock, $code("bst"):
        -- lib/helper/#mod.bst

        export @./utils {
            double,
            label as helper_label,
        }
    ]

    Facade rules:

    - outside importers can only see declarations and grouped imports marked with `export` in `#mod.bst`
    - internal implementation files are not public unless the facade wraps or re-exports them intentionally
    - normal imports inside `#mod.bst` are local implementation details
    - exported signatures cannot expose private facade-only types
    - receiver methods do not become public through facade type aliases; export the receiver type's source surface or a wrapper explicitly
    - `#mod.bst` is not shared implementation code for sibling files
    - runtime top-level statements and runtime top-level templates are invalid in `#mod.bst`
    - bare namespace exports such as `export @./utils` and wildcard exports remain deferred
]

#[section, $markdown:
    # Builder source libraries

    Builders can provide source libraries without requiring those files to exist in the user's project.
    The HTML builder provides the `@html` source library used throughout these docs.

    [codeblock, $code("bst"):
        import @html {center}
        import @styles/docs {title}

        #[title, center: HOME]
    ]

    Builder source libraries follow the same source-library idea as project-local libraries:

    - the builder registers a library prefix.
    - imports use that prefix directly.
    - the source files are still Beanstalk source.
    - the project does not own the library's filesystem location.

    This is useful for builder-specific components, templates, styles, or helper declarations that should feel like normal Beanstalk code.
]

#[section, $markdown:
    # Core external packages

    Some libraries are not source files.
    `@core/math` is registered as an external package by the builder.
    It exposes typed functions and constants that the frontend can type-check and the backend can lower directly.

    [codeblock, $code("bst"):
        import @core/math {sin, PI, clamp}

        angle = PI / 2.0
        wave = sin(angle)
        bounded = clamp(wave, 0.0, 1.0)

        io.line([: [bounded]])
    ]

    `@core/math` currently exposes functions such as `sin`, `cos`, `tan`, `atan2`, `log`, `pow`, `sqrt`, `min`, `max`, and `clamp`.
    It also exposes constants such as `PI`, `TAU`, and `E`.

    In the JavaScript backend, these math functions currently lower to JavaScript `Math` expressions rather than emitted helper files.
    Other external packages may lower through generated glue or runtime assets instead.

    The HTML builder currently opts into optional core packages such as:

    - @./core/math (Math)
    - @./core/text (Text)
    - @./core/random (Random)
    - @./core/time (Time)

    Other builders may expose a different core surface.
]

#[section, $markdown:
    # JavaScript libraries

    The HTML builder can import annotated JavaScript files.
    A JavaScript file becomes a Beanstalk import target when the JS external import provider can parse its `@bst.*` comments and supported exports.

    Beanstalk source imports the `.js` file directly:

    [codeblock, $code("bst"):
        -- src/#page.bst

        import @./drawing.js {draw_number}

        value = draw_number()
        io.line([: [value]])
    ]

    The JavaScript file describes its Beanstalk-facing surface with doc comments:

    [codeblock, $code("js"):
        /**
         * @bst.sig draw_number || -> Int
         */
        export function drawNumber() {
            return 7;
        }
    ]

    The Beanstalk-facing name is `draw_number`.
    The JavaScript export name is `drawNumber`.
    They do not need to match.
    The `@bst.sig` annotation binds to the next supported JavaScript export after the annotation.

    Every JavaScript export in a Beanstalk JS library must be annotated with `@bst.sig`.
    Keep private helpers unexported.
]

#[section, $markdown:
    # Writing JavaScript Beanstalk can import

    A Beanstalk-compatible JavaScript library should be small, explicit, and typed from Beanstalk's point of view.

    Supported public export forms:

    [codeblock, $code("js"):
        /**
         * @bst.sig free_function |x Float| -> Float
         */
        export function freeFunction(x) {
            return x * 2.0;
        }

        /**
         * @bst.sig arrow_function |x Int| -> Int
         */
        export const arrowFunction = (x) => {
            return x + 1;
        };
    ]

    Unsupported public export forms include:

    - default exports
    - re-exports such as `export { name }`
    - class exports
    - CommonJS exports such as `module.exports` or `exports.name`
    - `export const` values that are not arrow functions
    - expression-bodied arrow exports

    JavaScript parameters must be plain parameters.
    Rest parameters, destructuring parameters, optional parameters, and default parameter values are rejected.

    The signature subset currently supports:

    [styled_table:
        [header: [: Syntax] [: Meaning] ]

        [data:
            [: [$html:<code>@bst.opaque Canvas2d</code>] ]
            [: Declares an opaque external type. Beanstalk can name it and pass it through external functions, but does not know its internal fields.]
        ]

        [data:
            [: [$html:<code>@bst.sig name |x Float| -&gt; Float</code>] ]
            [: Declares a Beanstalk-facing function named [$html:<code>name</code>] with one parameter and one success return.]
        ]

        [data:
            [: [$html:<code>@bst.sig name || -&gt; Int</code>] ]
            [: Declares a function with no parameters and one success return.]
        ]

        [data:
            [: [$html:<code>@bst.sig name |id String| -&gt; Canvas, Error!</code>] ]
            [: Declares a fallible function. [$html:<code>Error!</code>] must be the final return slot.]
        ]

        [data:
            [: [$html:<code>@bst.sig fill |ctx ~Canvas2d, x Float|</code>] ]
            [: Declares a free function whose first parameter takes mutable access to a [$html:<code>Canvas2d</code>] value.]
        ]
    ]

    Built-in signature types are `Int`, `Float`, `Bool`, `String`, and `Char`.
    Other type names must be declared with `@bst.opaque` before they are used in a signature.

    Current JS signature limitations:

    - no collection types
    - no option types
    - no generic parameters
    - no callback/function parameter types
    - no multi-success returns
    - no `Void`, `None`, `Unit`, or `()` return spelling; omit `->` for no success return
]

#[section, $markdown:
    # Fallible JavaScript functions

    A JavaScript signature that returns `Error!` must return Beanstalk's result wrapper shape.
    The easiest way is to import the registered runtime helpers from `@beanstalk/runtime`.

    [codeblock, $code("js"):
        import { bstOk, bstErr } from "@beanstalk/runtime";

        /**
         * @bst.opaque CanvasElement
         */

        /**
         * @bst.sig get_canvas |id String| -> CanvasElement, Error!
         */
        export function getCanvas(id) {
            const canvas = document.getElementById(id);

            if (!canvas || canvas.tagName !== "CANVAS") {
                return bstErr(404, "Canvas element not found");
            }

            return bstOk(canvas);
        }
    ]

    In Beanstalk, call fallible JS functions the same way as other fallible functions:

    [codeblock, $code("bst"):
        import @./canvas_helpers.js {get_canvas}

        result = get_canvas("game") catch:
            then "missing canvas"
        ;

        io.line(result)
    ]

    JavaScript library files may currently import registered Beanstalk runtime modules with named static imports only.
    Arbitrary JavaScript imports, dynamic imports, default imports, namespace imports, and npm-style dependency graphs are not supported by the JS provider yet.

    If you are adapting an existing JS library, bundle or flatten its dependencies first, then expose a narrow annotated wrapper file to Beanstalk.
]

#[section, $markdown:
    # Mutable JavaScript free functions

    JavaScript-backed external packages expose free functions only.
    Put the opaque value in the first parameter when a function acts on that value, and add `~` when the function mutates it.
    `this` receiver-style signatures are rejected for JS imports and builder-owned external packages.

    [codeblock, $code("js"):
        /**
         * @bst.opaque Canvas2d
         */

        /**
         * @bst.sig set_fill_style |ctx ~Canvas2d, color String|
         */
        export function setFillStyle(ctx, color) {
            ctx.fillStyle = color;
        }

        /**
         * @bst.sig fill_rect |ctx ~Canvas2d, x Float, y Float, width Float, height Float|
         */
        export function fillRect(ctx, x, y, width, height) {
            ctx.fillRect(x, y, width, height);
        }
    ]

    The first JavaScript parameter receives the opaque context object.
    Beanstalk calls these as ordinary free functions:

    [codeblock, $code("bst"):
        import @web/canvas {get_canvas, context_2d, Canvas2d, set_fill_style, fill_rect}

        run |id String| -> String, Error!:
            canvas_ref = get_canvas(id)!
            ctx ~Canvas2d = context_2d(canvas_ref)!
            set_fill_style(~ctx, "red")
            fill_rect(~ctx, 0.0, 0.0, 100.0, 100.0)
            return "canvas_ok"
        ;
    ]

    Source libraries can wrap raw external values in ordinary Beanstalk structs when they want method-style ergonomics.
]

#[section, $markdown:
    # The canvas library

    The HTML builder's canvas library is a built-in JS-backed package exposed as `@web/canvas`.
    It is useful because it shows the whole path from authored JavaScript to Beanstalk imports.

    A page can import and use it like this:

    [codeblock, $code("bst"):
        import @web/canvas

        run |id String| -> String, Error!:
            canvas_ref = canvas.get_canvas(id)!
            ctx ~= canvas.context_2d(canvas_ref)!
            canvas.set_fill_style(~ctx, "red")
            canvas.fill_rect(~ctx, 0.0, 0.0, 100.0, 100.0)
            return "canvas_ok"
        ;

        result = run("game") catch:
            then "error"
        ;

        io.line(result)
    ]

    The higher-level `@html` source library exposes a source-owned `Canvas` wrapper and convenience `get_canvas()` helper for the built-in canvas component:

    [codeblock, $code("bst"):
        import @html {canvas, Canvas, get_canvas}

        #[canvas:
            640
            480
        ]

        draw || -> String, Error!:
            drawing ~= get_canvas()!
            ~drawing.set_fill_style("#202020")
            ~drawing.fill_rect(0.0, 0.0, 640.0, 480.0)
            return "canvas_ok"
        ;
    ]

    The JavaScript source lives inside the HTML builder:

    [codeblock, $code("text"):
        src/projects/html_project/external_libraries/web/canvas/
        ├── canvas.js
        └── mod.rs
    ]

    `canvas.js` is ordinary JavaScript plus Beanstalk annotations.
    It declares opaque types such as `CanvasElement` and `Canvas2d`, plus free functions such as `get_canvas`, `context_2d`, `fill_rect`, `set_fill_style`, `begin_path`, `move_to`, `line_to`, and `stroke`.

    The Rust registration code does the builder-owned part:

    - embeds the `canvas.js` source with `include_str!`
    - parses it with the same JS parser used for project-local `.js` imports
    - registers the package path `@web/canvas`
    - registers its opaque types and free functions in the external package registry
    - records the canvas JS file as a runtime asset that the HTML builder must emit
    - records runtime imports such as `@beanstalk/runtime` so the build can emit the runtime module and import map

    This means `@web/canvas` behaves like a normal import to Beanstalk code, but the project author does not need to keep `canvas.js` in their own project.
]

#[section, $markdown:
    # What the HTML builder emits

    JavaScript imports do more than type-check.
    The build output also needs callable runtime JavaScript.

    For a project-local `@./drawing.js` import, the HTML builder emits:

    - the original JavaScript asset under Beanstalk's generated JS output area
    - generated glue that adapts Beanstalk calls to JS exports
    - runtime modules such as `beanstalk-runtime.js` when the JS file imports registered runtime helpers
    - an import map when generated modules need stable runtime specifiers such as `@beanstalk/runtime`

    Builder runtime libraries such as `@web/canvas` use the same emission path.
    The difference is that the source asset is bundled with the builder instead of being read from the user's project.

    The HTML-Wasm mode currently has a stricter backend support surface.
    JS-backed packages need a Wasm lowering before they can be used by that backend path.
]

#[section, $markdown:
    # Converting an existing JavaScript file

    To make an existing JavaScript file importable from Beanstalk:

    1. Create a small wrapper file rather than exposing the whole library.
    2. Use named ESM exports only.
    3. Keep private helpers unexported.
    4. Add `@bst.opaque` declarations for objects Beanstalk should treat as external handles.
    5. Add one `@bst.sig` comment immediately before every exported function.
    6. Use Beanstalk-facing snake_case names in `@bst.sig`.
    7. Use one plain JavaScript parameter for each Beanstalk ABI parameter.
    8. Return the expected raw value for non-fallible functions.
    9. Return `bstOk(value)` or `bstErr(code, message)` for `Error!` functions.
    10. Import the wrapper from Beanstalk with its `.js` extension.

    Example wrapper shape:

    [codeblock, $code("js"):
        import { bstOk, bstErr } from "@beanstalk/runtime";

        function privateLookup(id) {
            return document.getElementById(id);
        }

        /**
         * @bst.opaque ElementHandle
         */

        /**
         * @bst.sig get_element |id String| -> ElementHandle, Error!
         */
        export function getElement(id) {
            const element = privateLookup(id);

            if (!element) {
                return bstErr(404, "Element not found");
            }

            return bstOk(element);
        }
    ]

    Beanstalk usage:

    [codeblock, $code("bst"):
        import @./dom_helpers.js {get_element, ElementHandle}

        load |id String| -> String, Error!:
            element ElementHandle = get_element(id)!
            return "loaded"
        ;
    ]
]

#[section, $markdown:
    # How builders provide libraries

    Project builders own the library set they expose.
    A builder can combine several mechanisms in one project:

    [styled_table:
        [header: [: Mechanism] [: Builder action] [: Example] ]

        [data:
            [: Mandatory core]
            [: Start from the compiler's required core surface.]
            [: Prelude-level runtime and host pieces required by every builder.]
        ]

        [data:
            [: Optional core package]
            [: Register typed external functions, constants, and backend lowerings.]
            [: [$html:<code>@core/math</code>] registers math functions and constants.]
        ]

        [data:
            [: Source library root]
            [: Register a source-library prefix to a filesystem root.]
            [: [$html:<code>@html</code>] resolves to builder-provided Beanstalk source.]
        ]

        [data:
            [: External import provider]
            [: Register a provider for file extensions such as [$html:<code>.js</code>].]
            [: The HTML builder registers the JS provider for project-local JavaScript imports.]
        ]

        [data:
            [: Builder runtime package]
            [: Register a virtual package plus runtime asset metadata.]
            [: [$html:<code>@web/canvas</code>] is bundled JS parsed and emitted by the HTML builder.]
        ]

        [data:
            [: Config key registry]
            [: Register backend-specific config keys before Stage 0 config validation.]
            [: The HTML builder registers keys such as [$html:<code>html_lang</code>] and [$html:<code>redirect_index_html</code>].]
        ]
    ]

    This separation keeps the compiler frontend generic.
    The frontend needs typed import surfaces, source roots, and provider results.
    The builder decides which surfaces exist and how their compiled output is emitted.
]

#[section, $markdown:
    # Current limits

    Library support is intentionally narrow while the language is still in alpha.

    Current limits to expect:

    - no package manager yet
    - no remote fetching or lockfiles yet
    - no source-library version resolution yet
    - no wildcard imports yet
    - no arbitrary JavaScript dependency graphs through the JS provider yet
    - no direct `.js` imports from `#config.bst`
    - no direct cross-module bypass into another module's private JavaScript file
    - no automatic documentation extraction from `#mod.bst` yet
    - no HIR cache for source libraries yet

    The practical model for now is:

    - use `lib/<name>/#mod.bst` for project-local Beanstalk libraries.
    - use `@core/...`, `@html`, and `@web/canvas` for builder-provided libraries.
    - use small annotated `.js` wrapper files when a project needs browser JavaScript directly.
]
