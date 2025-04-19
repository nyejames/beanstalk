<div align="center">

  <h1>Beanstalk 🌱</h1>

  <p>
    <strong>A lightweight language and toolchain for bringing joy back to building UIs, web pages and typesetting</strong>
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
Beanstalk is a lightweight, statically typed and compiled programming language.

The whole language is built around special template strings called 'Scenes'.

The goal is to also be a more general lightweight Wasm based UI building language that could be great for embedding in any application or game.
No LLVM backend.

HTML projects will use JS, CS and HTML to build websites and tie into the Wasm.

The compiler will also give you all the tools you need to quickly start working on projects.

**Design Goals**
- Minimal syntax with a focus around text content / typesetting and styling
- Simple, static type system with some dynamic casting for strings
- Batteries included with powerful built-in standard library
- Fast compile times to support hot reloading
- Fast for prototyping and refactoring with default/optional values
- Eventually Wasm based output

The language aims to be great for building a website, making config files or as a lightweight embedded or UI language for apps and games.

### Compiled Output
Beanstalk aims to just output Wasm and have its own backend for doing this, but JS is currently the primary output for web while the language is being created.
More JS will gradually get replaced, but only in cases where stricter runtime types or greater performance is needed.

The compiler aims to output as little bytecode/glue code as possible. No 100kb Wasm files to just print "hello world".

Being compiled means folding constants, type checking and optimizing the output to be as small as possible is all done for you.
 
The compiler itself is written in Rust, and uses as few dependencies/libraries as possible to keep it fast and reliable.

### Dependencies currently used in the compiler
- [wat2wasm](https://github.com/WebAssembly/wabt) for compiling wat to Wasm

<br>
