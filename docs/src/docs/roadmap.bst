import @libs/html/Basic
import @styles/Docs
import @./components
[basic.page:
[Navbar]

[Header center: [title(1): ROADMAP FOR BEANSTALK]]
[Page:

## Ongoing task list
- Runtime stuff needs to be outputted fully

- Scenes will parse blocks and look for style properties and insert successive scenes passed into it into a 'slot' scene if it has one.
If multiple scenes with slots are added to a scene, they need to be parsed correctly so each successive scene is passed into the slot of the previous scene.
If that slot has any styles, the styles need to be applied each time a new scene is added into that slot. 
Otherwise all the scenes are just passed directly into the slot one by one.

- Build out HTML standard library for styles once they can be folded at compile time and/or scenes have full runtime getting/setting working

- Figure out some kind of 'standardised error' interface that allows different custom error types to be coersed into each other,
allowing for more compatibility between libraries while also allowing custom error types

## Current Bugs

## Core Features Being Implemented

# Future plans and ideas
- Better Dev server with web sockets handling hot reloading (rather than the current janky system)

### Function Context
Function signatures must be given a context if they are not pure.

[Footer]