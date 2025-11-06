[#import(@libs/html/basic)]
[#import(@styles/docs_styles)]
[#import(@./components)]

[docs_styles.Navbar]

[docs_styles.Header, basic.Center: [basic.Title: BASIC BS SYNTAX]]

Every project needing to be split into multiple Wasm modules must have a config.bst file.

If instead, you build or run a specific Beanstalk file,
it will create a single Wasm module from that entry point.

From the config file you can define another Beanstalk file as the entry point of a program.

Web projects can be configured to have a directory based structure for each module.

# Quick Synax Overview
- Colon opens a scope, semicolon closes it. Semicolon does not end statements!
- No use of square brackets for arrays, curly braces are used instead. 
Square brackets are only used for templates.
- Equality and other logical operators use keywords like "is" and "not" 
(you can't use == or ! for example)
- ~ tilde symbol to indicate mutability (mutability must be explicit). 
This comes before the type.
- Double dashes for single line comments (--)
- Reference semantics. All copies have to be explict unless they are used in part of a new expression. 
Even for primitive types such as integers.

4 spaces are recommended for indentation. 

- Types use Upper_Snake_Case.
- Everything else uses regular_snake_case

# Comments
Comments use a double minus sign '--'.

Documentation comments will eventually be created via special templates.

[basic.Code: 

    -- normal comment

    [#docs():
        Multiline comment

        Woo
    ]
]

# Variables
Equals means assign. Tilde ~ means this is a mutable and can change.

All variables must be assigned a value when they are declared.

## Assignment

### Variables
[basic.Code:
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

    Struct:
        value Float,
        another_value String,
    ;
]

[basic.Code:
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
]

All copies are explicit and must use the 'copy' keyword in place of a type.

[basic.Code:
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
    print(a_collection) -- {1, 2, 3, 4, 5, 5}

    a_collection.pull(a_collection.length() - 1)
    print(a_collection) -- {23.0}
]

Expressions can span over multiple lines.

But new statements must start after a newline.

[basic.Code:
    -- Valid
    some_int =
        4 + 5 + 6 + 7 + 
        8 + 9 + 10

    -- Also valid
    some_int =
        4 + (5 + 6) * 7
        + 8 / 9 + 10
]

# Data Types
All data type keywords contain methods from the standard library for common manipulation of types.

## Numerical Types
[basic.table(3):
    [: Type] [: Description]

    [: float ] [: 64 bit floating point number]

    [: int ] [:  64 bit signed integer ]
]

## String based Types
These are all different ways to create strings.

[basic.table(3): 
    [: Type] [: Description]

    [: string slice ] [: UTF-16 (For JS compatibility)]

    [: template ] [: The string templating syntax of Beanstalk for creating strings. See [@./templates: Templates] for more info!]
]

# Strings and String slices
String is the keyword for string types in Beanstalk. 
Double quotes are automatically string slices. 

[basic.Code: "Double quotes for a UTF-16 string slice"]

Backticks are used for RAW strings. To escape a backtick it must be preceded with a backslash \.

Scenes are used instead of format strings. See [@./templates: Scenes] for more information.

# Logical Operators
The 'is' keyword is used to check equality, not '==''. 
The "and / or" keywords are used for logical and / or and 'not' is used to invert a truthy value to falsy or vice versa.

[basic.table(3):
    [: Operator] [: Description]          [: Precedence]
    [: `^`]        [: Exponent]            [: 4]
    [: `//`]       [: Root]                [: 4]
    [: `*`]        [: Multiplication]       [: 3]
    [: `/`]        [: Division]             [: 3]
    [: %]          [: Modulo (truncated)]   [: 3]
    [: %%]         [: Remainder (floored)]  [: 3]
    [: +]          [: Sum]                  [: 2]
    [: `-`]        [: Subtraction]          [: 2]
]

[docs_styles.Footer]
