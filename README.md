<div align="center">

# Beanstalk

<p><em>
  A high level language that prioritises modularity, safety and readability.
</em></p>

# 🌱

<p>⚠️ This is a pre-alpha work-in-progress compiler - nearly at alpha! ⚠️</p> 

<p><a href="https://nyejames.github.io/beanstalk/">The documentation site for Beanstalk</a> was created using the <a href="https://github.com/nyejames/beanstalk/blob/main/docs/">Language Itself</a>. </p>

<p>The language is under rapid active development and evolving constantly. See <a href="https://github.com/nyejames/beanstalk/blob/main/CONTRIBUTING.md">CONTRIBUTING</a> if you're inspired to help out</p>
</div>
<br>
<br>

<div align="center">

## First Class String Template Syntax

</div>
<p>Beanstalk is designed to be an opinionated and refreshing take on modern app building.</p>

<p>This project is focused on the long term and designed to be versatile and future proof. A carefully designed, high-level language for the future.</p>

<p>The main build system is web based, but the compiler can have any number of pluggable backends through its builder interface. This opens up a future where Beanstalk can be ran or embedded anywhere.</p>

<p>This is a serious attempt to never have to use TypeScript, web frameworks or bloated UI/web ecosystems again.</p>

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

- String templates powerful enough to become a tiny compile time HTML/markup engine. Perfect for UI generation and complex string formatting. Even comes with built-in markdown parsing.
- Readability-first syntax. Modern, concise and consistent.
- Modular and fast for snappy tooling and fast development builds
- Integrated build system and tooling for web projects and beyond
- Simple, static and strong type system with a borrow checker for writing confident, safe and correct code
- A memory model that can allow for future static optimizations. The GC can be  completly elided in ideal cases.
- Backend agnostic. Could be used as the baseline for a whole web framework, a Wasm module builder or eventually an embedded UI engine for Rust. Designed to be extendable to any target in the future.
- Keep compiler dependencies as few as possible

<div align="center">

## LLM Aware design

</div>

Beanstalk is designed for a future where LLM workflows are inevitable: 

Humans should validate, review and write the more declarative, creative and fun parts of the codebase while LLMs cover the boring churn. 

This is one reason why readability is the primary goal of the language.

The strict compiler and snappy, modular tooling enables LLMs to iterate fast and avoid bad patterns due to Beanstalk being opinionated, memory safe and panic avoidant. The language is simple and terse which is ideal for context limits and human validation.

Even the way compiler errors are designed is to provide good metadata for LLMs right from the start, not just pretty human readable ones.

Beanstalk not being saturated in LLM training data may provide a the long term advantage of having a smaller, higher quality codebase dataset as the language matures.

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

Here is the current <a href="https://github.com/nyejames/beanstalk/blob/main/docs/roadmap/roadmap.md">Roadmap to the first alpha release</a>.

The language is nearly at the first alpha stage, but already has a broad set of tooling, build system work and backend scaffolding already done. The upcoming alpha will be about taking an already powerful set of tools and language and making it stable and usable.

The syntax and some constructs (e.g. closures, async) are not implemented at all yet and will evolve in their design during the alpha stage. Not everything is completely set in stone so some things can be figured out based on how the language feels to use in real projects.

The goal once hitting alpha is to have a stable Wasm backend for web projects.

<br>
