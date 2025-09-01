[Navbar]

[Header center: [title(1): ROADMAP FOR BEANSTALK]]
[Page:

## Ongoing task list
- Ast creation must export each individual function / top level declaration as individual units.
This will make resolving imports easier in cases where modules import from one another.

- Built in array methods (bake directly into the compiler)

- enforce commas in scenes

- Runtime stuff needs to be outputted fully

- Scenes will parse blocks and look for style properties and insert sucessive scenes passed into it into a 'slot' scene if it has one.
If multiple scenes with slots are added to a scene, they need to be parsed correctly so each successsive scene is passed into the slot of the previous scene.
If that slot has any styles, the styles need to be applied each time a new scene is added into that slot. 
Otherwise all the scenes are just passed directly into the slot one by one.

- Runtime and reactive getters/setters for scene mutable variables. Figure out what properties scenes expose that can be manipulated at runtime.

- Type checking and final folding stage after AST creation for imports (and probably all variables too)

- Build out HTML standard library for styles once they can be folded at compile time and/or scenes have full runtime getting/setting working

- Figure out some kind of 'standardised error' that allows differnet custom error types to be coersed into eachother,
allowing for more compatiblity between libraries while also allowing custom error types

- Dev output is very readable JS and HTML. Run release builds through a minifier.

## Current Bugs

## Vscode plugin
- Characters like strings inside scenes don't highlight like strings

## Core Features Being Implemented
- Boolean expressions and operators
- If statements
- Loops
- Finish implementing Style system for scenes (separate from core language into library of keywords)
- Capitol letter constant enforced
- Importing HTML, CSS and JS libraries seemlessly into a BS project
- Scoping and variable shadowing?
- Folding (inlining) compile time functions (need to do step through eval in inline_function_call)

## Future plans and ideas
- Any type?
- https://docs.orca-app.dev/ Orca for native UI? Or just Servo as optional depenancy? https://servo.org/
- Documentation scenes (can be put anywhere in a module or scene and a special docs compile mode will generate docs using these scenes)
There can also be additional features or searchable things added to these scene heads that make the documentation of the project really easy
it will output an entire documentation webpage by default
- Embedding on top of fast exciting general purpose languages
- Better Dev server with web sockets handling hot reloading (rather than the current janky system)
- Server files and server side BS code (Using a Wasm runtime on the server)

### Function Context
Function signatures must be given a context if they are not pure.

### Colour type ideas
Would be a function that creates an RGB value at the end.

Arguments:
- Would be able to mix colors in (like painting) e.g red(100) green(20) magenta(40).
This will be converted to a relative hue value. This can approach white or black without a value.
- Saturation
- Lightness
- Hue (becomes priority colour input when used - but other colours can still be mixed in, which will modify the hue/saturation)
- Integer (or float) passed in to select a theme colour as a base (primary colour = 1, secondary = 2 etc) Maybe 0 is always black or white depending on lightmode/darkmode

The colour theme selections will have a lightmode / darkmode


### Concurrency (channels/threads) syntax
Like go, but hopefully simpler and clearer control flow. 

Uses >> and << to specify send / recieve channels. No "chan" keyword.

keyword 'async' to mark async function calls.

[Code:
    -- Create a channel
    channel <<int>>

    -- Send a value to the channel (notice value on the left, not right)
    -- Channel operator can't be used in the middle of an expression, must be right at the end
    5 >> channel

    -- Recieve a value from the channel
    x = << channel

    -- Close the channel
    channel close()
]

[Footer]