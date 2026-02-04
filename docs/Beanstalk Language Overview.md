# Beanstalk Language Design Guide
Beanstalk is a programming language and build system with minimal syntax and a simple type system.

Beanstalk is a programming language and build system with minimal syntax and a simple type system. 
It is designed primarily for use building UIs and text-heavy content for the Web and other host environments.

You can think of the language at a high level as being like a blend of Go and Rust. 
Fast compile times, very minimal and simple like Go, but with a unique style of automatic memory management, and a unique modern syntax with very powerful string templates.

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
here are some key idiosyncrasies from other C-like languages to note:

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
- Result types are created with the '!' symbol. Options use '?'.

**Naming conventions (strictly enforced):**
- Types/Objects/Choices: `PascalCase`
- Variables/functions: `regular_snake_case`
- Constants: `UPPER_SNAKE_CASE`

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
### Handling Errors in Beanstalk
Errors are treated as values in Beanstalk, and
they represent Result types similar to Rust.

Any function that can return an error must have its error handled.

The bang symbol ! is used for creating Result types and handling errors.

```beanstalk
    func_call_that_can_return_an_error() Int!:
        -- Error handling code
    ;

    -- Here, we define a type called 'BadStuff' that we will use as our error value.
    BadStuff = |
        msg String
    |

    -- This function can return a String and an Int or a BadStuff error
    -- The ! indicates that instead of the normal return values, the error value could be returned instead of the other two
    -- Using return! returns only the error value
    -- The regular return doesn't return the error value
    -- Only one value can use the ! symbol to represent the error value
    parent_func || -> String, Int, BadStuff!:
        text = func_call_that_can_return_an_error() !err:
            io("Error: ", err)
            return! BadStuff(err.msg)
        ;

        return text, 42
    ;
    
    text, number = parent_func() !err:
        io("Error from parent_func: ", err.msg)
        return
    ;

    -- Handling an error with default values
    string_returned, number_returned = parent_func() !("", 0)

    -- Bubbling up errors without handling them
    another_parent_func || -> String, Int, BadStuff!:
        -- Since this function has the same return signature, 
        -- it can be directly returned without handling the error here
        return parent_func()
    ;

    -- By default, accessing items in a collection by their index using .get() returns an error if the index is out of bounds
    -- Unlike other languages where this can cause a panic or exception by default

    my_list ~= [1, 2, 3]

    -- If you wanted to both open a scope for handling the error and provide a default value, you could do it like this:
    -- Default to the last index, and also print an error message
    value ~= my_list.get(5) !(
        my_list.length() - 1
    ) !msg:
        io("Index out of bounds error: ", msg)
    ;
```

## Using the ? operator
```beanstalk

    -- Using the Option type (?) we can represent that a value might not exist
    -- This function returns a string or None
    getURL || -> String?:
        response = getNetworkRequest()

        if response is None:
            return None
        ;

        return response.body
    ;

    -- We can use some ? syntax sugar to set a default value if the value is None
    url = getURL() ?("https://nyejames.com")

    -- This function always returns a Response, as we've handled the None case inside the function
    getURL || -> Response:
        return getNetworkRequest() ?(
            Response("Default Body")
        )
    ;

    -- Ignoring the error syntax sugar and returning errors with the values
    -- Notice, not using the ! 
    -- Instead, we will use an option '?' to represent that the error could be None
    -- This is equivalent to the pattern used for error handling in Go
    parent_func_no_sugar || -> String, Int, BadStuff?:
        text, number = func_call_that_can_return_an_error() !:
            return "", 0, BadStuff("An error occurred")
        ;

        return text, number, None
    ;

    -- We are just treating the error as an optional here instead
    -- So we can seperately check if there was an error
    text, number, error? = parent_func_no_sugar()

    if error is not None:
        io("Error from parent_func_no_sugar: ", error.msg)
        return
    ;
```

### Panics
Panics use a compiler directive.
```beanstalk
    #panic
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
    io([: Hello, [name]!])
;
```

Every host environment must provide an io function to compile Beanstalk.

**Control flow patterns:**

There is no 'else if', you use pattern matching instead for more than 2 branches.
```beanstalk
-- Conditional (use 'is', never ==)
if value is not 0:
    -- code
else
    -- code
;

-- Pattern matching (always exhaustive)
if value is:
    0: return "zero";
    < 0: return "negative";
    else: return "other";
;
```

Only 1 keyword for loops "loop". 

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

Beanstalk supports organizing code across multiple files.

Everything at the top level of a file is visible to the rest of the module by default, but must be explicitly exported to be exported from the final wasm output.

Imports can't yet be aliased, so the import will just have the same name as the file and can be used like a struct containing all the headers at the top level of the file.

**Import syntax:**
```beanstalk
-- Import another file in the same module
import @path/to/file
```

**Entry files and implicit main functions:**
- Every Beanstalk file has an **implicit start function** containing all top-level code
- The **entry file** (specified during compilation) has its implicit start become the module's entry point (the main function). Only the entry file's top-level code executes automatically
- Imported files' implicit mains are callable but don't execute automatically. They must be imported as "start"
- **Single WASM output**: All files in a module compile to one WASM module with proper exports

**File execution semantics:**
```beanstalk
-- main.bst (entry file)
import @utils/helper

-- This executes automatically when the module starts
io("Starting main")
helper.start()  -- Call imported file's implicit start function

helper.another_func() -- Call another top level function from the file
```

**Import resolution rules:**
- Paths are relative to the root of the module, defined by the directory the entry point file is in or a config file
- Circular imports are detected and cause compilation errors
- Each file can only be imported once per module

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
