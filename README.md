<div align="center">

# Beanstalk

<p><em>
  A high level language that prioritises modularity, safety and readability.
</em></p>

# 🌱

<p>⚠️ This is a pre-alpha work-in-progress compiler ⚠️</p> 

<p><a href="https://nyejames.github.io/beanstalk/">The documentation site for Beanstalk</a> was created using the language itself. This is the main testing ground for building static websites with Beanstalk. </p>

<p>The language is under rapid active development and evolving constantly. See <a href="https://github.com/nyejames/beanstalk/blob/main/CONTRIBUTING.md">CONTRIBUTING</a> if you're inspired to help out</p>
</div>
<br>
<br>

<div align="center">

## First Class String Template Syntax

</div>
<p>Beanstalk is designed to be an original, opinionated and a refreshing modern app building language and ecosystem</p>

<p>The build system is web first, while being modular enough to be agnostic about the backend or platform. This opens up a future where Beanstalk can be ran or embedded anywhere.
It originated with the desire to never have to use TypeScript, web frameworks or bloated UI/web ecosystems again.</p>
<p>Beanstalk is an attempt to make something fresh, future aware and carefully designed from the ground up for a wide variety of applications with the sleek feel of a modern high-level language</p>

<br>

```haskell
import @html {center}
import @blog_styles {section, divider}

-- Create a new blog post
# create_post |title String, date Int, content String| -> String:
    
    io("Creating a blog post!")

    formatted_blog = [section, $markdown:
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

The HTML build system will generate an HTML page from this code:

```haskell
import @generators {create_post}
import @blog_styles {page, title}

# page_title = "Pointless Blog!!!"

date = 2025
post = create_post("Boring Title", date, [$markdown:
    I have absolutely nothing interesting to say, and never will.
])

[page:
    [post]
]
```

<div align="center">
</div>

<div align="center">

## Goals 

</div>

- String templates that can double up as a tiny compile time HTML/markup engine or anything else you want. Perfect for UI generation and string formatting. Even comes with built-in markdown parsing.
- Readability-first syntax. Modern, concise and consistent.
- Modular and fast for snappy tooling and fast development builds
- Integrated build system and tooling for web projects and beyond
- Simple, static and strong type system with a borrow checker for writing confident, safe and correct code
- A memory model that can allow for future static optimizations. The GC can be  completly elided in ideal cases.
- Backend agnostic. Could be used as the baseline for a whole web framework, a Wasm module builder or eventually an embedded UI engine for Rust. Extendable to any target in the future.

<div align="center">

## Documentation

</div>
<strong>
<li>
<a href="https://github.com/nyejames/beanstalk/blob/main/docs/language-overview.md">The language</a>
</li>
<br>
<li>
<a href="https://github.com/nyejames/beanstalk/blob/main/docs/compiler-design-overview.md">An Overview of the Compiler</a>
</li>
<br>
<li>
<a href="https://github.com/nyejames/beanstalk/blob/main/docs/memory-management-design.md">The memory management strategy</a>
</li>
</strong>

<div align="center">

## Tools

</div>

<a href="https://github.com/nyejames/beanstalk-plugin">Language support / syntax highlighting for Visual Studio / VSCode</a>

<div align="center">
<br>

## Development Progress

</div>

Here is the current <a href="https://github.com/nyejames/beanstalk/blob/main/docs/roadmap.md">Roadmap to the first alpha release</a>.

The syntax and some constructs (e.g. closures, interfaces, async, pattern matching) will still evolve in their design during the alpha stage. Not everything is completely set in stone.

The Wasm backend scaffolding is in place but needs a lot of work before it is stable enough for regular projects.
Wasm backend stability is also a goal once the lanaguge is at the Alpha stage.

<br>
