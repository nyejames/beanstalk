<div align="center">

  <h1>Beanstalk 🌱</h1>

  <p>
    <strong>A lightweight, purpose built, programming language for building WebAssembly apps.</strong>
  </p>

  *The only BS in programming should be in the filename*

  <br>

  ---
  <br>

  <p>⚠️ This is currently a work in progress compiler and proof of concept. It's not recommended you try and actually use it</p>
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
Beanstalk is a new, statically typed language that compiles to Wasm and aims to provide all the glue code, runtimes and scaffolding for your Wasm project.

Beanstalk is built around an extremely clean template syntax.

The goal is to also be lightweight, purpose built Wasm language. This means each project will be specifically tailored to producing all the files you need to deploy on your target platform (Web / Native / embedding / as a library).

Beanstalk aims to be great for building websites, making config files or as a lightweight embedded language for apps and games.

**Design Goals**
- Minimal syntax with very powerful string templates for generating text content / typesetting and styling or configs
- Simple, static type system
- Comes batteries-included with as much tooling as possible integrated directly in the compiler
- Fast development compile times to support easy hot-reloading and fast iteration
- Bundled with existing Wasm runtimes or HTML boilerplate for quickly building for a specific platform without having to scaffold everything yourself
- Secure and fast. Uses a borrow checker with no 'unsafe' but with simpler move semantics and lifetimes than Rust.

The compiler itself is written in Rust, and uses as few dependencies as possible to keep the frontend fast and reliable.

<br>
