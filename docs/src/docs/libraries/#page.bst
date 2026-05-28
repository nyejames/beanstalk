import @html {center}
import @styles/docs {navbar, title, section, styled_table, header, data, codeblock, codesnippet, theme_head}

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
            [: [codesnippet:import @helper/utils {double}] ]
            [: Normal [codesnippet:.bst] source behind a [codesnippet:#mod.bst] facade.]
        ]

        [data:
            [: Builder source library]
            [: [codesnippet:import @html {center}] ]
            [: Builder-provided [codesnippet:.bst] source. The HTML builder provides [codesnippet:@html].]
        ]

        [data:
            [: Core external package]
            [: [codesnippet:import @core/math {sin, PI}] ]
            [: Typed functions and constants registered by the builder. These do not have to come from [codesnippet:.bst] files.]
        ]

        [data:
            [: JavaScript library]
            [: [codesnippet:import @./drawing.js {draw}] ]
            [: An annotated [codesnippet:.js] file parsed into Beanstalk-visible types and functions.]
        ]

        [data:
            [: Builder runtime library]
            [: [codesnippet:import @web/canvas {get_canvas}] ]
            [: A builder-owned JavaScript library bundled with the project builder.]
        ]
    ]

    The current canvas library is exposed as [codesnippet:@web/canvas].
    This page calls it the canvas library, but examples use the current full import path.
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
        import @helper/utils {double}
    ]

    General rules:

    - [codesnippet:@./] resolves relative to the importing file.
    - parent-directory imports with [codesnippet:..] are not supported.
    - [codesnippet:.bst] imports are extensionless.
    - direct [codesnippet:.js] imports keep the [codesnippet:.js] extension.
    - source-library imports use their library prefix directly, such as [codesnippet:@helper] or [codesnippet:@html].
    - non-relative imports whose first path segment matches a source-library prefix resolve from that library root.
    - other non-relative source imports resolve from the project's configured [codesnippet:entry_root].
    - core and builder runtime packages use their registered package path, such as [codesnippet:@core/math] or [codesnippet:@web/canvas].
    - specific symbol imports use grouped syntax. Direct symbol-path imports such as [codesnippet:import @core/math/sin] are rejected.
    - direct imports of special files such as [codesnippet:#mod.bst], [codesnippet:#page.bst], and [codesnippet:#config.bst] are rejected.
    - folder imports across module boundaries require a [codesnippet:#mod.bst] facade. Import concrete same-module files directly when no facade is intended.

    Namespace imports create a shallow namespace for ordinary fields.
    Use [codesnippet:math.sin(1.0)] for values and [codesnippet:canvas.Canvas2d] in type position, but do not pass the namespace record itself as a value.
    Receiver methods are called through receiver-call syntax rather than as namespace fields.
]

#[section, $markdown:
    # Source libraries

    Source libraries are normal Beanstalk source roots.
    A project-local source library usually lives under a configured [codesnippet:library_folders] folder.
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

    The direct child folder [codesnippet:lib/helper] becomes the [codesnippet:@helper] library prefix.
    The library's public surface is controlled by [codesnippet:lib/helper/#mod.bst].
    Ordinary project source under [codesnippet:entry_root] is not automatically a source-library prefix.
    If a nested page module needs declarations from another module, expose them through that module's [codesnippet:#mod.bst] facade instead of bypassing into private files.

    A minimal library facade can define declarations directly:

    [codeblock, $code("bst"):
        -- lib/helper/#mod.bst

        double |x Int| -> Int:
            return x * 2
        ;
    ]

    A page can then import from that source library:

    [codeblock, $code("bst"):
        -- src/#page.bst

        import @helper {double}

        value = double(5)
        io(value)
    ]

    Facades can also wrap declarations imported from elsewhere.
    This is how a library exposes a stable public name without exposing the implementation path:

    [codeblock, $code("bst"):
        -- lib/helper/#mod.bst

        import @core/math {sin as internal_sin}

        sine |x Float| -> Float:
            return internal_sin(x)
        ;
    ]

    Facade rules:

    - outside importers can only see declarations authored by [codesnippet:#mod.bst]
    - internal implementation files are not public unless the facade wraps them with real declarations
    - normal imports inside [codesnippet:#mod.bst] are local implementation details
    - [codesnippet:#mod.bst] is not shared implementation code for sibling files
    - runtime top-level statements and runtime top-level templates are invalid in [codesnippet:#mod.bst]

    This is why facade examples use wrappers and aliases instead of direct re-export syntax.
    Direct facade re-export syntax is not part of the current Alpha surface.
]

#[section, $markdown:
    # Builder source libraries

    Builders can provide source libraries without requiring those files to exist in the user's project.
    The HTML builder provides the [codesnippet:@html] source library used throughout these docs.

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
    [codesnippet:@core/math] is registered as an external package by the builder.
    It exposes typed functions and constants that the frontend can type-check and the backend can lower directly.

    [codeblock, $code("bst"):
        import @core/math {sin, PI, clamp}

        angle = PI / 2.0
        wave = sin(angle)
        bounded = clamp(wave, 0.0, 1.0)

        io(bounded)
    ]

    [codesnippet:@core/math] currently exposes functions such as [codesnippet:sin], [codesnippet:cos], [codesnippet:tan], [codesnippet:atan2], [codesnippet:log], [codesnippet:pow], [codesnippet:sqrt], [codesnippet:min], [codesnippet:max], and [codesnippet:clamp].
    It also exposes constants such as [codesnippet:PI], [codesnippet:TAU], and [codesnippet:E].

    In the JavaScript backend, these math functions currently lower to JavaScript [codesnippet:Math] expressions rather than emitted helper files.
    Other external packages may lower through generated glue or runtime assets instead.

    The HTML builder currently opts into optional core packages such as:

    - [codesnippet:@core/math]
    - [codesnippet:@core/text]
    - [codesnippet:@core/random]
    - [codesnippet:@core/time]

    Other builders may expose a different core surface.
]

#[section, $markdown:
    # JavaScript libraries

    The HTML builder can import annotated JavaScript files.
    A JavaScript file becomes a Beanstalk import target when the JS external import provider can parse its [codesnippet:@bst.*] comments and supported exports.

    Beanstalk source imports the [codesnippet:.js] file directly:

    [codeblock, $code("bst"):
        -- src/#page.bst

        import @./drawing.js {draw_number}

        value = draw_number()
        io(value)
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

    The Beanstalk-facing name is [codesnippet:draw_number].
    The JavaScript export name is [codesnippet:drawNumber].
    They do not need to match.
    The [codesnippet:@bst.sig] annotation binds to the next supported JavaScript export after the annotation.

    Every JavaScript export in a Beanstalk JS library must be annotated with [codesnippet:@bst.sig].
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
    - re-exports such as [codesnippet:export { name }]
    - class exports
    - CommonJS exports such as [codesnippet:module.exports] or [codesnippet:exports.name]
    - [codesnippet:export const] values that are not arrow functions
    - expression-bodied arrow exports

    JavaScript parameters must be plain parameters.
    Rest parameters, destructuring parameters, optional parameters, and default parameter values are rejected.

    The signature subset currently supports:

    [styled_table:
        [header: [: Syntax] [: Meaning] ]

        [data:
            [: [codesnippet:@bst.opaque Canvas2d] ]
            [: Declares an opaque external type. Beanstalk can name it, pass it, and call receiver methods on it, but does not know its internal fields.]
        ]

        [data:
            [: [codesnippet:@bst.sig name |x Float| -> Float] ]
            [: Declares a Beanstalk-facing function named [codesnippet:name] with one parameter and one success return.]
        ]

        [data:
            [: [codesnippet:@bst.sig name || -> Int] ]
            [: Declares a function with no parameters and one success return.]
        ]

        [data:
            [: [codesnippet:@bst.sig name |id String| -> Canvas, Error!] ]
            [: Declares a fallible function. [codesnippet:Error!] must be the final return slot.]
        ]

        [data:
            [: [codesnippet:@bst.sig fill |this ~Canvas2d, x Float|] ]
            [: Declares a receiver method on a mutable [codesnippet:Canvas2d] receiver.]
        ]
    ]

    Built-in signature types are [codesnippet:Int], [codesnippet:Float], [codesnippet:Bool], [codesnippet:String], and [codesnippet:Char].
    Other type names must be declared with [codesnippet:@bst.opaque] before they are used in a signature.

    Current JS signature limitations:

    - no collection types
    - no option types
    - no generic parameters
    - no callback/function parameter types
    - no multi-success returns
    - no [codesnippet:Void], [codesnippet:None], [codesnippet:Unit], or [codesnippet:()] return spelling; omit [codesnippet:->] for no success return
]

#[section, $markdown:
    # Fallible JavaScript functions

    A JavaScript signature that returns [codesnippet:Error!] must return Beanstalk's result wrapper shape.
    The easiest way is to import the registered runtime helpers from [codesnippet:@beanstalk/runtime].

    [codeblock, $code("js"):
        import { bstOk, bstErr } from "@beanstalk/runtime";

        /**
         * @bst.opaque Canvas
         */

        /**
         * @bst.sig get_canvas |id String| -> Canvas, Error!
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

        io(result)
    ]

    JavaScript library files may currently import registered Beanstalk runtime modules with named static imports only.
    Arbitrary JavaScript imports, dynamic imports, default imports, namespace imports, and npm-style dependency graphs are not supported by the JS provider yet.

    If you are adapting an existing JS library, bundle or flatten its dependencies first, then expose a narrow annotated wrapper file to Beanstalk.
]

#[section, $markdown:
    # Receiver methods from JavaScript

    Use the [codesnippet:this] parameter to declare a JavaScript export as a receiver method.
    [codesnippet:this] must be the first parameter and may appear at most once.
    Add [codesnippet:~] when the method mutates the receiver.

    [codeblock, $code("js"):
        /**
         * @bst.opaque Canvas2d
         */

        /**
         * @bst.sig set_fill_style |this ~Canvas2d, color String|
         */
        export function setFillStyle(ctx, color) {
            ctx.fillStyle = color;
        }

        /**
         * @bst.sig fill_rect |this ~Canvas2d, x Float, y Float, width Float, height Float|
         */
        export function fillRect(ctx, x, y, width, height) {
            ctx.fillRect(x, y, width, height);
        }
    ]

    The first JavaScript parameter is the receiver object.
    Beanstalk calls it with receiver-call syntax:

    [codeblock, $code("bst"):
        import @web/canvas {get_canvas, context_2d, Canvas2d}

        run |id String| -> String, Error!:
            canvas_ref = get_canvas(id)!
            ctx ~Canvas2d = context_2d(canvas_ref)!
            ~ctx.set_fill_style("red")
            ~ctx.fill_rect(0.0, 0.0, 100.0, 100.0)
            return "canvas_ok"
        ;
    ]

    Receiver-method dispatch includes the receiver type, so the same method name can exist for different receiver types.
    The receiver type must still be visible in the importing file.
]

#[section, $markdown:
    # The canvas library

    The HTML builder's canvas library is a built-in JS-backed package exposed as [codesnippet:@web/canvas].
    It is useful because it shows the whole path from authored JavaScript to Beanstalk imports.

    A page can import and use it like this:

    [codeblock, $code("bst"):
        import @web/canvas

        run |id String| -> String, Error!:
            canvas_ref = canvas.get_canvas(id)!
            ctx ~= canvas.context_2d(canvas_ref)!
            ~ctx.set_fill_style("red")
            ~ctx.fill_rect(0.0, 0.0, 100.0, 100.0)
            return "canvas_ok"
        ;

        result = run("game") catch:
            then "error"
        ;

        io(result)
    ]

    The JavaScript source lives inside the HTML builder:

    [codeblock, $code("text"):
        src/projects/html_project/external_libraries/web/canvas/
        ├── canvas.js
        └── mod.rs
    ]

    [codesnippet:canvas.js] is ordinary JavaScript plus Beanstalk annotations.
    It declares opaque types such as [codesnippet:Canvas] and [codesnippet:Canvas2d], free functions such as [codesnippet:get_canvas] and [codesnippet:context_2d], and receiver methods such as [codesnippet:fill_rect], [codesnippet:set_fill_style], [codesnippet:begin_path], [codesnippet:move_to], [codesnippet:line_to], and [codesnippet:stroke].

    The Rust registration code does the builder-owned part:

    - embeds the [codesnippet:canvas.js] source with [codesnippet:include_str!]
    - parses it with the same JS parser used for project-local [codesnippet:.js] imports
    - registers the package path [codesnippet:@web/canvas]
    - registers its opaque types, free functions, and receiver methods in the external package registry
    - records the canvas JS file as a runtime asset that the HTML builder must emit
    - records runtime imports such as [codesnippet:@beanstalk/runtime] so the build can emit the runtime module and import map

    This means [codesnippet:@web/canvas] behaves like a normal import to Beanstalk code, but the project author does not need to keep [codesnippet:canvas.js] in their own project.
]

#[section, $markdown:
    # What the HTML builder emits

    JavaScript imports do more than type-check.
    The build output also needs callable runtime JavaScript.

    For a project-local [codesnippet:@./drawing.js] import, the HTML builder emits:

    - the original JavaScript asset under Beanstalk's generated JS output area
    - generated glue that adapts Beanstalk calls to JS exports
    - runtime modules such as [codesnippet:beanstalk-runtime.js] when the JS file imports registered runtime helpers
    - an import map when generated modules need stable runtime specifiers such as [codesnippet:@beanstalk/runtime]

    Builder runtime libraries such as [codesnippet:@web/canvas] use the same emission path.
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
    4. Add [codesnippet:@bst.opaque] declarations for objects Beanstalk should treat as external handles.
    5. Add one [codesnippet:@bst.sig] comment immediately before every exported function.
    6. Use Beanstalk-facing snake_case names in [codesnippet:@bst.sig].
    7. Use one plain JavaScript parameter for each Beanstalk ABI parameter.
    8. Return the expected raw value for non-fallible functions.
    9. Return [codesnippet:bstOk(value)] or [codesnippet:bstErr(code, message)] for [codesnippet:Error!] functions.
    10. Import the wrapper from Beanstalk with its [codesnippet:.js] extension.

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
            [: [codesnippet:@core/math] registers math functions and constants.]
        ]

        [data:
            [: Source library root]
            [: Register a source-library prefix to a filesystem root.]
            [: [codesnippet:@html] resolves to builder-provided Beanstalk source.]
        ]

        [data:
            [: External import provider]
            [: Register a provider for file extensions such as [codesnippet:.js].]
            [: The HTML builder registers the JS provider for project-local JavaScript imports.]
        ]

        [data:
            [: Builder runtime package]
            [: Register a virtual package plus runtime asset metadata.]
            [: [codesnippet:@web/canvas] is bundled JS parsed and emitted by the HTML builder.]
        ]

        [data:
            [: Config key registry]
            [: Register backend-specific config keys before Stage 0 config validation.]
            [: The HTML builder registers keys such as [codesnippet:html_lang] and [codesnippet:redirect_index_html].]
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
    - no direct [codesnippet:.js] imports from [codesnippet:#config.bst]
    - no direct cross-module bypass into another module's private JavaScript file
    - no automatic documentation extraction from [codesnippet:#mod.bst] yet
    - no HIR cache for source libraries yet

    The practical model for now is:

    - use [codesnippet:lib/<name>/#mod.bst] for project-local Beanstalk libraries.
    - use [codesnippet:@core/...], [codesnippet:@html], and [codesnippet:@web/canvas] for builder-provided libraries.
    - use small annotated [codesnippet:.js] wrapper files when a project needs browser JavaScript directly.
]
