[Navbar]

[Header center: [title(1): BASIC BS SYNTAX]]
[Page:

Each folder containing a config.bst file becomes a module. 
Each .bst file inside this folder is combined together and their top level declarations are public to eachother.
Each module is just a file that exports functions and fields.

The config.bst file defines the entry point file.
The main function of the module is just the top level of the entry point file.

Projects are a collection of one or more modules.

Every Beanstalk file can also export functions, which exposes those functions to the host environment.

# Quick Synax Overview
- Colon opens a scope, semicolon closes it.
- No use of square brackets for arrays, curly braces are used instead. Square brackets are only used for templates.
- Equality and other logical operators use keywords like "is" and "not" (you can't use == or ! for example)
- ~ tilde symbol to indicate mutability (mutability must be explicit). This comes before the type.
- Double dashes for single line comments (--)

4 spaces are recommended for indentation. 

- Types use Upper_Snake_Case.
- Everything else uses regular_snake_case

# Comments
Comments use a double minus sign '--'.

Documentation comments will eventually be created via special templates.

[#Code: 

-- normal comment

[#Docs:
    Multiline comment

    Woo
]

]

# Variables
Equals means assign. Tilde ~ means this is a mutable and can change.

All variables must be assigned a value when they are declared.

## Assignment

### Variables
[#Code:
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
]

## Static Assignment
[#Code:
    -- 64 bit immutable float
    number = 420.69

    -- Becomes an immutable reference to the value
    -- For primitives this is actually just an immutable copy,
    -- But the behaviour is consistent for all datatypes, including heap allocated ones
    another_number = number

    -- Becomes a mutable reference to the number
    -- or moves the value if it isn't used again later in the scope
    another_mutable_number ~= number

    -- Type error (number is not mutable)
    number = 1

    -- Type error (another_mutable_number is a float type)
    another_mutable_number ~= "Not a number"
]

All copies of collections are explicit and must use the 'copy' keyword in place of a type.

[#Code:
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

But statements must start after a newline.

[#Code:
    -- Valid
    some_int =
        4 + 5 + 6 + 7 + 
        8 + 9 + 10

    -- Also valid
    some_int =
        4 + 5 + 6 + 7
        + 8 + 9 + 10
]

# Data Types
All data type keywords contain methods from the standard library for common manipulation of types.

## Numerical Types
[table(3):
    [: Type] [: Description]

    [: float ] [: 64 bit floating point number]

    [: int ] [:  32 bit signed integer (may become 64 bit in the future or in certain contexts) ]
]

## String based Types
These are all different ways to create strings.

[table(3): 
    [: Type] [: Description]

    [: string slice ] [: UTF-16 (For JS compatibility)]

    [: template ] [: The string templating syntax of Beanstalk for creating strings. See [link "./scenes": Scenes for more info!]]
]

# Strings and String slices
String is the keyword for string types in Beanstalk. 
Double quotes are automatically string slices. 

[#Code: "Double quotes for a UTF-16 string slice"]

Backticks are used for RAW strings. To escape a backtick it must be preceded with a backslash \.

Scenes are used instead of format strings. See [link "./scenes": Scenes] for more information.

# Logical Operators
The 'is' keyword is used to check equality, not '==''. 
The "and / or" keywords are used for logical and / or and 'not' is used to invert a truthy value to falsy or vice versa.

[table(3):
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

]

[Footer]
