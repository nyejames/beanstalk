<div align="center">

# Beanstalk

<p><em>
  A refreshing Wasm-first language that prioritises safety, simplicity, and a fast compiler.
</em></p>

# 🌱

<p>⚠️ This is a work in progress compiler ⚠️</p> 
<p>The compiler backend (HIR, LIR, borrow checking, and codegen) is still under active development and evolving rapidly. See CONTRIBUTING.md and get in touch if you're inspired to help out</p>
</div>
<br>
<br>

<div align="center">

## First Class Template Syntax

</div>

Beanstalk is designed for UI generation, templated content, and embedded Wasm applications, with a powerful compile-time and runtime template system at its core.

```haskell
import @html/Basic

-- Create a new blog post
create_post |title String, date Int, content String| -> String:
    
    io("Creating a blog post!")

    formatted_blog = [Basic.page:
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

This file now compiles to HTML + Wasm:
```haskell
import @PostGenerator

date = 2025
post = PostGenerator.create_post(date, [:
    I have absolutely nothing interesting to say, and never will.
])

[post]
```

<div align="center">

## Unique Memory Model

</div>

Memory safety is enforced through static analysis and a unified runtime ownership model, which keeps binaries small and compilation predictable. All without a GC or reference counting.

This is a fairly unique idea based off of taking ideas from Rust and creating a tradeoff that allows for faster compile times and a simpler, friendlier language. There are no lifetime annotations or complex ownership or move semantics to worry about.
The cost is a small amount of runtime overhead and keeping the single mutable reference rule from Rust.

Beanstalk isn't trying to be a zero cost abstraction language, but is still trying to be faster and more predictable than a GC or RC language and safer than manual memory mangaement.

<div align="center">

## Goals 

</div>

- String templates that can double up as a tiny compile time HTML/markup engine or anything else you want
- Wasm focused backend designed around producing high-quality Wasm for portability and web integration
- Minimal and coherent syntax for maximum readbility
- A modular compiler with Fast compile times for snappy tooling and fast development builds
- Memory safety, with no 'unsafe' mode and no explicit lifetime syntax
- An integrated build system for web projects and beyond
- Simple, static and strong type system
- Clean and deterministic error handling

[//]: # (  <h1>)

[//]: # (    <a href="https://nyejames.github.io/beanstalk">)

[//]: # (      Plans and Documentation)

[//]: # (    </a>)

[//]: # (  </h1>)

[//]: # (  <p>The docs were created using this language. The output of the compiler is directly pushed to GitHub pages. Not everything in the documentation has been implemented fully, it's mostly full of design plans.</p>)

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

### Complete
- Header parsing 
- AST creation
- Expression folding
- Name resolution
- Type checking

### Underway
- IR construction
- Last use analysis
- Borrow Validation
- Codegen
- Build system

<br>
