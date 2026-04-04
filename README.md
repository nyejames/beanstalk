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
<p>Beanstalk is designed to be an original, opinionated and a refreshing modern app building experience.</p>

<p>The build system is web first, while being modular enough to be agnostic about the backend or platform. 
It originated with the desire to never have to use TypeScript, web frameworks or bloated UI/web ecosystems again.</p>
<p>Beanstalk is an attempt to make something fresh, future aware and carefully designed from the ground up for a wide variety of applications</p>

<br>
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

- String templates that can double up as a tiny compile time HTML/markup engine or anything else you want. Perfect for UI generation and string formatting.
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
</strong>

<div align="center">

## Tools

</div>

<a href="https://github.com/nyejames/beanstalk-plugin">Language support / syntax highlighting for Visual Studio / VSCode</a>

<div align="center">
<br>

## Development Progress

</div>

Before the first alpha release here are the current goals:

- Stabilising the JS backend and HTML project build system
- Make user facing errors *WAY* more comprehensive, descriptive and helpful
- All core syntax and language features must be represented and have comprehensive tests in place throughout the whole pipeline

These goals are now close to being met and a first alpha release should come within the next couple of months.

The syntax and some constructs (e.g. closures, interfaces, async) are still evolving in their design before full pipeline support. Not everything is set in stone with the design yet.

The Wasm backend scaffolding is in place but needs a lot of work before it is stable enough for regular projects.

<br>
