# Beanstalk Language Design Guide

Beanstalk is a programming language and build system with minimal syntax and a simple type system.

You can think of the language at a high level as being similar to Go (fast compile times, very minimal and simple), but with a Rust style memory management (instead of a GC) and a unique modern syntax with very powerful string templates.

## Language Syntax Rules
# Quick Syntax Summary
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
- Immutable Reference semantics are the default for all stack and heap allocated types. 
- All copies have to be explicit unless they are used in part of a new expression. Even for primitive types such as integers.
- Errors use the '?' symbol. Options use '?'.

**Naming conventions (strictly enforce):**
- Types/Objects: `PascalCase`
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

    Struct:
        value Float,
        another_value String,
    ;
    
    instance ~= Struct(
        value = 1.2, 
        another_value = "hey"
    )
```

**Function definitions:**
```beanstalk
-- Basic function pattern
function_name |param Type| -> ReturnType:
    -- 4-space indentation
    return value
;

-- Error handling pattern
-- Error can be any type, the bang indicates this is a possible error value
risky_function || -> String, Error!:
    return other_function() !err:
        return "", err
    ;
;
```
**Collections**
When a new collection uses the mutable symbol, its internal values can be mutated by default.

Instead of accessing elements directly, 
all collections have built-in methods for accessing, mutating, pushing or removing elements.

Collections are ordered groups of values that are zero-indexed (start from 0). 

For unordered groups of values with optional keys, use a Hash Map (see below).

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
io([: Hello, name!])

-- Print in functions
greet |name String| -> Void:
    io([: Hello, name!])
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
    0: ["zero"]
    < 0: ["negative"]
    else: ["other"]
;
```

Only 1 keyword for loops "loop". 

Using the "in" keyword, you can specify an integer, float or collection to iterate through or define a new integer, float or collection. 

```beanstalk
    loop thing in things:
        print(thing)
    ;

    loop -20 to big_number:
        print("hello")
    ;

    -- reverse loop
    loop n in big_number to smaller_number:
        print(n)
    ;

    loop item in collection:
        print(item.to_string())
    ;

    -- Getting the index
    loop item, index in collection:
        print(index.to_string())
    ;
```

**Structs**
```beanstalk
    -- Define a new object
    -- To create a new instance of this object, it must have 2 parameters passed in,
    -- a string and an integer
    Person:
        name String,
        age Int,
    ;

    -- Create a new instance of the type
    person ~= Person("Alice", 30)

    -- Access fields using dot notation
    print(person.name) -- "Alice"
    print(person.age)  -- 30

    -- Defining a struct, then defining a method for it
    Vector2:
        x Float,
        y Float,
    ;

    reset |vec ~Vector2|:
        vec.x = 0
        vec.y = 0
    ;

    vec = Vector2(12, 87)
    vec.reset()
```

## Memory Model Overview
Beanstalk uses a Rust style memory management system.
For a more detailed breakdown, see the Beanstalk Compiler Development Guide.

**Memory management:**
- Borrow checker without explicit lifetimes. No unsafe.
- Reference passing by default, `~` for mutable
- Move semantics determined by compiler analysis

**Module memory semantics:**
- Each file's variables are scoped to that file
- Memory safety maintained across module boundaries

## Module System and Imports

**Multi-file modules**: Beanstalk supports organizing code across multiple files that compile into a single WASM module.

Everything at the top level of a file is visible to the rest of the module by default, but must be explicitly exported to be exported from the final wasm module.

Currently imports can't yet be aliased, so the import will just have the same name as the file and can be used like a struct containing all the headers at the top level of the file.

**Import syntax:**
```beanstalk
-- Import another file in the same module
#import("path/to/file")
```

**Entry files and implicit main functions:**
- Every Beanstalk file has an **implicit start function** containing all top-level code
- The **entry file** (specified during compilation) has its implicit start become the module's entry point (the main function). Only the entry file's top-level code executes automatically
- Imported files' implicit mains are callable but don't execute automatically. They must be imported as "start"
- **Single WASM output**: All files in a module compile to one WASM module with proper exports

**File execution semantics:**
```beanstalk
-- main.bst (entry file)
#import "utils/helper"

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

Templates are either folded to strings at compile time, or become functions that return strings at runtime. They are the ONLY way to create mutable strings in Beanstalk. "" are only for string slices.

**Template structure:**
- Head and body separated by `:`
- Variable capture from surrounding scope
- Runtime ID assignment with `@` symbol

**Template patterns:**
```beanstalk
-- Basic template
[: content]

-- With style/struct
-- Unpacks the fields so they can be accessed by the children
-- Styles are structs with special fields that affect how the template is parsed.
-- Styles include a formatter function, which parses all of the template content at compile time. A common use case for this is to use a markdown formatter which will convert the content into HTML from a markdown-like syntax.
[Section: content]

-- With ID for easy runtime access
[@my_id: content]

-- Control flow in templates

-- Only becomes an empty string if false
[if condition: content]

[for item in collection: [item]]
```




