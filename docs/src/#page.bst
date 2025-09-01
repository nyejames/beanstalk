[Navbar]

[Header center pad(2, 0, 0, 0):
    [title(1): THE BEANSTALK BOOK]
    # &#127793;
][Page:

## What is Beanstalk?
This website was created using Beanstalk! 
It's an open source, strongly typed language that compiles to HTML, CSS and Web Assembly (and some necessary Javascript).

The documentation is currently more a collection of evolving design ideas and notes on the language as it's being developed. 
Most of the language is still in the planning stages and not yet implemented.

## Documentation
[red size(0.85): 
    *Only a fraction of this documentation is implemented so far. It is being used as a design plan and notes on the vision for the language and to keep track of ideas.*
]
- [link("./docs/setting-up-a-project"): Setting up a project]
- [link("./docs/basics"): Basic Language Syntax ]
- [link("./docs/scenes"): Introduction to scenes]
- [link("./docs/control-flow"): Control Flow]
- [link("./docs/collections"): Collections]
- [link("./docs/types-and-tuples"): Types and Tuples]
- [link("./docs/functions"): Functions]
- [link("./docs/errors"): Error Handling and Option Types]
- [link("./docs/embedding-js"): Embedding JS]

More documentation to be added soon!

## Language Design Goals
The Beanstalk is designed to be:

### Simple and easy to learn
Minimal keywords and symbols to reduce cognitive load and syntax noise. 
One way to do things where possible. 
Beanstalk trades expressiveness for readibility and simplicity.
As little boilerplate as possible. 
Safe type inferance and batteries included core language features to reduce the amount of code you need to write for simple tasks.

### Easy to debug and resistant to runtime errors
Strong compile time checks that mean you never forget to handle an error, use the wrong type, or struggle to find the source of a bug.

### Seemless syntax for declaritive programming
Scenes provide a very clean and concide way to describe the structure of a webpage or UI (or any other declaritive structure in the future).
Having a declaritive syntax inside the language itself makes it easy to reason about desgining UI and visual components.

### Fast compile times
Being a compiled language, Beanstalk should have very fast compile times. 
The compiler is written in Rust and is designed to be fast and efficient. 
With built in hot reloading you should see changes to your code and UI designes almost instantly for fast iteration.

## Current Status
This is a work in progress site for Beanstalk documentation as it's being developed. 

Beanstalk is in the early stages of development (it doesn't even have a version number yet!), the documentation is subject to frequent changes. The syntax and design of the language itself is still being iterated on and refined.

The source code for this site on [link("https://github.com/nyejames/beanstalk/tree/main/docs/src"): Github]. You can view the [link("https://github.com/nyejames/beanstalk/tree/main/docs/release"): output of the Beanstalk compiler]!

## Contributing
If you are intersted in contributing, it's very early days, but if you want to contribute to the language, please get in touch.

You can find the [link("./docs/roadmap"): roadmap here].

]

[Footer]