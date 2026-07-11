import @html {center}
import @styles/docs {navbar, title, section, codeblock, theme_head}

page_title #= "Structs"
page_description #= "Structs in Beanstalk."
page_head #= theme_head

#[navbar]
#[title, center: STRUCTS]

#[section, $md:

    # The Struct
    A struct is a collection of named fields, similar to a struct in other languages.

    Runtime structs are nominal types. Two structs with the same field shape are still different runtime types.

    They can implement statically resolved receiver methods by defining a top-level function whose first argument is literally named `this`.
    `this Type` is immutable, `this ~Type` is mutable, and receiver methods can only be called with `value.method(...)`.
    Source-authored receiver methods must be declared in the same file as their user-defined struct or choice receiver type.

    Compiler-owned builtin operations are not user extension methods. Use free functions for values
    owned by another file, package, builtin type, or external opaque type.

    Structs can be assigned with default arguments, these can be any constant expression.

    [codeblock, $code("bst"):
        Person = |
            name String,
            age Int,
        |

        -- Create a new instance of the type
        person ~= Person("Alice", 30)

        -- Access fields using dot notation
        io.line(person.name) -- "Alice"
        io.line(person.age)  -- 30

        -- Defining a struct, then defining a method for it
        Vector2 = |
            x Float = 0,
            y Float = 0,
        |

        reset |this ~Vector2|:
            this.x = 0
            this.y = 0
        ;

        vec = Vector2(12, 87)
        ~vec.reset()
    ]

]

#[section, $md:
    ## Methods
    Methods in Beanstalk are created using the special `this` receiver parameter.

    You define a receiver by making the first parameter literally named `this`.

    [codeblock, $code("bst"):
        -- Define a struct
        Rectangle = |
            width Float,
            height Float,
        |

        -- Define an immutable method for the Rectangle struct
        area |this Rectangle|:
            return this.width * this.height
        ;

        rect = Rectangle(10, 5)
        io.line(rect.area()) -- 50
    ]

    `this` is reserved for method receivers. You are limited to one `this` parameter per method, and `method(value, ...)` is not valid syntax for calling receiver methods.
    Receiver methods are visible through the receiver type; they are not imported, aliased, or re-exported independently.

    Const-coerced struct values are data-only records. They can be read, but they do not expose runtime methods.
]
