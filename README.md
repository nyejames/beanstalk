<div align="center">

# Beanstalk

<p><em>
  A high-level, ambitious programming language for building reliable apps and websites
</em></p>

# 🌱

<p>⚠️ This project is in Alpha ⚠️</p> 

<p><a href="https://nyejames.github.io/beanstalk/">The documentation site</a> was created using <a href="https://github.com/nyejames/beanstalk/blob/main/docs/src">Beanstalk</a>. </p>

<p>The language is in active development and changes quickly. See <a href="https://github.com/nyejames/beanstalk/blob/main/CONTRIBUTING.md">CONTRIBUTING</a> if you're interested in helping with this project.</p>
</div>
<br>
<br>

<div align="center">

## What is Beanstalk?

</div>
<p>Beanstalk is a small, opinionated language that gives you as much tooling as possible already baked into the language and build system.</p>

<p>The first-party HTML builder makes web development the current focus. A pluggable builder interface keeps the compiler open to other targets without turning the language into a pile of target-specific exceptions.</p>

<p>Templates are first-class language values. They can produce text, Markdown and HTML, fold at compile time or lower to runtime string construction, with structured slots, wrappers and formatter directives. They replace the same underpowered string formatter functions/macros languages have been stuck using forever</p>

<p>Deliberately small: static nominal types, explicit trait conformance and constrained generics. There is no general macro system or turing-complete type system spaghetti.</p>

<p>Borrow validation enforces exclusive mutation and move safety today. A GC semantic baseline keeps execution practical while ownership analysis leaves room for future allocation and deterministic-drop optimisations.</p>

<p>The goal is modern application development without TypeScript framework lasagne, build-tool linguini or 17 package dependencies for padding a string.</p>

<br>

```haskell
import @html {center}
import @blog_styles {section, divider}

-- Create a new blog post
create_post |title String, date Int, content String| -> String:
    
    io.line("Creating a blog post!")

    formatted_blog = [section, $md:
        [date]
        [center: 
            # [title]
            ## The worst blog on the internet
        ]

        [divider]

        [content]
    ]

    return formatted_blog
;
```

`@html` is provided by the HTML builder. Project-local libraries can live under `/lib` and import through their exposed prefix.

The HTML build system will generate an HTML page from this code:

```haskell
import @generators {create_post}
import @blog_styles {page, title}

page_title #= "Pointless Blog!!!"

date = 2025
post = create_post("Boring Title", date, [$md:
    I have absolutely nothing interesting to say, and never will.
])

[page:
    [post]
]
```

<div align="center">

## Beanstalk loves Markdown

</div>

Beanstalk includes its own compact, template-aware flavour of Markdown through the built-in `$md` formatter. Markdown lives inside normal templates, so content can capture values, compose styles and fold straight into HTML at compile time.

This makes content-heavy pages quick to build and easy to format. Write prose where it belongs, keep templates around it and skip the traditional ceremony of feeding Markdown through a JavaScript plugin stack to recover the HTML you wanted in the first place.

## Getting Started

`bean` is the project tool for creating, checking, building and running Beanstalk projects.
It is the CLI bundled with the compiler and build system.

It's currently best to install the compiler manually from a tagged release. Installation scripts will arrive for Beta.

### Check the installation

```bash
bean --version
```

### Create a project

```bash
bean new html my-site
cd my-site
```

### Run the development server

```bash
bean dev .
```

The dev server rebuilds the project when files change and refreshes the browser output automatically.

### Release build

```bash
bean build . --release
```

This compiles the project using the configured Beanstalk builder and writes output to the configured release directory. The default scaffold uses `/release`.

### Check a project compiles without writing output

```bash
bean check .
```

<br>

<div align="center">
</div>

<div align="center">

## Goals 

</div>

- First-class string templates powerful enough to act as a small compile-time markup engine. They support built-in Markdown, formatting, slots and reactive runtime output.
- Readable, consistent syntax. Each keyword or symbol exclusively covers one concept.
- Fast, modular tooling for short feedback loops and quick development builds (currently needs a lot more optimisation work).
- One project tool for checking, building and running the development server.
- A small static type system plus borrow validation memory-safe code free of data races and iterator invalidation by default.
- A GC fallback with ownership analysis that can remove runtime collection in ideal cases.
- A backend-neutral frontend. HTML and JavaScript are the Alpha target, Wasm is experimental and other builders can follow.
- Few dependencies. A language project shouldn't need a PhD dissertation for a lockfile.

<div align="center">

## LLM-aware design

</div>

Beanstalk assumes developers are using coding agents increasingly as part of their workflow.

Programmers should own the final design and architecture. Agents can handle repetitive churn, provided the compiler gives them nowhere comfortable to hide mistakes.

Readability is not decoration. A small syntax, strict rules, fast tooling and stable diagnostics make generated changes easier to inspect and easier to reject when they are wrong.

Compiler diagnostics carry stable codes, structured facts and source metadata for editors, development servers and coding agents.

Beanstalk has very little legacy training data. That is inconvenient today and useful later: examples can grow around the language that exists. No legacy frameworks bloating the picture.

<div align="center">
  
## Documentation

</div>
<strong>
<li>
<a href="https://github.com/nyejames/beanstalk/blob/main/docs/language-overview.md">The language</a>
</li>
<br>
<li>
<a href="https://nyejames.github.io/beanstalk/docs/codebase/compiler-design/">Compiler design</a>
</li>
<br>
<li>
<a href="https://nyejames.github.io/beanstalk/docs/codebase/memory-management/">Memory management</a>
</li>
<br>
<li>
<a href="https://nyejames.github.io/beanstalk/docs/codebase/style-guide/">Development standards</a>
</li>
</strong>

<div align="center">

## Tools

</div>

<a href="https://github.com/nyejames/beanstalk-plugin">Language support and syntax highlighting for Visual Studio Code</a>

<div align="center">
<br>

## Development Progress

</div>

Here is the current <a href="https://nyejames.github.io/beanstalk/docs/progress/">implementation progress matrix</a>.

The compiler already has broad frontend, backend and build-system tooling in place.

<br>
