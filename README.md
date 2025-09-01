<div align="center">

  <h1>Beanstalk 🌱</h1>

  <p>
    <strong>A lightweight, purpose built, programming language for building WebAssembly modules and web apps.</strong>
  </p>

  <p><em>
    A free and open source project
  </em></p>

  <br>

  ---
  <br>

  <p>⚠️ This is currently a work in progress compiler. It's not recommended you try and actually use it. See CONTRIBUTING.md if you're inspired to help out!</p>
  <p> (There isn't even a semantic version yet!)</p>

  <h2>Current Progress</h2>
  <ul>
    <li><strong>Frontend</strong> - Core, basic syntax implemented into an AST</li>
    <li><strong>Type Checking</strong> - Basics complete, trying out an eager AST approach with early import parsing</li>
    <li><strong>Mid Level Optimisation</strong> - Fast Constant folding in place, more powerful optimisation passes to be added in the IR</li>
    <li><strong>Compiler Error Messages and CLI</strong> - Basics Complete, to be expanded on</li>
    <li><strong>IR / Codegen</strong> - In progress. Basic Wasm optimised MIR structure and borrow checker in place</li>
    <li><strong>Build system</strong> - Basics in place, tentative handling of imports and different project structures. To be expanded on greatly as the project grows</li>
  </ul>

[//]: # (  <h1>)

[//]: # (    <a href="https://nyejames.github.io/beanstalk">)

[//]: # (      Plans and Documentation)

[//]: # (    </a>)

[//]: # (  </h1>)

[//]: # (  <p>The docs were created using this language. The output of the compiler is directly pushed to GitHub pages. Not everything in the documentation has been implemented fully, it's mostly full of design plans.</p>)
<br>
<h2>Tools</h2>
<a href="https://github.com/nyejames/beanstalk-plugin">Language support / syntax highlighting for Visual Studio / VSCode can be found here</a>

</div>

<br>
<br>

# Overview / Goals
Beanstalk is a statically typed language that compiles to Wasm and aims to provide all the glue code, runtimes and scaffolding for your Wasm project.

It is inspired by Go, Rust, Lua and bits of many other languages.

Beanstalk is built around an extremely clean and powerful string template syntax. It includes a Markdown parser with its own flavour specifically integrated with this template syntax.

The goal is to also be lightweight, purpose built Wasm language. This means each project will be specifically tailored to producing all the files and glue code you need. 

The aim is to be able to easily slot Beanstalk files into existing projects by automatically producing the needed Wasm or HTML, or build flexible projects from scratch (Web / Native / Embedded / Wasm libraries).

**Design Goals**
- Minimal, clean syntax with very powerful string templates for generating text content / HTML / typesetting
- Simple, static type system
- Comes batteries-included with as much tooling as possible integrated directly in the compiler
- Fast development compile times to support easy hot-reloading and fast iteration, aiming to compete with interpreted/JIT languages while still being AOT compiled
- Optimised for producing Wasm
- Secure and fast. No 'unsafe' but with simpler move semantics and lifetimes than Rust
- No garbage collector or RC. Uses a borrow checker with a simpler ownership model than Rust to trade some raw performance for consistency and ease of use

The compiler itself is written in Rust, and uses as few dependencies as possible to keep the frontend fast and reliable.

<br>
