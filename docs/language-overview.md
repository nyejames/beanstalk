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

    -- You could also create a float with an explicit Type like this:
    float Float ~= 0

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

### Function Call Arguments

Named call arguments use `parameter = value`. Access mode is chosen at the call site.

```beanstalk
sum(values)                 -- positional shared
sum(~values)                -- positional mutable/exclusive
sum(items = values)         -- named shared
sum(items = ~values)        -- named mutable/exclusive
```

Rules:

* Function-call mutability is explicit at the call site.
* A parameter declared as `~T` accepts either:
  * `~place` for mutable/exclusive access to an existing place, or
  * a plain fresh rvalue (literal, template, constructor call, computed value).
* Passing an existing place to a mutable/exclusive parameter without `~` is an error.
* `~` is place-only syntax. Using `~` on an immutable binding, literal, temporary, or computed expression is an error; pass fresh values without `~`.
* Collections follow the same rule. Mutating collection operations do not get a permissive exception.
* Positional arguments must come before named arguments.
* No positional arguments are allowed after the first named argument.
* Each parameter can be provided only once.
* Host function calls and builtin member calls are currently positional-only.

Variable mutability declarations and call-site mutable access are separate concepts:

* `value ~= ...` declares or reassigns a mutable binding.
* `fn(~value)` or `fn(param = ~value)` requests mutable/exclusive access for one specific call argument.
* A mutable binding does not automatically satisfy a mutable parameter. Existing places still require `~` at the call site.

### Numeric Semantics
- Whole-number literals are `Int`.
- Decimal-point literals are `Float`.
- `+`, `-`, `*`, and `%` preserve `Int` when both operands are `Int`.
- `/` is always real division. `Int / Int` evaluates to `Float`.
- `//` is integer division. It currently requires `Int // Int` and evaluates to `Int`.
- Integer division uses truncation toward zero (`-5 // 2` -> `-2`).
- Mixed `Int`/`Float` arithmetic for regular operators evaluates to `Float`.
- There is no implicit `Float -> Int` coercion.
- Use `//` for integer division.
- Use `Int(...)` when an explicit conversion is required.

### Results and Options
Beanstalk supports optional values and error returns with compact syntax.
none is parse-context-sensitive: it requires an optional surrounding type context rather than being recovered later by post-parse coercion.

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

Multi-bind is currently intended for multi-return function-call results only. Regular declarations
remain single-target, and other multi-value expression blocks are not supported yet.

Named handler scopes are supported for explicit error-handling blocks, including fallback values
when the success path still needs values:

```beanstalk
name, score = load_user(id) err! "guest", 0.0:
    io(err.message)
;
```

Beanstalk still uses multiple returns, so the success path keeps normal return values.  
The special `!` return is only for the error path.

### Collections
Collections are ordered groups of values that are zero-indexed.

When a new collection uses the mutable symbol, its internal values can be mutated by default.

`set`, `push`, and `remove` are mutating collection operations and require explicit mutable/exclusive receiver access at the call site.
`get(index) = value` is also a mutating write and therefore requires a mutable place target.
Collections do not get a permissive exception: mutating collection operations follow the same explicit call-site mutability rules as user-defined mutable parameters.

```beanstalk
items ~= {10, 20, 30}
~items.push(40)
~items.set(0, 99)
~items.remove(1)
```

`collection.get(index)` returns `Result<Elem, Error>`, so value-position reads must be
handled with `!` syntax.

`push`, `remove`, and `length` enforce the same strict runtime contracts as `get`.
Invalid receivers or out-of-bounds indices produce structured errors rather than
silent no-ops; the backend handles propagation automatically for these methods.

Both indexed write forms are supported:

```beanstalk
~items.set(0, value)
~items.get(0) = value
```

`set` and `get(index) = value` require mutable element semantics.

There may not be a runtime call under the hood when using collection methods, because the
compiler can lower these operations directly.

### Standard Output
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

Template structure:
- Head and body separated by `:`
- Variable capture from the surrounding scope

Templates unlock the full power of Beanstalk's HTML / CSS generation capabilities.

### Template Styles
Templates can be used to build complex UI components. They can use slots to insert content from other templates and have **style metadata** attached to them.

In the HTML project builder, only direct top-level template expressions in the entry file contribute page fragments.
Top-level runtime templates are evaluated by the entry `start()` function in source order, while top-level const templates are folded at compile time and merged separately.
Templates assigned to variables or returned from functions do not contribute page fragments by themselves.

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

For `$children(..)` template arguments, the child template must close with `]` before the directive closes with `)`.

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

### If Statements / Pattern Matching
If statements are non-exhaustive and don't have 'else if'.
The else branch is optional.

```beanstalk
if value is true:
    io("then branch")
else
    io("else branch")
;
```

Pattern matching is exhaustive and allows you to uses `if <value> is:` with one or more `case` arms.

```beanstalk
value ~= 2
allow ~= false
result ~= "unset"

if value is:
    case 1 => result = "one"
    case 2 if allow => result = "guarded-two"
    case 2 => result = "two"
    else => result = "fallback"
;
```

- Arms are delimited by the next `case`, `else`, or the final match-closing `;`
- Per-arm semicolons are invalid in match blocks
- Guard expressions (`if <bool_expr>`) must be `Bool`
- For non-choice scrutinees, `else =>` is required in Alpha
- For choice scrutinees:
  - `else =>` always satisfies exhaustiveness
  - Without `else =>`, every variant must be covered
  - If any arm has a guard, `else =>` is required
  - The same variant cannot be matched more than once

Arm syntax:
- `case <pattern> => <body>`
- `case <pattern> if <bool_expr> => <body>`
- `else => <body>`

## Loops
Beanstalk uses a **single** loop keyword: `loop`.

* **`to`** / **`to &`** select range semantics (exclusive vs inclusive)
* **Range loops can bind the current counter**
* **Collection loops can bind the current item**
* **An optional second binding provides the zero-based index**
* **No `reverse` keyword**; direction is inferred from the bounds
* **`by`** controls step size and works with both directions
* **Loop bindings use `|...|` and come after the loop condition**
* **The binding brackets are optional** when the current item / counter is not needed

Loops come in two forms:

1. **Conditional loops** (repeat while a condition is true)
2. **Iteration loops** (iterate over a collection or a numeric range)

### Conditional loops
A conditional loop repeats for as long as its condition stays true.

```beanstalk
loop is_connected():
    io("still connected")
;
```

Conditional loops usually do not need bindings, so the parameter brackets are normally omitted.

### Iteration loops
Iteration loops evaluate the loop condition as an iterable source and optionally bind values for each iteration.

Iteration can step through either:

* a **collection** (yielding the current item), or
* a **range** (yielding the current counter)

The loop bindings are written after the loop condition using `|...|`.

```beanstalk
loop items |item|:
    io(item.to_string())
;
```

A second binding can be added to receive the current zero-based index:

```beanstalk
loop items |item, index|:
    io([index]: [item])
;
```

If the current item or counter is not needed, omit bindings entirely:

```beanstalk
loop items:
    io("next item")
;
```

### Range loops
If the loop header contains **`to`**, it is a range loop.

* `to` uses an **exclusive** end bound
* `to &` uses an **inclusive** end bound

You can omit the leading `0` before `to` as sugar:

```beanstalk
loop to 10 |i|:
    io(i.to_string())
;
-- equivalent to: loop 0 to 10 |i|:

loop to & 10 |i|:
    io(i.to_string())
;
-- equivalent to: loop 0 to & 10 |i|:
```

```beanstalk
loop 0 to 10 |i|:
    io(i.to_string())
;
-- yields: 0, 1, 2, 3, 4, 5, 6, 7, 8, 9

loop 0 to & 10 |i|:
    io(i.to_string())
;
-- yields: 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10
```

You can specify a step using `by`.

```beanstalk
loop 0 to 10 by 2 |i|:
    io(i.to_string())
;
-- yields: 0, 2, 4, 6, 8
```

The binding is optional here too:

```beanstalk
loop 0 to 3:
    io("tick")
;
```

### Direction is inferred from bounds
Beanstalk automatically determines the iteration direction from the bounds:

* If `start < end`, the default direction is ascending
* If `start > end`, the default direction is descending

With no `by`, the default step is `+1` for ascending ranges and `-1` for descending ranges.

```beanstalk
loop 10 to 0 |i|:
    io(i.to_string())
;
-- yields: 10, 9, 8, 7, 6, 5, 4, 3, 2, 1
```

You can also supply an explicit step:

```beanstalk
loop 10 to & 0 by 2 |i|:
    io(i.to_string())
;
-- yields: 10, 8, 6, 4, 2, 0
```

* When the bounds imply descending iteration, `by` is treated as a magnitude and the compiler applies the correct sign automatically.
* A step of `0` is invalid.

Float ranges are supported, but **`by` should be considered required** to avoid ambiguous or non-terminating loops.

```beanstalk
loop 0.0 to 1.0 by 0.1 |t|:
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

    vec ~= Vector2(x = 12, y = 87)
    ~vec.reset()
```

Runtime structs are nominal types. Matching field shapes do not make two structs interchangeable.

Receiver methods in v1 are statically resolved:
- A method is a top-level function whose first parameter is literally named `this`
- There may be exactly one `this` parameter
- Supported receiver types are user-defined structs and built-in scalars (`Int`, `Float`, `Bool`, `String`)
- Collection built-ins (`get`, `set`, `push`, `remove`, `length`) are compiler-owned operations and are not declared via `this`
- `this Type` declares an immutable receiver
- `this ~Type` declares a mutable receiver
- Mutable receiver calls must spell mutable/exclusive access at the receiver site; a mutable binding alone is not enough.
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
-- Import one exported symbol:
import @path/to/file/symbol

-- Import several exported symbols from one shared file path:
import @path/to/file {symbol_a, symbol_b}

-- Grouped entries can include nested relative symbol paths:
import @docs {
    pages/home/render,
    pages/about/render,
}
```

Imports target exported symbols, not file-level start functions.
Bare file imports such as import @path/to/file are invalid.

**Entry files and implicit start functions:**
- The module entry file has an implicit `start` function containing its top-level runtime code.
- Only the entry file executes top-level runtime code automatically.
- Non-entry files may contain imports and top-level declarations, but not top-level executable statements.
- The implicit `start` function is build-system-only and cannot be imported or called directly from Beanstalk code.

**File execution semantics:**
```beanstalk
-- main.bst (entry file)
import @utils/helper/run_helper
import @utils/helper/another_func

io("Starting main")

run_helper()
another_func()
```

Only the entry file's top-level runtime code executes automatically.
Other files contribute declarations that must be imported explicitly by symbol.

**Import resolution rules:**
- Relative imports (`@./x` / `@..`) resolve from the importing file's directory
- Non-relative imports whose first segment matches `#root_folders` resolve from the project root
- Other non-relative imports resolve from the configured module entry root
- Grouped imports expand into multiple individual symbol imports.
- Circular imports are detected and cause compilation errors

### Hash (`#`) semantics
At top level, `#` changes behavior by declaration kind:
- Variable declaration: exported constant declaration (compile-time only)
- Function declaration: exported function (visibility only)
- Struct or choice declaration: exported type/symbol (visibility only)
- Template head (`#[...]`): entry-file-only top-level const template declaration that must fully fold at compile time

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

Top-level const templates follow the same compile-time rule and are currently entry-file only.

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