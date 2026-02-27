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
import @(html/Basic)

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
import @(PostGenerator)
import @(html/Basic)

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
    -- Mutability here means that string_slice_value can be reassigned with another string slice
    string_slice_value ~= "text"
    raw_string_slice ~= `hi`
    
    char ~= '😊'
    
    -- Owned mutable string
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

### Loops in Beanstalk

Beanstalk uses a **single** loop keyword: `loop`.

* **`to` / `upto`** select range semantics (exclusive vs inclusive)
* **Range loops yield the counter**
* **Collection loops yield elements**
* **No enumeration syntax yet**
* **No `reverse` keyword**; direction inferred from bounds
* **`by`** controls step size and works with both directions

Loops come in two forms:

1. **Conditional loops** (repeat while a condition is true)

```beanstalk
loop is_connected():
    io("still connected")
;
```

2. **Iteration loops** (iterate over a collection or a numeric range)

Iteration loops bind a value each iteration and step through either:

* a **collection** (yielding elements), or
* a **range** (yielding a counter)

```beanstalk
loop item in items:
    io(item.to_string())
;
```

The form is determined entirely by the loop header:

* If the header contains **`to`** or **`upto`**, it is a **range loop**.
* Otherwise, it is treated as a **conditional loop**.

* `to` for **exclusive** end bounds
* `upto` for **inclusive** end bounds

```beanstalk
loop i in 0 to 10:
    io(i.to_string())
;
-- yields: 0, 1, 2, 3, 4, 5, 6, 7, 8, 9

loop i in 0 upto 10:
    io(i.to_string())
;
-- yields: 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10
```

You can specify a step using `by`.

```beanstalk
loop i in 0 to 10 by 2:
    io(i.to_string())
;
-- yields: 0, 2, 4, 6, 8
```

### Direction is inferred from bounds

Beanstalk automatically determines the iteration direction from the bounds:

* If `start < end`, the default direction is ascending.
* If `start > end`, the default direction is descending.

With no `by`, the default step is `+1` for ascending ranges and `-1` for descending ranges.

```beanstalk
loop i in 10 to 0:
    io(i.to_string())
;
-- yields: 10, 9, 8, 7, 6, 5, 4, 3, 2, 1
```

You can also supply an explicit step:

```beanstalk
loop i in 10 upto 0 by 2:
    io(i.to_string())
;
-- yields: 10, 8, 6, 4, 2, 0
```

- When bounds imply descending iteration, `by` is treated as a magnitude (the compiler will apply the correct sign based on direction).
- A step of `0` is invalid.

Float ranges are supported, but **`by` should be considered required** to prevent ambiguous or non-terminating loops.

```beanstalk
loop t in 0.0 to 1.0 by 0.1:
    io(t.to_string())
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

At the root of every project is a `#config.bst` file.
`#config.bst` uses normal Beanstalk declaration syntax. Stage 0 reads top-level constant declarations from it.
Legacy shorthand config declarations like `#project "html"` are invalid.

Example:
```beanstalk
#project = "html"
#src = "src"
#output_folder = "dist"
#libraries = {"src/libs"}
```

**Import syntax:**
```beanstalk
-- Import a file start function as a callable alias:
import @(path/to/file)

-- Import one exported symbol:
import @(path/to/file/symbol)

-- Import multiple exported symbols:
import @(path/to/file/{symbol_a, symbol_b})
```

**Entry files and implicit main functions:**
- Every Beanstalk file has an **implicit start function** containing all top-level code
- The **entry file** selected for a module (for example `#page.bst`) has its implicit start chosen as that module's entry start function. Only that file's top-level code executes automatically
- Imported files' implicit start functions are callable but do not execute automatically

**File execution semantics:**
```beanstalk
-- main.bst (entry file)
import @(utils/helper)
import @(utils/helper/another_func)

-- This executes automatically when the module starts
io("Starting main")
helper()  -- Call imported file's implicit start function

another_func() -- Call imported exported symbol directly
```

**Import resolution rules:**
- Relative imports (`@(./x)` / `@(..)`) resolve from the importing file's directory
- Non-relative imports resolve from the configured module source root first, then configured library roots
- Grouped imports are expanded into individual dependency edges
- Circular imports are detected and cause compilation errors

### Hash (`#`) semantics

At top level, `#` changes behavior by declaration kind:
- Variable declaration: exported constant declaration (compile-time only)
- Function declaration: exported function (visibility only)
- Struct or choice declaration: exported type/symbol (visibility only)
- Template head (`#[...]`): top-level const template declaration

Non-`#` top-level declarations are module-private.

```beanstalk
-- Exported constants
#name = "Beanstalk"
#pi = 0.1 + 0.2

-- Exported function
#foo |...| -> T: 
    ... 
;

-- Exported struct/type
#MyStruct = | ... |
```

### Constants and compile-time folding

Constants use the same declaration syntax as regular variables, including optional explicit type annotations.
The difference is semantic: top-level `#` variable declarations are module constants.

Constant rules:
- Must be initialized
- Cannot be mutable
- May only reference constants (non-constant references are compile errors)
- Must fully fold at compile time (runtime expressions are compile errors)
- Same-file constant evaluation follows source order
- Cross-file constant dependencies are resolved in dependency order so constants can reference constants from imported files

```beanstalk
#site_name String = "Beanstalk"
#major_version Int = 1
#full_name = [: [site_name] v[major_version]]
```

### Struct instances in constants (const records)

Struct instances can be coerced into compile-time records when assigned to a constant.
All constructor arguments must also be constant-foldable values.

```beanstalk
Basic = | defaults String |
#values = Basic("Only allowed const values here")
```

`values` has type `#Basic` and is data-only.

### Start fragments and the builder interface

Project builders are aware of:

- the entry start function
- an ordered `start_fragments` stream
- `module_constants` metadata in HIR
- backend output (for example JS bundle)

`start_fragments` interleave:

* compile-time strings (`ConstString`)
* runtime fragment functions (`RuntimeStringFn`)

Builders **do not** consume arbitrary exports directly. They consume the ordered fragments and decide how to materialize output for their target.

Exported constants exist so that **templates can reference them** and remain guaranteed-foldable.
They are also useful for constant data that wants to be shared module wide.

Example:

```beanstalk
#head_defaults = [:
  <meta charset="UTF-8">
]

-- `#[...]` is a top-level const template.
-- Top-level const templates are entry-file only.
-- They must fully fold at compile time.
-- Captures must be constant-only.
-- Slots are allowed if their resolved content is constant.
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

## Key Differences from Rust
| Aspect | Rust | Beanstalk |
|--------|------|-----------|
| Borrow syntax | `&x`, `&mut x` | `x` (shared), `x ~=` (mut) |
| Default semantics | Move | Borrow |
| Explicit operations | Borrow | Mutability/Move |
| Copy behavior | Implicit for Copy types | Always explicit |
