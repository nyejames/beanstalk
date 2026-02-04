<div align="center">

# Beanstalk

<p><em>
  A high level language that prioritises safety, simplicity and string building.
</em></p>

# 🌱

<p>⚠️ This is a work in progress compiler ⚠️</p> 
<p>The compiler backend is still under active development and evolving rapidly. See CONTRIBUTING.md and get in touch if you're inspired to help out</p>
</div>
<br>
<br>

<div align="center">

## First Class Template Syntax

</div>
<p>Beanstalk is designed for the web. It originated with the desire to never have to use TypeScript, web frameworks or bloated web ecosystems again.</p>
<p>Beanstalk is an attempt to make something fresh and carefully designed from the ground up.</p>
<br>
<strong>An ambitious language for the future</strong>
<br>
<br>

```haskell
@(html/Basic)

-- Create a new blog post
create_post |title String, date Int, content String| -> String:
    
    io("Creating a blog post!")

    formatted_blog = [Basic.section:
        [Basic.small, date]
        [Basic.center: 
            # [title]
            ## The worst blog on the internet
        ]

        [Basic.divider]

        [content]
    ]

    return formatted_blog
;
```

The HTML build system will generate an HTML page from this code:
```haskell
@(PostGenerator)
@(html/Basic)

date = 2025
post = PostGenerator.create_post(date, [:
    I have absolutely nothing interesting to say, and never will.
])

[Basic.page:
    [Basic.pad(3), post]
]
```

<div align="center">
</div>

<div align="center">

## Goals 

</div>

- String templates that can double up as a tiny compile time HTML/markup engine or anything else you want
- Near-term milestone: a stable JS backend/build system for static page generation and JS output. Wasm remains the long-term primary target for portability and web integration
- Minimal and coherent syntax for maximum readability
- A modular compiler with Fast compile times for snappy tooling and fast development builds
- An integrated build system for web projects and beyond
- Simple, static and strong type system
- Clean and deterministic error handling

<div align="center">

## Documentation

</div>
<strong>
<li>
<a href="https://github.com/nyejames/beanstalk/blob/main/docs/Beanstalk%20Language%20Overview.md">The language overview</a>
</li>
<br>
<li>
<a href="https://github.com/nyejames/beanstalk/blob/main/docs/Beanstalk%20Compiler%20Design%20Overview.md">An Oveview of the Compiler</a>
</li>
<br>
<li>
<a href="https://github.com/nyejames/beanstalk/blob/main/docs/Beanstalk%20Compiler%20Codebase%20Style%20Guide.md">Codebase Style Guide</a>
</li>
<br>
<li>
<a href="https://github.com/nyejames/beanstalk/blob/main/docs/Beanstalk%20Memory%20Management.md">A breakdown of the memory management strategy</a>
</li>
</strong>

<div align="center">

## Tools

</div>

<a href="https://github.com/nyejames/beanstalk-plugin">Language support / syntax highlighting for Visual Studio / VSCode can be found here</a>

<div align="center">

## Development Progress

</div>

A full backend refactor of the compiler is underway.
Current focus: stabilising the JS backend/build system for static page generation and JS output; syntax and some constructs (e.g., closures, interfaces) are still evolving before full pipeline support.

<br>
