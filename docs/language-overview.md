# Beanstalk Language Design Guide
Beanstalk is a programming language and build system designed for modern UI driven apps and webpages.

The design principles are:
- Powerful and flexible string templates for rendering content or describing UI
- Minimal and consistent syntax. Simple to learn and reason about
- Fast compile times for hot reloading dev builds that can give quick feedback for UI heavy projects
- Memory Safe with fallback GC that can eventually be statically optimised out with borrow checker rules
- Strict, statically typed and opinionated about doing things in as few ways as possible as concisely as possible

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

**Naming conventions:**
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
    
    instance ~= Struct(1.2, "hey")

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

### Results and Options

Beanstalk supports optional values and error returns with compact syntax.

Optional types use `?`:

```beanstalk
name String? = none

find_name |id String| -> String?:
    if id.is_empty():
        return none
    ;

    return "Alice"
;
```

A normal value of type `T` can be used where `T?` is expected.  
`none` is the only special option value.

Error-returning functions mark one return slot with `!`:

`Error` is a builtin language type with this default shape:

- `kind ErrorKind`
- `code String`
- `message String`
- `location ErrorLocation?`
- `trace {StackFrame}?`

`Error`, `ErrorKind`, `ErrorLocation`, and `StackFrame` are reserved builtin symbols and cannot be re-declared by user code.

```beanstalk
parse_number |text String| -> Int, Error!:
    if text.is_empty():
        return! Error("Parse", "int.parse_invalid_format", "Missing number")
    ;

    return 42
;
```

Error-returning calls must be handled at the call site.

Bubble the error up:

```beanstalk
value = parse_number(text)!
```

Or provide fallback values:

```beanstalk
value = parse_number(text) ! 0
```

Multiple success values use the normal return list and a shared assignment on the caller side:

```beanstalk
pair || -> String, Int:
    return "Ana", 2
;

name, count = pair()
```

Named handler scopes are supported for explicit error-handling blocks, including fallback values
when the success path still needs values:

```beanstalk
name, score = load_user(id) err! "guest", 0.0:
    io(err.message)
;
```

Beanstalk still uses multiple returns, so the success path keeps normal return values.  
The special `!` return is only for the error path.

**Collections**

When a new collection uses the mutable symbol, its internal values can be mutated by default.

Instead of direct index syntax, ordered collections use compiler-owned built-ins:
`get`, `set`, `push`, `remove`, and `length`.

Collections are ordered groups of values that are zero-indexed.

`collection.get(index)` returns `Result<Elem, Error>`, so value-position reads must be
handled with `!` syntax.

Both indexed write forms are supported:

```beanstalk
items.set(0, value)
items.get(0) = value
```

`set` and `get(index) = value` require mutable element semantics.
`push` and `remove` are allowed on immutable collections.

There may not be a runtime call under the hood when using collection methods, because the
compiler can lower these operations directly.

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

## String Template System
**Templates use `[]` exclusively** - never confuse with collections `{}`.

Templates are either folded to strings at compile time, or become functions that return strings at runtime. 
They are the ONLY way to create mutable strings in Beanstalk. "" are only for string slices.

**Template structure:**
- Head and body separated by `:`
- Variable capture from the surrounding scope

Templates unlock the full power of Beanstalk's HTML / CSS generation capabilities.

### Template Styles
Templates can be used to build complex UI components. They can use slots to insert content from other templates and have **style metadata** attached to them.

A template’s style is defined in the **template head** using `$` directives. 
`$` introduces **compiler-handled directives** (so they don’t collide with normal variables and can be extended in the future), such as formatter-like built-ins, precedence controls, and default child templates that are automatically applied to direct child templates.

Directive availability is frontend-registry based:
- Frontend built-ins are available by default (`$markdown`, `$code`, `$raw`, slots, etc.).
- Project builders can register additional project-specific directives using the same `$name` syntax. In the HTML project, that includes `$html`, `$css`, and `$escape_html`.
- Unknown directives fail as syntax/rule errors unless they are registered.

```beanstalk
-- Define a template style
[
  $markdown,                        
  $children([: All children start with this prefix! ])    -- Applies only to direct children
:
  # Hello
  This template is parsed as markdown.

  @example.com (Here is a link!) using this custom markdown flavour.

  [$todo: write some more info!]

  [: This child is prefixed!]
]
```

**Frontend Built-in Style Directives**

- $slot / $insert(..) - See slots below!
- $fresh              - Opts this child template out of wrappers applied by the immediate parent's `$children(..)` directive
- $markdown           - Parses the template bodies with a custom flavour of Markdown
- $code               - Highlights code blocks using the compiler's built-in formatter
- $raw                - Preserves authored template body whitespace exactly
- $note / $todo       - Comments (ignored by final output)
- $doc                - Turns the template into a documentation comment
- $children(..)       - Accepts a template (or string slice) that will be applied only to this template's direct child templates

**HTML Project Directives**

- $html               - Parses the template body as raw HTML (no escaping)
- $css                - Provides some basic warnings for malformed CSS
- $escape_html        - Escapes HTML-sensitive characters in the template body

Formatting directives do not automatically flow into nested child templates.
If a child template should keep using a formatter such as `$markdown`, redeclare it in that child template's head.

`$fresh` is per-child and only affects wrapper application from the immediate parent. Siblings without `$fresh` still receive the parent wrappers:

```beanstalk
# list = [$children([:<li>[$slot]</li>]):
  <ul>
    [$slot]
  </ul>
]

[list:
  [: one ]
  [$fresh: [: two ]]
]
```

In this example, `one` is wrapped with `<li>...</li>`, while `two` opts out and is rendered without the parent `$children(..)` wrapper.

### Template Slots

Template slots let one template receive content from another template. The default slot is written as `[$slot]` and marks where the main body content should be inserted.

Named slots can also be declared with `[$slot("name")]`. These allow helper templates to insert content into a specific part of another template using `$insert("name")`.

Positional slots can be declared using positive integers, such as `[$slot(1)]`, `[$slot(2)]`, etc. Loose contributions (those not explicitly targetting a named slot) are assigned to positional slots first, in ascending numeric order. Any remaining loose contributions go to the default slot if it exists.

```beanstalk
img = [:
    <img src="[$slot(1)]" alt="[$slot]">
]

[img, "logo.png": Site logo]
````

In this example, `"logo.png"` fills the first positional slot `[$slot(1)]`, and `"Site logo"` fills the default slot `[$slot]`.

Named inserts:
```beanstalk
title = [:
    <h1 style="[$slot("style")]">
        [$slot]
    </h1>
]

blue = [$insert("style"): color: blue;]

[title, blue:
    Hello world
]
````

In this example, `blue` inserts `color: blue;` into the `style` slot of `title`, while `Hello world` is inserted into the default slot.

If a template has named or positional slots but no default slot, any loose body content that cannot be assigned to a positional slot is an error. 
If a slot receives no content, it expands to an empty string.
Repeated slots, such as two occurrences of `[$slot(1)]`, will replay the same content in both places.

Because `$children(..)` only applies to direct children, nested helpers can scope row and cell wrappers independently:

```beanstalk
# table = [$children([:<tr>[$slot]</tr>]):
  <table style="[$slot("style")]">
    [$children([:<td>[$slot]</td>]):[$slot]]
  </table>
]

[table:
    [: [: Type] [: Description] ]
    [: [: float ] [: 64 bit floating point number] ]
]
```

In that example, each direct child of `table` becomes a row, while each direct child of a row becomes a cell. The outer `<tr>` wrapper does not leak into the inner cell templates.

**Control flow patterns:**

There is no 'else if', you use pattern matching instead for more than two branches. Pattern matching is exhaustive, if-statements are not.
```beanstalk
-- Conditional (use 'is', never ==)
if value is not 0:
    -- code
else
    -- code
;
```

## Loops

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

## Structs

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

    -- Defining a struct, then defining a receiver method for it
    Vector2 = |
        x Float,
        y Float,
    |

    reset |this ~Vector2|:
        this.x = 0
        this.y = 0
    ;

    vec = Vector2(12, 87)
    vec.reset()
```

Runtime structs are nominal types. Matching field shapes do not make two structs interchangeable.

Receiver methods in v1 are statically resolved:
- A method is a top-level function whose first parameter is literally named `this`
- There may be exactly one `this` parameter
- Supported receiver types are user-defined structs and built-in scalars (`Int`, `Float`, `Bool`, `String`)
- Collection built-ins (`get`, `set`, `push`, `remove`, `length`) are compiler-owned operations and are not declared via `this`
- `this Type` declares an immutable receiver
- `this ~Type` declares a mutable receiver
- Methods are called with receiver syntax only: `value.method(...)`
- `method(value, ...)` is not valid for receiver methods
- Mutable receiver methods require a mutable place receiver, so temporaries and rvalues cannot be mutated through method syntax
- Field writes follow the same mutable-place rule as mutable methods

User-defined struct methods must be declared in the same file as the struct definition. This same-file restriction does not apply to built-in scalar receivers.

Exported receiver methods become available through the receiver type, not as free-function imports.

```beanstalk
double |this Int| -> Int:
    return this + this
;

value = 21
io(value.double()) -- 42
```

## Module System and Imports

**Multi-file modules**

A module is multiple Beanstalk files compiled together into a single output. Each module will have its own entry point.

A project is one or more of these modules together with libraries and sometimes other file types that is all compiled together into a more complex output.

At the root of every project is a `#config.bst` file.
`#config.bst` uses normal Beanstalk declaration syntax. Stage 0 reads top-level constant declarations from it.

Example:
```beanstalk
# project = "html"
# entry_root = "src"
# dev_folder = "dev"
# output_folder = "release"
# root_folders = { @lib, @assets }
```

**Import syntax:**
```beanstalk
-- Import a file start function as a callable alias:
import @path/to/file

-- Import one exported symbol:
import @path/to/file/symbol

-- Grouped relative path expansion from one shared base:
import @path/to/file {symbol_a, symbol_b}

-- Grouped entries can include nested relative paths:
import @docs {
    intro.md,
    guides/getting-started.md,
}
```

**Entry files and implicit start functions:**
- Every Beanstalk file has an **implicit start function** containing all top-level code
- The **entry file** selected for a module (for example `#page.bst`) has its implicit start chosen as that module's entry start function. Only that file's top-level code executes automatically
- Imported files' implicit start functions are callable but don't execute automatically.

**File execution semantics:**
```beanstalk
-- main.bst (entry file)
import @utils/helper
import @utils/helper/another_func

-- This executes automatically when the module starts
io("Starting main")
helper()  -- Call imported file's implicit start function

another_func() -- Call imported exported symbol directly
```

**Import resolution rules:**
- Relative imports (`@./x` / `@..`) resolve from the importing file's directory
- Non-relative imports whose first segment matches `#root_folders` resolve from the project root
- Other non-relative imports resolve from the configured module entry root
- Grouped import paths are expanded into individual dependency edges
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
# name = "Beanstalk"
# pi = 0.1 + 0.2

-- Exported function
# foo |...| -> T: 
    ... 
;

-- Exported struct/type
# MyStruct = | ... |
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
# site_name String = "Beanstalk"
# major_version Int = 1
# full_name = [: [site_name] v[major_version]]
```

### Struct instances in constants (const records)

Struct instances can be coerced into compile-time records when assigned to a constant.
All constructor arguments must also be constant-foldable values.

```beanstalk
Basic = | defaults String |
# values = Basic("Only allowed const values here")
```

`values` has type `#Basic` and is data-only. Const records do not have a runtime method surface, so `values.some_method()` is not valid.

### Start fragments and the builder interface

Project builders are aware of:

- the entry start function
- an ordered `start_fragments` stream
- `module_constants` metadata in HIR
- backend output (for example JS or Wasm bundle)

`start_fragments` interleave:

* compile-time strings (`ConstString`)
* runtime fragment functions (`RuntimeStringFn`)

Builders **do not** consume arbitrary exports directly. They consume the ordered fragments and decide how to materialize output for their target.

Exported constants exist so that **templates can reference them** and remain guaranteed-foldable.
They are also useful for constant data that wants to be shared module wide.

Example:

```beanstalk
# head_defaults = [:
  <meta charset="UTF-8">
]

-- `#[...]` is a top-level const template.
-- Top-level const templates are entry-file only.
-- They must fully fold at compile time.
-- Captures must be constant-only.
-- Slots are allowed if their resolved content is constant.
#[html.head: [head_defaults]]
```
