# Contributing to this project
This project is at a very early stage with long term goals.

If you are interested in compilers/programming languages or the goals of this language 
and want to contribute or make suggestions, please get in touch or open a discussion on GitHub.

Any questions about the future / design of this language are welcome.
Just open a discussion on Github if you're curious.

## The Current Goal

The current goal is compiling the documentation in the language itself using the HTML project builder without errors. This is still a solo project as of now.

This is a slow process as I'm writing the docs in an ideal form of Beanstalk code, then implement the missing features or fix bugs along the way to get that code to work correctly without test regressions.

See <a href="https://github.com/nyejames/beanstalk/blob/main/docs/Beanstalk%20Language%20Overview.md">the language overview</a> and <a href="https://github.com/nyejames/beanstalk/blob/main/docs/Beanstalk%20Compiler%20Design%20Overview.md">the compiler overview</a> for more in depth details.

New code constributions must follow the style guide: <a href="https://github.com/nyejames/beanstalk/blob/main/docs/Beanstalk%20Compiler%20Codebase%20Style%20Guide.md">Codebase Style Guide</a>

## Testing

Run the compiler integration suite with `cargo run -- tests`.

New integration fixtures should use the canonical `tests/cases/<case>/input + expect.toml` layout.
An optional `tests/cases/manifest.toml` can define case ordering and tags during fixture migrations.

## New contributions
If you are thinking of contributing, start with something small that is easy to read and review and follow the style guide closely. Readbility and modularity is *TOP PRIORITY* in this codebase. 90% of the time I use a simple subset of Rust that avoids complexity as the primary goal.

Only as things really solidify will that code get reviewed for performance and more to noisier syntax and more 'clever' patterns.

## Agents
If using agents to help with contributing to this project, it is important that the .md files inside /docs are provided for context as a minimum. 

Minimising redundant code and reading and validating EVERYTHING an agent produces is really important for maintaining a managable codebase. 

You usually have to end up removing or refactoring agent generated code to reduce LOC and complexity, ask it to add more helpful, descriptive comments or tell it to keep tests separated from the rest of the code.

**Tests**

Agents should avoid updating or changing existing tests unless you understand exactly why a test might need to be updated and describe exactly how it should be updated.

Otherwise, the tests provide a useful baseline to prevent regressions and provide the agents a useful way to make progress without breaking stuff.

There is an AGENTS.md file in the root directory that can be used as a baseline for improving LLM output when working with this codebase.
