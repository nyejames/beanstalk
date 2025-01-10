<div align="center">

  <h1>Beanstalk 🌱</h1>

  <p>
    <strong>A lightweight language for bringing joy back to building UI, web pages and embedded tasks</strong>
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
      Documentation
    </a>
  </h1>

  <p>The docs were created using this language. The output of the compiler is directly pushed to GitHub pages. Not everything in the documentation has been implemented fully.</p>
  <a href="https://github.com/nyejames/beanstalk-plugin">Language support / syntax highlighting for Visual Studio / VSCode can be found here</a>

</div>

<br>
<br>

# Overview / Goals
Beanstalk is a simple compiled programming language which outputs HTML, CSS and Wasm/JS all in one consistent syntax. Eventually, the goal is to also be a more general lightweight Wasm based UI building language that could be great for embedding in any application or game.

At it's simplest it can be thought of as Markdown expanded into an entire language designed from the ground up.

**Design Goals**
- Easy to learn and minimal
- Consistent or intuitive syntax (particular for designing UIs and contentful pages)
- Simple but powerful static type system that helps avoid bugs without getting in the way of productivity
- Very batteries included with powerful built-in standard library
- Fast compile times (to remove the need for being interpreted)
- Wasm output backend for fast web apps and for Wasm embedding on native projects
- Be the best language for visual and written content related tasks that have some interactivity and dynamic behaviour

With fast compile times and built in hot-reloading, Beanstalk is designed to feel like a scripting language with all the power of being compiled.
The language aims to replace the use cases of both JS / other Wasm compiler targets for the web or Lua in the embedded world.

The language wants to be great for building a website, making config files or as an embedded and UI language for apps and games.

### The tradeoffs
#### Raw performance 
Trade highest possible runtime speeds for faster compile times, Development speed and security (avoid memory bugs).
There may be a good option to vary compile optimisations, so more optimisation passes can be performed for certain builds, 
but the main focus is to have a custom-built backend that uses only inexpensive code optimisations.

This is not a language designed to be used for general purpose programming or low level domains.
The aim is to be a very powerful tool for what it's designed to do, but be more performant and scalable than equivalent languages in those domains like Python or JS.

The goal is to be faster and less dynamic than other high level interpreted languages, without falling into the design trap of being yet another C/C++ replacement
or general purpose programming language that tries to do everything.

The language is aiming to *avoid having a GC* (at least for the most part). 
But there are some plans and ideas to experiment with <a href="https://nyejames.github.io/beanstalk/docs/memory-management"> hybrid memory management strategies </a>
for performance gains / better memory security / predictable performance.

There is also the posibility of using slower arbitary precision numerical types by default instead of floats. But this hasn't been decided yet.

#### Unique syntax
Designed to be as concise and intuitive as possible at the cost of familiarity when coming from other mainstream languages. 

It is also very minimal with as few operators / keywords are in the language as possible. This means the language is less expressive than languages like rust or C++, but has far less to learn and (hopefully) avoids crazy symbol soup expressions. But also there may be more boilerplate code for some more complex algorithmic tasks.

## Scenes
Beanstalk's core syntax idea is using scenes, which are declarative and built into an otherwise highly procedural language.

Scenes can be used to write content, styling and basic dynamic logic all in one place.

Scenes provide a template for your styles and content, with the ability to create custom elements and styling.

They can be nested and used as components in other scenes.

**Markdown Built In**
Write content in a simpler dialect of markdown. Images, videos and other media are easy to add and style with a sensible modern CSS starting point.

You can finally center that div with only one keyword 🔥

Use keywords at the start of scenes to define, style and position all your elements.
The compiler will only create any HTML / CSS / Wasm / JS you've actually used.

### Compiled Output
Beanstalk will eventually use as much Wasm as possible for a backend, but JS is currently the primary output for web.
More JS will gradually get replaced, but only in cases where stricter runtime types or greater performance is needed.

Being compiled means folding constants, type checking and optimizing the output to be as small as possible is all done for you.

The built-in hot-reloading development server can be used to see changes in real time. 
The compiler itself is written in Rust, and uses as few dependencies/libraries as possible to keep it fast and reliable.

### Technologies currently used in the compiler
- [Pico CSS](https://picocss.com/) for the default CSS styling reset (will be replaced with a custom system in the near future)

<br>
