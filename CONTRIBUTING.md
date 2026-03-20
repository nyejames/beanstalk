# Contributing to this project
This project is at a very early stage with long term goals.

The current goal is compiling the documentation in the language itself using the HTML project builder without errors. This is still a solo project as of now.

This is a slow process as I'm writing the docs how in the way I want them to look like in Beanstalk code, then implement features I feel are missing or fix bugs along the way.

If you are interested in compilers/programming languages or the goals of this language 
and want to contribute or make suggestions, please get in touch or open a discussion on GitHub.

Any suggestions or questions about the future / design of this language are welcome.

See `docs/Beanstalk Language Overview.md` and `docs/Beanstalk Compiler Development Guide.md` for more in depth details.

New code constributions must follow the style guide: `docs/Beanstalk Compiler Codebase Style Guide.md`.

## New contributions
If you are thinking of contributing, start with something small that is easy to read and review and follow the style guide closely. Readbility and modularity is *TOP PRIORITY* in this codebase. 90% of the time I use a simple subset of Rust that avoids complexity as the primary goal.

Only as things really solidify will that code get reviewed for performance and more to noisier syntax and more 'clever' patterns.

## Agents
If using agents to help with contributing to this project, it is important that the .md files inside /docs are provided for context as a minimum. 

Minimising redundant code and reading and validating EVERYTHING an agent produces is really important for maintaining a managable codebase. 

One of the most common things I find myself doing it removing or refactoring code to reduce LOC and complexity, ask it to add more helpful, descriptive comments or telling it to keep tests separated from the rest of the code.

**Tests**

Agents should avoid updating or changing existing tests unless you understand exactly why a test might need to be updated and describe exactly how it should be updated.

Otherwise, the tests provide a useful baseline to prevent regressions and provide the agents a useful way to make progress without breaking stuff.

**Useful Rules**

I create detailed integration plans first and also use the following rules sometimes to make it clear what the priorities are for this codebase:

ALWAYS Use modern, up to date idiomatic Rust code that follows best practices

ALWAYS precisely follow project documentation and always ensure new code is following the style guide and design goals of the project

ALWAYS add helpful comments to new functions, struct parameters and control flow that concisely describes WHAT code is doing and WHY it is there. These should be helpful for someone reading the code for the first time to quickly understand the codebase

ALWAYS ensure code is as readable and maintainable as possible

ALWAYS add a review stage to new plans that includes making sure code being worked on adheres to any provided style guides and checks for opportunities to reduce complexity / indirection or LOC through removing redundancy, bad patterns or consolidating other parts of the code.