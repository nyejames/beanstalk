import @html {center}
import @styles/docs {navbar, title, section, palette, styled_table, header, data, codeblock,theme_head}

page_title #= "Language Basics"
page_description #= "Core language syntax and semantics in Beanstalk."
page_head #= theme_head

#[navbar]
#[title, center: LANGUAGE BASICS]

#[section, $markdown:
    Every project has a `#config.bst` file that defines the entry points and dependencies of the project.

    If instead, you build or run a specific Beanstalk file,
    it will create a single module from that entry point and pull in all the file's dependencies.

    [codeblock, $code("bst"):
        page_title #= "Welcome"

        say_hello || -> String:
            return "Hi, welcome to Beanstalk"
        ;
        
        [say_hello()]
    ]

    # Quick Syntax Summary
    For developers coming from most other languages, 
    here are some key idiosyncrasies from other C-like languages to note:

    - Colon opens a scope, semicolon closes it. Semicolon does not end statements.

    - Square brackets are NOT used for arrays, curly braces are used instead
    Square brackets are only used for string templates. Items in collections are accessed via methods.

    - Equality and other logical operators use keywords like "is" and "not"; symbolic equality
    and logical-not forms are not part of Beanstalk syntax.

    - tilde symbol ` ~` to indicate mutability (mutability must be explicit).
    This comes before the type if there is an explicit type declaration.

    - hash symbol ` #` for constants (mutually exclusive with mutability).

    - Immutable Reference semantics are the default for all stack and heap allocated types. 

    - All copies have to be explicit unless they are used in part of a new expression. 
    Even for primitive types such as integers.

    - Explicit conversions use `cast` at a typed boundary. Constructor-style conversions such as
    `Int(value)` and `String(value)` are removed.

    ## Imports and Receiver Methods
    Namespace imports expose receiver methods only through receiver-call syntax, never as namespace fields.
    Source-authored receiver methods are visible through their receiver type; they are not imported,
    aliased, or re-exported independently.
    Receiver-method names can be shared across different receiver types because dispatch includes
    the receiver type.

    ## External JavaScript Signatures
    Annotated `.js` imports use `@bst.opaque` and `@bst.sig` comments.
    Supported JavaScript export forms are `export function name(...) { ... }` and block-bodied `export const name = (...) => { ... }`.
    Expression-bodied arrow exports are rejected.
    Source-authored `@bst.sig` comments should expose free functions and opaque types; `this`
    receiver signatures are being kept out of the source receiver-method surface.
    JS libraries may import registered runtime modules such as `@beanstalk/runtime` with named static imports only.
    The named runtime import list may span multiple lines.
    Runtime modules and import maps are emitted from the actual accepted JS imports, so fallible JS functions may either use `bstOk` / `bstErr` helpers or return the wrapper shape manually.

    ## Formatting
    - 4 spaces are recommended for indentation.
    - Types use UpperCamelCase.
    - Everything else uses regular_snake_case.

    # Comments
    Comments use a double minus sign. `--`
    
    Documentation comments will eventually be created via special templates.

    [codeblock, $code("bst"):
        -- normal comment
    ]

]

[section, $markdown:
    # Variables

    Equals means assign. Tilde ` ~` means this is a mutable and can change.

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

        mutable_collection ~{Int} = {}
        read_only_collection {Int} = {}
        fixed_collection ~{4 Int} = {}

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
        a_copy_of_a_number ~= copy another_mutable_number

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

    All copies are explicit and must use the `copy` keyword in front of the value being copied.

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
        ~a_reference.push(5) catch:
            assert(false, "This should never fail for this growable collection in this example")
        ;
        io(a_collection) -- {1, 2, 3, 4, 5, 5}

        ~a_collection.remove(a_collection.length() - 1) catch:
            then 0
        ;
        io(a_collection) -- {1, 2, 3, 4, 5}
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

#[section, $markdown:
    # Explicit Casts

    `cast` converts one value to an explicit builtin target type supplied by the immediate
    receiving site.

    [codeblock, $code("bst"):
        parse_count |text String| -> Int, Error!:
            return cast! text
        ;

        whole Int = parse_count("42") catch:
            then 0
        ;

        ratio Float = cast whole
        label String = cast ratio
        maybe_label String? = cast whole
    ]

    Use plain `cast` for infallible evidence, `cast!` to propagate a fallible cast through an
    `Error!` return slot, and `cast ... catch:` to recover locally.

    [codeblock, $code("bst"):
        count Int = cast "not a number" catch |err|:
            then 0
        ;
    ]

    Valid targets are the compiler-supported builtin core types: `Bool`, `Int`, `String`,
    `Char`, `Float`, and `Error`. Optional receiving contexts cast to the inner builtin type and
    then wrap normally. For `T? = cast source catch:`, the recovery handler also produces the
    inner `T`; `then none` is invalid because recovery handles the cast result before optional
    wrapping.

    In the Alpha JS runtime target, `String -> Int` and `Float -> Int` casts materialize signed
    32-bit integer values from `-2147483648` through `2147483647`. Values outside that range fail
    the cast so compile-time folding and runtime behavior agree.

    These forms are invalid because they do not provide an immediate builtin target:

    [codeblock, $code("bst"):
        value = cast source
        if cast value:
            io("bad")
        ;
        [: [cast value]]
    ]

    `Error(...)` remains a constructor. Scalar constructor-style conversions such as `Int(...)`,
    `Float(...)`, `Bool(...)`, `String(...)`, and `Char(...)` are rejected; use `cast` instead.
]

#[section, $markdown:
    # Data Types
    Beanstalk data type keywords use UpperCamelCase.
    Mutibility can change the underlaying data type but is not actually part of the type.

    [styled_table:
        [header: [: Type] [: Description] ] 

        [data: [: Float ] [: 64-bit floating point number] ]

        [data: [: Int ] [:  32-bit signed integer ] ]

        [data: [: String ] [: UTF-8 string slice or string template] ]
    ]

    ## Numerical Types
    These may be extended to smaller sized and unsigned variants in the future.
    By default, `"Float"` is a finite 64-bit floating point type and `"Int"` is a signed 32-bit integer type.

    The language currently supports a small set of core types but may be extended in the future.

    # Strings and String slices
    String is the keyword for string types in Beanstalk. 
    Double quotes are automatically string slices. 
    
    `"Double quotes for a UTF-8 string slice"`

    Backticks create raw string slices in expression position. Inside template bodies, backticks are ordinary text available to formatters such as `$markdown`.

    Scenes are used instead of format strings.

    # Operators

    ## Logical and Comparison Operators
    The 'is' keyword is used to check equality. 
    The "and / or" keywords are used for logical and / or and 'not' is used to invert a truthy value to falsy or vice versa.

    ## Mathematical Operators
    [styled_table:
        [header: [: Operator] [: Description]          [: Precedence] ]
        [data: [: ^]        [: Exponent]            [: 4] ]
        [data: [: *]        [: Multiplication]       [: 3] ]
        [data: [: /]        [: Division]             [: 3] ]
        [data: [: //]       [: Integer Division]     [: 3] ]
        [data: [: %]        [: Modulo (Euclidean)]   [: 3] ]
        [data: [: +]        [: Sum]                  [: 2] ]
        [data: [: -]        [: Subtraction]          [: 2] ]
    ]
]
