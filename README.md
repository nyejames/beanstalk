<div align="center">

  <h1>BS 🌱</h1>

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

  <h1>
    <a href="https://nyejames.github.io/beanstalk">
      Plans and Documentation
    </a>
  </h1>

  <p>The docs were created using this language. The output of the compiler is directly pushed to GitHub pages. Not everything in the documentation has been implemented fully, it's mostly full of design plans.</p>
  <a href="https://github.com/nyejames/beanstalk-plugin">Language support / syntax highlighting for Visual Studio / VSCode can be found here</a>

</div>

<br>
<br>

# Overview / Goals
BS is a simple compiled programming language which outputs HTML, CSS and Wasm/JS all in one consistent syntax.

The whole language is built around it's unique markup syntax.

Eventually, the goal is to also be a more general lightweight Wasm based UI building language that could be great for embedding in any application or game.

At it's simplest it can be thought of as Markdown expanded into an entire language designed from the ground up.

The compiler's IR will be Web Assembly Text Format. 

**Design Goals**
- Minimal syntax with a focus around text content / typesetting and styling
- Simple, static type system with some dynamic elements
- Secure. Rust style memory management
- Batteries included with powerful built-in standard library
- Fast compile times to support hot reloading
- Fast for prototyping and refactoring with default/optional values
- Wasm based for fast web apps and for Wasm embedding in projects
- The compiler should provide all the scaffolding and glue to embed Wasm

The language wants to be great for building a website, making config files or as an embedded and UI language for apps and games.

## Scenes
BS's core syntax idea is using scenes, which are declarative and built into an otherwise highly procedural language.

Scenes can be used to write content, styling and basic dynamic logic all in one place.

Scenes provide a template for your styles and content, with the ability to create custom elements and styling.

They can be nested and used as components in other scenes.

**Markdown Built In**
Write content in a simpler dialect of markdown. Images, videos and other media are easy to add and style with a sensible modern CSS starting point.

You can finally center that div with only one keyword 🔥

Use keywords at the start of scenes to define, style and position all your elements.

### Compiled Output
BS aims to just output Wasm and have it's own backend for doing this, but JS is currently the primary output for web while the language is being created.
More JS will gradually get replaced, but only in cases where stricter runtime types or greater performance is needed.

The compiler aims to output as little bytecode/glue code as possible. No 100kb Wasm files to just print "hello world".

Being compiled means folding constants, type checking and optimizing the output to be as small as possible is all done for you.

The built-in hot-reloading development server can be used to see changes in real time. 
The compiler itself is written in Rust, and uses as few dependencies/libraries as possible to keep it fast and reliable.

### Technologies currently used in the compiler
- [wat2wasm](https://github.com/WebAssembly/wabt) for compiling wat to wasm
- [Pico CSS](https://picocss.com/) for the default CSS styling reset (will be replaced with a custom system in the near future)

<br>
