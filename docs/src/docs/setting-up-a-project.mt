[#import(@libs/html/basic)]
[#import(@styles/docs_styles)]
[#import(@./components)]

[docs_styles.Navbar]

[docs_styles.Header, basic.Center: [basic.Title: SETTING UP A PROJECT]]

## The Compiler
You currently have to compile the Beanstalk compiler yourself. 
The compiler is written in Rust. 
This will change once the compiler and language development is more stable.

The compiler can be found on [link("https://github.com/nyejames/beanstalk/"): Github].

## Setting up a Project
TODO: Expand this section as the build system is built out.

This file is called **config.bst** and is placed in the root folder of your project by default, 
this is what the compiler will look for when compiling your project to determine the entry point. 

*All Beanstalk files use the .bst file extension.*

The compiler will read through any configuration settings first.

# Routes
Beanstalk web projects use a directory-based router, with a few special naming rules.

[Footer]
