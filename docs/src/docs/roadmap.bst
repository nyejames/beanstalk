import @libs/html/Basic
import @styles/Docs
import @./components
[basic.page:
[Navbar]

[Header center: [title(1): ROADMAP FOR BEANSTALK]]
[Page:

## Ongoing task list
- Interface Syntax?

- AST -> HIR figure out
- Borrow validation stage

- Scenes will parse blocks and look for style properties and insert successive scenes passed into it into a 'slot' scene if it has one.
If multiple scenes with slots are added to a scene, they need to be parsed correctly so each successive scene is passed into the slot of the previous scene.
If that slot has any styles, the styles need to be applied each time a new scene is added into that slot. 
Otherwise all the scenes are just passed directly into the slot one by one.

- Build out HTML standard library for styles once they can be folded at compile time and/or scenes have full runtime getting/setting working

- More complete statement and function call folding for pure statements/functions at AST stage

## Core Features Being Implemented
- Make HTML build system take top level templates and translate them directly to HTML / CSS / just
This should. JS will handle the basic runtime stuff, 
but functions and constants used outside of the top level template will still need to wait for Wasm integration and create the JS glue code to wire those in.

# Future plans and ideas
- Rewrite Dev server with web sockets handling hot reloading (rather than the current janky system)

[Footer]