<div align="center">

  <h1>Beanstalk 🌱</h1>

  <p>
    <strong>A simple, safe, statically typed programming language for building WebAssembly modules and web apps.</strong>
  </p>

  <br>

  ---
  <br>

  <p>⚠️ This is currently a work in progress compiler. See CONTRIBUTING.md if you're inspired to help out!</p>

  <h2>Current Progress</h2>
  <ul>
    <li><strong>Frontend</strong> - Core syntax implemented into an AST</li>
    <li><strong>Type Checking</strong> - Basics complete, an eager AST approach</li>
    <li><strong>Mid Level Optimisation</strong> - Fast Constant folding in place, more powerful optimisation passes to be added in the IR</li>
    <li><strong>Compiler Error Messages and CLI</strong> - Basics Complete, need to add more detailed errors and coverage</li>
    <li><strong>IR / Codegen</strong> - In progress. Basic Wasm optimised WIR structure and borrow checker in place, to be refactored and simplified later.</li>
    <li><strong>Build system</strong> - Basics in place, some scaffolding for dealing with different types of projects. To be expanded on greatly as the project grows
    </li>
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
Beanstalk is a statically typed, borrow checker language that compiles to Wasm and aims to provide all the glue code, runtimes and scaffolding for your Wasm project.

It is inspired by Go, Rust, Lua and bits of many other languages. You can think of some of its core design like a much simpler Rust with its own minimal syntax.

The goal is to also be lightweight, purpose built Wasm language. This means each project will be specifically tailored to producing all the files and glue code you need for creating an app or web page.

**Design Goals**
- Minimal, clean syntax with very powerful string templates that are perfect for generating text content / HTML / typesetting or other tree based string structures.
- Simple, strong, static type system
- Batteries-included with as much tooling as possible integrated directly in the compiler and standard library
- Fast development compile times to support easy hot-reloading and fast iteration, aiming to compete with interpreted/JIT languages while still being AOT compiled and optimisable for production builds
- Backend designed around only producing high-quality Wasm efficiently and integrating with that ecosystem
- Secure and fast. No 'unsafe' but with simpler move semantics and lifetimes than Rust
- Uses a borrow checker with a simpler ownership model than Rust to trade some raw performance for consistency and ease of use

The compiler itself is written in Rust.

<br>
