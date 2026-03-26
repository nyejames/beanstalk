import @lib/html {format, center}
import @styles/docs {navbar, title, section, palette, table, row, header_row, codeblock, theme_head}

# page_title = "Language Basics"
# page_description = "Core language syntax and semantics in Beanstalk."
# page_head = theme_head

[navbar]
[title, center: LANGUAGE BASICS]

[section, $markdown:
    Every project has a #config.bst file.

    If instead, you build or run a specific Beanstalk file,
    it will create a single module from that entry point and pull in all the file's dependencies.

    From the config file you can define a Beanstalk file as the entry point of a program.

    Web projects have their own directory based structure and config file also.

    [codeblock, $code("bst"):
        # page_title = "Welcome"

        say_hello || -> String:
            return "Hi, welcome to Beanstalk"
        ;
        
        [say_hello()]
    ]

    # Quick Syntax Summary
    For developers coming from most other languages, 
    here are some key idiosyncrasies from other C-like languages to note:

    - Colon opens a scope, semicolon closes it. Semicolon does not end statements

    - Square brackets are NOT used for arrays, curly braces are used instead
    Square brackets are only used for string templates. Items in collections are accessed via methods

    - Equality and other logical operators use keywords like "is" and "not" 
    (you can't use == or ! for example)

    - ~ tilde symbol to indicate mutability (mutability must be explicit). 
    This comes before the type if there is an explicit type declaration.

    - Double dashes for single line comments (--)

    - Immutable Reference semantics are the default for all stack and heap allocated types. 

    - All copies have to be explicit unless they are used in part of a new expression. 
    Even for primitive types such as integers.

    ## Formatting
    - 4 spaces are recommended for indentation.
    - Types use UpperCamelCase
    - Everything else uses regular_snake_case

    # Comments
    Comments use a double minus sign. 
    Documentation comments will eventually be created via special templates.

    [codeblock, $code("bst"):
        -- normal comment
    ]

]

[section, $markdown:
    # Variables
    Equals means assign. Tilde ~ means this is a mutable and can change.

    All variables must be assigned a value when they are declared.

    [codeblock, $code("bst"):
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
    ]

    By default, Beanstalk code will create immutable references to values.
    **This is true for all types.**

    The compiler might decide to make an immutable copy of a primative value,
    but the assignment will **always** behave the same way no matter what the type is.


    [codeblock, $code("bst"):
        -- 64 bit immutable float
        number = 420.69

        -- Becomes an immutable reference to the value
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
        another_mutable_number ~= "A string instead. Whoops."

        -- Explicit type declarations
        a_float ~Float = 84
        string_slice = "Hello "
        a_mutable_string ~String = [:World!]

        -- You can't use the '+' operator on strings,
        -- They can be concatenated using templates
        concatenated_strings = [string_slice, a_mutable_string]
    ]

    All copies are explicit and must use the 'copy' keyword in place of a type.

    [codeblock, $code("bst"):
        -- Create a new collection of integers
        a_collection ~= {1, 2, 3, 4, 5}
        
        -- Deep mutable copy of a collection
        a_copy ~= copy a_collection

        -- Immutable reference to a_collection
        a_reference = a_collection

        -- Ownership passed or mutable reference depending on context
        a_reference ~= a_collection

        -- a_reference is still a reference to the original collection
        a_reference.push(5)
        io(a_collection) -- {1, 2, 3, 4, 5, 5}

        a_collection.pull(a_collection.length() - 1)
        io(a_collection) -- {23.0}
    ]

    Expressions can span over multiple lines.

    But new statements must start after a newline.

    [codeblock, $code("bst"):
        -- Valid
        some_int =
            4 + 5 + 6 + 7 + 
            8 + 9 + 10

        -- Also valid
        some_int =
            4 + (5 + 6) * 7
            + 8 / 9 + 10
    ]
]

[section, $markdown:
    # Data Types
    Beanstalk data type keywords use UpperCamelCase.
    Mutibility can change the underlaying data type but is not actually part of the type.

    [table:
        [header_row: [: Type] [: Description] ] 

        [row: [: Float ] [: 64-bit floating point number] ]

        [row: [: Int ] [:  64-bit signed integer ] ]

        [row: [: String ] [: UTF-8 string slice or string template] ]
    ]

    ## Numerical Types
    These will be extended to smaller sized and unsigned variants in the future.
    By default, "Float" and "Int" will become 64 bit signed types.

    The language currently supports a small set of core types but may be extended in the future.

    # Strings and String slices
    String is the keyword for string types in Beanstalk. 
    Double quotes are automatically string slices. 

    `"Double quotes for a UTF-8 string slice"`

    Backticks are used for RAW strings. To escape a backtick it must be preceded with a backslash \.

    Scenes are used instead of format strings.

    # Operators

    ## Logical and Comparison Operators
    The 'is' keyword is used to check equality. 
    The "and / or" keywords are used for logical and / or and 'not' is used to invert a truthy value to falsy or vice versa.

    ## Mathematical Operators
    [table:
        [header_row: [: Operator] [: Description]          [: Precedence] ]
        [row: [: ^]        [: Exponent]            [: 4] ]
        [row: [: //]       [: Root]                [: 4] ]
        [row: [: *]        [: Multiplication]       [: 3] ]
        [row: [: /]        [: Division]             [: 3] ]
        [row: [: %]          [: Modulo (truncated)]   [: 3] ]
        [row: [: %%]         [: Remainder (floored)]  [: 3] ]
        [row: [: +]          [: Sum]                  [: 2] ]
        [row: [: -]        [: Subtraction]          [: 2] ]
    ]
]
