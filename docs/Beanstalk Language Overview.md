# Beanstalk Language Design Guide
Beanstalk is a programming language and build system with minimal syntax and a simple type system.

It is designed primarily for use building UIs and text-heavy content for the Web and other host environments.

The design principles are:
- Very powerful and flexible string templates for rendering content or describing UI
- Very minimal and consistent syntax. Simple to learn and reason about
- Fast compile times for hot reloading dev builds that can give quick feedback for UI heavy projects
- Memory Safe with fallback GC that can eventally be statically optimised out with borrow checker rules
- Strict, statically typed and opinionated about doing things in as few ways as possible as cleanly as possible

```beanstalk
@(html/Basic)

-- Create a new blog post
create_post |title String, date Int, content String| -> String:
    
    io("Creating a blog post!")

    formatted_blog = [Basic.section:
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

The HTML build system will generate an HTML page from this code:
```beanstalk
@(PostGenerator)
@(html/Basic)

date = 2025
post = PostGenerator.create_post(date, [:
    I have absolutely nothing interesting to say, and never will.
])

[Basic.page:
    [Basic.pad(3), post]
]
```

## Syntax Summary
For developers coming from most other languages, 
here are some key idiosyncrasies:

- Colon opens a scope, semicolon closes it. Semicolon does not end statements!
- Square brackets are NOT used for arrays, curly braces are used instead. 
Square brackets are only used for string templates.
- Equality and other logical operators use keywords like "is" and "not" 
(you can't use == or ! for example)
- ~ tilde symbol to indicate mutability (mutability must be explicit). 
This comes before the type if there is an explicit type declaration.
- Double dashes for single line comments (--)
- Immutable reference semantics are the default for all stack and heap allocated types. 
- All copies have to be explicit unless they are used in part of a new expression. Including integers, floats and bools.
- Parameters and struct definitions use vertical pipes | 
- Result types are created with the '!' symbol. Options use '?'.

**Naming conventions (strictly enforced):**
- Types/Objects/Choices: `PascalCase`
- Variables/functions: `regular_snake_case`

## Core Syntax Patterns
```beanstalk
    int ~= 0
    float ~= 0.0

    -- Note: This is not a mutable heap allocated string, just a slice
    -- Regular strings are created using string templates
    string_slice_value ~= "text"
    char ~= '😊'
    raw_string_slice ~= `hi`

    string_template ~= [:
        This is the Beanstalk programming language
    ]

    bool ~= true

    mutable_collection ~= {}
    immutable_collection = {}

    Struct = |
        value Float,
        another_value String,
    |
    
    instance ~= Struct(
        value = 1.2, 
        another_value = "hey"
    )

    -- Notice the double colon
    Choice :: 
        Option1,
        Option2 String,
        Option3 |
            inner_value String,
            another_value Float,
        |,
    ;
```

**Function definitions:**

```beanstalk
-- Basic function pattern
function_name |param Int| -> Int:
    -- 4-space indentation
    value = param + 1
    return value
;
```

**Collections**

When a new collection uses the mutable symbol, its internal values can be mutated by default.

Instead of accessing elements directly, 
all collections have built-in methods for accessing, mutating, pushing or removing elements. Collections are ordered groups of values that are zero-indexed. 

Elements inside collections are accessed using the .get() method.

array.get(0) is the equivalent of array[0] in most C like languages. 
There is no square or curly brackets notation.

There may not be a function call under the hood when using collection methods, 
as the compiler abstracts these to be direct accesses in many cases.


**Output and printing:**
```beanstalk
-- Print to stdout using io() function
io("Hello, World!")

-- Print with variables
message = "Hello"
io(message)

-- Print with interpolation using templates
name = "Alice"
io([: Hello, [name]!])

-- Print in functions
greet |name String|:
    io(Hi [name])
;
```

**Control flow patterns:**

There is no 'else if', you use pattern matching instead for more than two branches. Pattern matching is exhaustive, if statements are not.
```beanstalk
-- Conditional (use 'is', never ==)
if value is not 0:
    -- code
else
    -- code
;
```

Only one keyword for loops "loop". 

Using the "in" keyword, you can specify an integer, float or collection to iterate through or define a new integer, float or collection. 

```beanstalk
    loop thing in things:
        io(thing)
    ;

    loop -20 to big_number:
        io("hello")
    ;

    -- reverse loop
    loop n in big_number to smaller_number:
        io(n)
    ;

    loop item in collection:
        io(item.to_string())
    ;

    -- Getting the index
    loop item, index in collection:
        io(index.to_string())
    ;
```

**Structs**
```beanstalk
    -- To create a new instance of this struct, it must have 2 parameters passed in,
    -- a string and an integer
    Person = |
        name String,
        age Int,
    |

    -- Create a new instance of the struct
    person ~= Person("Alice", 30)

    -- Access fields using dot notation
    io(person.name) -- "Alice"
    io(person.age)  -- 30

    -- Defining a struct, then defining a method for it
    -- This will be dynamically dispatched
    Vector2 = |
        x Float,
        y Float,
    |

    reset |vec ~Vector2|:
        vec.x = 0
        vec.y = 0
    ;

    vec = Vector2(12, 87)
    vec.reset()
```

## Module System and Imports

**Multi-file modules**

A module is multiple Beanstalk files compiled together into a single output. Each module will have its own entry point.

A project is one or more of these modules together with libraries and sometimes other file types that is all compiled together into a more complex output.

At the root of every project is a #config.bst file.

**Import syntax:**
```beanstalk
-- Import another file in the same module
@(path/to/file)
```

**Entry files and implicit main functions:**
- Every Beanstalk file has an **implicit start function** containing all top-level code
- The **entry file** (specified during compilation) has its implicit start become the module's entry point (the main function). Only the entry file's top-level code executes automatically
- Imported files' implicit mains are callable but don't execute automatically.

**File execution semantics:**
```beanstalk
-- main.bst (entry file)
@(utils/helper)

-- This executes automatically when the module starts
io("Starting main")
helper.start()  -- Call imported file's implicit start function

helper.another_func() -- Call another top level function from the file
```

**Import resolution rules:**
- Paths are relative to the root of the module, defined by the directory the entry point file is in or a config file
- Circular imports are detected and cause compilation errors

### Exports, const coercion, and compile-time folding

`#` **at the top level** means **exported from the module**.
Non-`#` top-level declarations are private. 
By design, only **functions** and **struct/type declarations** are allowed as runtime exports.
Regular runtime variables are **not exportable**. 
Top-level runtime temporaries are treated as part of `start()`.

There are two kinds of exported symbols:
- Exported constant binding (const coercion + export)
- Exported runtime symbols (visibility only)

```beanstalk
-- Constant binding
#name = "Beanstalk"
#pi = 0.1 + 0.2

-- Runtime function exports
#foo |...| -> T: 
    ... 
;

-- Struct Export
#MyStruct = | ... |

```

Constants can't capture variables, and must fully fold down to a single known value or the compiler will throw a compile error.

**Struct instance coerced to const record**

Structs can be coersed into const records when assigned to a constant. 
When creating the instance of the record, all the parameters must also be constants.

```beanstalk
Basic = | defaults String |
#values = Basic("Only allowed const values here")
```

`values` has type `#Basic` and is data-only.

### Compile-time templates and the builder interface

Project builders are only aware of:

* `start() -> String`
* top-level compile-time templates (`#[ ... ]` and labeled variants)

Builders **do not** consume arbitrary exports directly, they know about the string returned from the start function at runtime and top-level constant templates.

Exported constants exist so that **templates can reference them** and remain guaranteed-foldable.
They are also useful for constant data that wants to be shared module wide.

Example:

```beanstalk
#head_defaults = [:
  <meta charset="UTF-8">
]

-- Only the outer (top level) template can have a `#` to indicate this is a compile time only template.
-- Anything passed into the template head, or any child templates must also be const.
#[html.head: [head_defaults]]
```

## Template System
**Templates use `[]` exclusively** - never confuse with collections `{}`.

Templates are either folded to strings at compile time, or become functions that return strings at runtime. 
They are the ONLY way to create mutable strings in Beanstalk. "" are only for string slices.

**Template structure:**
- Head and body separated by `:`
- Variable capture from the surrounding scope

Templates unlock the full power of Beanstalk's HTML / CSS generation capabilities.
You can use slots and special Style structs to determine how the templates are constructed.
They can be used to build complex HTML pages with minimal boilerplate.
