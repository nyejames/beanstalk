<div align="center">

# Beanstalk

<p><em>
  A high level language that prioritises modularity, safety and readability.
</em></p>

# 🌱

<p>⚠️ This is a work in progress compiler ⚠️</p> 
<p>The language is under rapid active development and evolving constantly. See CONTRIBUTING.md if you're inspired to help out!</p>
</div>
<br>
<br>

<div align="center">

## First Class String Template Syntax

</div>
<p>Beanstalk is designed first for the web, while being modular enough to be agnostic about the backend or platform. 
It originated with the desire to never have to use TypeScript, web frameworks or bloated UI/web ecosystems again.</p>
<p>Beanstalk is an attempt to make something fresh and carefully designed from the ground up.</p>
<br>
<br>

```haskell
import @html/center
import @blog_styles/{section, divider}

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
import @generators/create_post
import @html/doc
import @blog_styles/{page, title, pad2}

date = 2025
post = create_post(date, [$markdown:
    I have absolutely nothing interesting to say, and never will.
])

-- Compile time generated HTML
#[doc.prelude, title: Pointless Blog!!!]

-- Runtime generated HTML
[page:
    [pad2, post]
]
```

<div align="center">
</div>

<div align="center">

## Goals 

</div>

- String templates that can double up as a tiny compile time HTML/markup engine or anything else you want. Perfect for UI generation and string formatting.
- Readability-first syntax. As modern, concise and consistent as possible.
- Modular and fast for snappy tooling and fast development builds
- Integrated build system and tooling for web projects and beyond
- Simple, static and strong type system with a borrow checker for writing confident, safe and correct code
- A memory model that can allow for future static optimizations – to the point of completely eliding the GC for non-GC platforms if needed.
- Backend agnostic. Can be used to generate JS, Wasm or as a whole web framework. Extendable to any target in the future.

<div align="center">

## Documentation

</div>
<strong>
<li>
<a href="https://github.com/nyejames/beanstalk/blob/main/docs/Beanstalk%20Language%20Overview.md">The language</a>
</li>
<br>
<li>
<a href="https://github.com/nyejames/beanstalk/blob/main/docs/Beanstalk%20Compiler%20Design%20Overview.md">An Overview of the Compiler</a>
</li>
<br>
<li>
<a href="https://github.com/nyejames/beanstalk/blob/main/docs/Beanstalk%20Memory%20Management.md">The memory management strategy</a>
</li>
<br>
<li>
<a href="https://github.com/nyejames/beanstalk/blob/main/docs/Beanstalk%20Compiler%20Codebase%20Style%20Guide.md">Codebase Style Guide</a>
</li>
</strong>

<div align="center">

## Tools

</div>

<a href="https://github.com/nyejames/beanstalk-plugin">Language support / syntax highlighting for Visual Studio / VSCode can be found here</a>

<div align="center">

## Development Progress

</div>

Current focus: stabilising the JS and Wasm backend and HTML project build system for static website generation.

<a href="https://nyejames.github.io/beanstalk/">The documentation site for this language is being written in Beanstalk.</a>

The syntax and some constructs (e.g. closures, interfaces, async) are still evolving in their design before full pipeline support.

<br>

<div align="center">

## Testing

</div>

Run the compiler integration suite with `cargo run -- tests`.

New integration fixtures should use the canonical `tests/cases/<case>/input + expect.toml` layout.
An optional `tests/cases/manifest.toml` can define case ordering and tags during fixture migrations.
