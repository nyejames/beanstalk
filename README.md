<div align="center">

# Beanstalk

<p><em>
  A high level language that prioritises modularity, safety and readability.
</em></p>

# 🌱

<p>⚠️ This project is in Alpha ⚠️</p> 

<p><a href="https://nyejames.github.io/beanstalk/">The documentation site</a> was created using <a href="https://github.com/nyejames/beanstalk/blob/main/docs/src">Beanstalk</a>. </p>

<p>The language is under rapid active development and evolving constantly. See <a href="https://github.com/nyejames/beanstalk/blob/main/CONTRIBUTING.md">CONTRIBUTING</a> if you're inspired to help out</p>
</div>
<br>
<br>

## Getting Started

`bean` is the main project tool for creating, checking, building, and running Beanstalk projects.
It is a cli tool bundled with the compiler and build system.

#### macOS and Linux

```bash
curl -fsSL https://raw.githubusercontent.com/nyejames/beanstalk/main/install.sh | sh
````

This installs `bean` into:

```bash
~/.local/bin
```

Make sure this directory is in your `PATH`:

```bash
export PATH="$PATH:$HOME/.local/bin"
```

To install somewhere else:

```bash
BIN_DIR="$HOME/bin" curl -fsSL https://raw.githubusercontent.com/nyejames/beanstalk/main/install.sh | sh
```

#### Windows

In PowerShell:

```powershell
irm https://raw.githubusercontent.com/nyejames/beanstalk/main/install.ps1 | iex
```

To add `bean` to your user `PATH` automatically:

```powershell
$script = irm https://raw.githubusercontent.com/nyejames/beanstalk/main/install.ps1
Invoke-Expression "$script -AddToPath"
```

### Check the installation

```bash
bean --version
```

Expected output:

```text
bean 0.1.4
```

### Create a project

```bash
bean new my-site
cd my-site
```

### Run the development server

```bash
bean dev
```

The dev server rebuilds the project when files change and refreshes the browser output automatically.

### Release Build

```bash
bean build --release
```

This compiles the project using the configured Beanstalk builder and writes the output to the project’s `/release` output directory.

### Check a project without writing output

```bash
bean check
```

Use this when you want compiler diagnostics without producing final build artifacts.

<br>

<div align="center">

## What is Beanstalk?

</div>
<p>Beanstalk is designed to be an opinionated, batteries-included, refreshing language for modern app building.</p>

<p>This project is focused on the long term and carefully designed to be versatile and future proof.</p>

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

`@html` is provided by the HTML builder. Project-local libraries can live under `/lib` and import through their exposed prefix.

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
- Readability-first syntax. Modern, concise, consistent and procedural.
- Modular and fast for snappy tooling and fast development builds
- Integrated build system and tooling for web projects and beyond
- Simple, static and strong type system with a borrow checker for writing confident, safe and correct code
- A memory model that can allow for future static optimizations. The GC can be  completly elided in ideal cases.
- Backend agnostic. Could be used as the baseline for a whole web framework, a Wasm module builder or eventually an embedded UI engine for Rust. Designed to be extendable to any target in the future.
- Few compiler dependencies

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

Here is the current <a href="https://nyejames.github.io/beanstalk/docs/progress/">implementation progress matrix</a>.

This project already has a broad set of tooling, build system work and most of the backend/frontend scaffolding already done.

<br>
