[Navbar]

[Header center: [title(1): PROJECT SETUP]]
[Page:
## The Compiler
You currently have to compile the Beanstalk compiler yourself. The compiler is written in Rust. This will change once the compiler and language development is more stable.

The compiler can be found on [link("https://github.com/nyejames/beanstalk/"): Github].

## Setting up a Project
Running the command 'new html' followed by your directory path will create a new static HTML Beanstalk project in that directory. The path is relative to the current directory.

*More project types will be added in the future.*

The entry point for a Beanstalk project is configured when creating a new Beanstalk project. 

This file is called **config.bst** and is placed in the root folder of your project by default, this is the entry point for the compiler. 

*All Beanstalk files use the .bst file extension.*

The compiler will read through any configuration settings first.

- src folder contains your bst files and determines the file structure of your pages. 
- dev folder contains the dev output of the compiler.
- release folder contains the release builds.

# Routes
Beanstalk web projects use a directory based router, with a few special naming rules. 

## Comptime files
Prefixing your BS files with a '#' symbol and keyword means it is a special comptime file. The compiler recognises different kinds of compile time files and will handle them differently. 

After the special file name, you can leave a space and name the file whatever you want.

- '#page' is the main page of the folder it is in. This page takes the name of the directory that it's in if there is no name specified after the #page keyword.
- '#layout' automatically wraps all pages in the same directory in a parent layout. Use the 'slot' keyword in a scenehead to specify where the content should be placed. 
- '#error' handles what the server does by default when an error occurs on the server, such as a 404. This will eventually extend to handle all different types of errors.


# Compiling your BS files
### Build
The simpliest way to get the compiler to output some HTML is to use the *build* command.

While in the root directory of your project, where the config.bst file is, run the command **build** to compile your whole project.

[Code:
    $ build ./path_to_project
]

If you specify a specific bs file, build will only compile that specific file and will output it into the same directory.

[Code:
    $ build ./path_to_project/src/index.bst
]

### Dev
To create a development server with a watcher, run the command 'dev' in the root directory of your project. This is the main way to develop your HTML project.

This will create a localhost server that will automatically reload the page when you make changes to your bs files. The command line will give you a link to open in your browser.

]

[Footer]
