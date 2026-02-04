@(libs/html/Basic)
@(styles/Docs)
@(./components)
[Basic.page:

[Docs.navbar]
[Docs.header, Basic.center:
    [Basic.title: LANGUAGE BASICS]
]

Every project needing to be split into multiple Wasm modules must have a config.bst file.

If instead, you build or run a specific Beanstalk file,
it will create a single Wasm module from that entry point and pull in all its dependencies.

From the config file you can define a Beanstalk file as the entry point of a program.

Web projects have their own directory based structure and config file also.

# Quick Syntax Summary
For developers coming from most other languages, 
here are some key idiosyncrasies from other C-like languages to note:

- Colon opens a scope, semicolon closes it. Semicolon does not end statements.
- Square brackets are NOT used for arrays, curly braces are used instead. 
Square brackets are only used for string templates. Items in collections are accessed via methods.
- Equality and other logical operators use keywords like "is" and "not" 
(you can't use == or ! for example)
- ~ tilde symbol to indicate mutability (mutability must be explicit). 
This comes before the type if there is an explicit type declaration.
- Double dashes for single line comments (--)
- Immutable Reference semantics are the default for all stack and heap allocated types. 
- All copies have to be explicit unless they are used in part of a new expression. 
Even for primitive types such as integers.

4 spaces are recommended for indentation. 

- Types use UpperCamelCase
- Constants use UPPER_SNAKE_CASE
- Everything else uses regular_snake_case

# Comments
Comments use a double minus sign. 
Documentation comments will eventually be created via special templates.

```beanstalk
    -- normal comment

    --[:
        Multiline comment

        Woo
    ]
```

# Variables
Equals means assign. Tilde ~ means this is a mutable and can change.

All variables must be assigned a value when they are declared.

## Assignment

### Variables
```beanstalk
    int ~= 0
    float ~= 0.0

    string_slice ~= "wow!"
    char ~= '😊'
    raw_string_slice ~= `hi`

    string_template ~= [:
        This is the Beanstalk programming language
    ]

    bool ~= true

    mutable_collection ~= {}
    immutable_collection = {}

    NewStruct = |
        value Float,
        another_value String,
    |
    
    instance ~= NewStruct(
        value = 1.2, 
        another_value = "hey"
    )
```

```beanstalk
    -- 64 bit immutable float
    number = 420.69

    -- Becomes an immutable reference to the value
    -- For primitives this is actually just an immutable copy,
    -- But the behaviour is consistent for all datatypes, including heap allocated ones
    another_number = number

    -- Becomes a mutable reference to the number
    -- or moves the value if it isn't used again later in the scope
    another_mutable_number ~= number

    -- Even with primative stack types,
    -- Copying is still explicit
    a_copy_of_a_number ~Copy = another_mutable_number

    -- Type error (number is not mutable)
    number = 1

    -- Type error (another_mutable_number is a float type)
    another_mutable_number ~= "Not a number"

    -- Explicit type declarations
    a_float ~Float = 84
    string_slice = "Hello "
    a_mutable_string ~String = [:World!]

    -- You can't use the '+' operator on strings,
    -- They are always concatenated using templates
    concatenated_strings = [string_slice, a_mutable_string]
```

All copies are explicit and must use the 'copy' keyword in place of a type.

```beanstalk
    -- Create a new collection of integers
    a_collection ~= {1, 2, 3, 4, 5}
    
    -- Deep mutable copy of a collection
    a_copy ~copy = a_collection

    -- Immutable reference to a_collection
    a_reference = a_collection

    -- Ownership passed or mutable reference depending on context
    a_reference ~= a_collection

    -- a_reference is still a reference to the original collection
    a_reference.push(5)
    io(a_collection) -- {1, 2, 3, 4, 5, 5}

    a_collection.pull(a_collection.length() - 1)
    io(a_collection) -- {23.0}
```

Expressions can span over multiple lines.

But new statements must start after a newline.

```beanstalk
    -- Valid
    some_int =
        4 + 5 + 6 + 7 + 
        8 + 9 + 10

    -- Also valid
    some_int =
        4 + (5 + 6) * 7
        + 8 / 9 + 10
```

# Data Types
All data type keywords contain methods from the standard library for common manipulation of types.

## Numerical Types
[Basic.table(3):
    [: Type] [: Description]

    [: float ] [: 64 bit floating point number]

    [: int ] [:  64 bit signed integer ]
]

## String based Types
These are all different ways to create strings.

```beanstalk
    [: Type] [: Description]

    [: string slice ] [: UTF-8]

    [: template ] [: The string templating syntax of Beanstalk for creating strings. See [@./templates: Templates] for more info!]
```

# Strings and String slices
String is the keyword for string types in Beanstalk. 
Double quotes are automatically string slices. 

`"Double quotes for a UTF-8 string slice"`

Backticks are used for RAW strings. To escape a backtick it must be preceded with a backslash \.

Scenes are used instead of format strings. See [@./templates: Scenes] for more information.

# Logical Operators
The 'is' keyword is used to check equality, not '==''. 
The "and / or" keywords are used for logical and / or and 'not' is used to invert a truthy value to falsy or vice versa.

```beanstalk
    [: Operator] [: Description]          [: Precedence]
    [: `^`]        [: Exponent]            [: 4]
    [: `//`]       [: Root]                [: 4]
    [: `*`]        [: Multiplication]       [: 3]
    [: `/`]        [: Division]             [: 3]
    [: %]          [: Modulo (truncated)]   [: 3]
    [: %%]         [: Remainder (floored)]  [: 3]
    [: +]          [: Sum]                  [: 2]
    [: `-`]        [: Subtraction]          [: 2]
```

[Docs.footer]
