<div align="center">

  <h1>Beanstalk 🌱</h1>

  <p>
    <strong>A lightweight, purpose built, programming language and toolchain that compiles to Web Assembly.</strong>
  </p>

  *The only BS in programming should be in the filename*

  <br>

  ---
  <br>

  <p>⚠️ This is currently a work in progress compiler and proof of concept. It's not recommended you try and actually use it</p>
  <p>⚠️ The design and direction of the language is still subject to change</p>
  <p> (There isn't even a semantic version yet!)</p>

[//]: # (  <h1>)

[//]: # (    <a href="https://nyejames.github.io/beanstalk">)

[//]: # (      Plans and Documentation)

[//]: # (    </a>)

[//]: # (  </h1>)

  <p>The first proof of concept for the syntax has been figured out, and now the compiler is being completely refactored to accommodate these changes</p>

[//]: # (  <p>The docs were created using this language. The output of the compiler is directly pushed to GitHub pages. Not everything in the documentation has been implemented fully, it's mostly full of design plans.</p>)
  <a href="https://github.com/nyejames/beanstalk-plugin">Language support / syntax highlighting for Visual Studio / VSCode can be found here</a>

</div>

<br>
<br>

# Overview / Goals
Beanstalk is a statically typed language that compiles to Wasm and aims to generate glue code for that Wasm.

The whole language is built around an extremely clean template syntax.

The goal is to also be a more general lightweight Wasm-focused language. 
The language aims to be great for building a website, making config files or as a lightweight embedded or UI language for apps and games.

**Design Goals**
- Minimal syntax with very powerful string templates for generating text content / typesetting and styling or configs
- Simple, static type system
- Comes batteries-included with as much tooling as possible integrated directly in the compiler
- Fast compile times to support hot reloading
- No LLVM backend, no preferred Wasm runtime targets
- Borrow checker with no 'unsafe' and much simpler move semantics / lifetimes than Rust

### Compiled Output
The compiler aims to optimise specifically for Wasm, and create small and efficient modules. No 100kb Wasm files to just print "hello world".

Being compiled means folding constants, type checking and optimising is all done for you.
But the priority is to compile fast and small modules rather than highly optimised ones.

The compiler itself is written in Rust, and uses as few dependencies as possible to keep it fast and reliable.

<br>
