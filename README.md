<div align="center">

# Beanstalk

<p><em>
  A minimal, statically typed programming language for creating fast and safe web apps
</em></p>

# 🌱

<p>⚠️ This is currently a work in progress compiler. See CONTRIBUTING.md if you're inspired to help out</p>
</div>

## HTML project Snippets

(For illustrative purposes, the build system and compiler is still missing features to compile all of this correctly)
```haskell
#import "html/Basic"

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
#import "PostGenerator"

date = 2025
post = PostGenerator.create_post(date, [:
    I have absolutely nothing interesting to say, and never will.
])

[post]
```

<br>

## Goals 
- The minimalism and fast compile speed of Go.
- The memory safety of Rust, with no 'unsafe' mode.
- Predictable performance
- **The King of Strings**: A string template system can double up as a tiny HTML/markup engine or anything else you want.
- A full static-site–capable build system.
- Simple and strong type system
- Wasm focused backend designed around producing high-quality Wasm efficiently and integrating with that ecosystem




[//]: # (  <h1>)

[//]: # (    <a href="https://nyejames.github.io/beanstalk">)

[//]: # (      Plans and Documentation)

[//]: # (    </a>)

[//]: # (  </h1>)

[//]: # (  <p>The docs were created using this language. The output of the compiler is directly pushed to GitHub pages. Not everything in the documentation has been implemented fully, it's mostly full of design plans.</p>)

See `docs/Beanstalk Language Overview.md` and `docs/Beanstalk Compiler Development Guide.md` for more in depth details.

<h2>Tools</h2>
<a href="https://github.com/nyejames/beanstalk-plugin">Language support / syntax highlighting for Visual Studio / VSCode can be found here</a>

<br>
<br>
