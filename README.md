<div align="center">

# Beanstalk

<p><em>
  A high level language that prioritises safety, readability and scalability.
</em></p>

# 🌱

<p>⚠️ This is a work in progress compiler ⚠️</p> 
<p>The compiler backend is still under active development and evolving rapidly. See CONTRIBUTING.md if you're inspired to help out!</p>
</div>
<br>
<br>

<div align="center">

## First Class String Template Syntax

</div>
<p>Beanstalk is designed first for the web. It originated with the desire to never have to use TypeScript, web frameworks or bloated web ecosystems again.</p>
<p>Beanstalk is an attempt to make something fresh and carefully designed from the ground up.</p>
<br>
<br>

```haskell
import @(html/basic)

-- Create a new blog post
#create_post |title String, date Int, content String| -> String:
    
    io("Creating a blog post!")

    formatted_blog = [basic.section:
        [basic.small, date]
        [basic.center: 
            # [title]
            ## The worst blog on the internet
        ]

        [basic.divider]

        [content]
    ]

    return formatted_blog
;
```

The HTML build system will generate an HTML page from this code:
```haskell
import @(generators/create_post)
import @(html/basic)

date = 2025
post = create_post(date, [:
    I have absolutely nothing interesting to say, and never will.
])

-- Compile time generated HTML
#[basic.prelude, basic.title: Pointless Blog!!!]

-- Runtime generated HTML
[basic.page:
    [basic.pad(3), post]
]
```

<div align="center">
</div>

<div align="center">

## Goals 

</div>

- String templates that can double up as a tiny compile time HTML/markup engine or anything else you want. Perfect for UI generation and string formatting.
- Readability-first syntax. As modern, minimal, coherent and consistent as possible.
- A modular compiler with Fast compile times for snappy tooling and fast development builds
- An integrated build system for web projects and beyond
- Simple, static and strong type system
- Clean and deterministic error handling
- A memory model that can allow for future static optimizations – to the point of completely eliding the GC when desired.

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
<a href="https://github.com/nyejames/beanstalk/blob/main/docs/Beanstalk%20Memory%20Management.md">A breakdown of the memory management strategy</a>
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

Current focus: stabilising the JS backend/build system for static page generation and JS output.
The syntax and some constructs (e.g., closures, interfaces, async) are still evolving in their design before full pipeline support.

<br>
