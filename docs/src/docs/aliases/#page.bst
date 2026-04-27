import @lib/html {style}
import @styles/docs {navbar, title, section, codeblock, codesnippet, theme_head}

# page_title = "Type Aliases"
# page_description = "Transparent type aliases in Beanstalk."
# page_head = theme_head

#[navbar]
#[title, style.center: TYPE ALIASES]

#[section, $markdown:
    Type aliases give another name to an existing type.

    They are useful when a type has a domain-specific meaning, when a type annotation is long,
    or when imported types should have a shorter local name.

    [codeblock, $code("bst"):
        UserId as Int
        Names as {String}
        MaybeName as String?
    ]

    The syntax is:

    [codeblock, $code("bst"):
        AliasName as ExistingType
    ]

    Alias names should use UpperCamelCase, the same as structs, choices and other type names.

    Type aliases are compile-time-only. They do not create runtime values and they do not emit code.
]

#[section, $markdown:
    ## Transparent Aliases

    Type aliases are transparent.

    This means an alias does not create a new nominal type. It is another name for the target type.

    [codeblock, $code("bst"):
        UserId as Int

        id UserId = 42
        raw Int = id -- valid, because UserId is Int
    ]

    This is different from a struct.

    [codeblock, $code("bst"):
        UserId as Int

        User = |
            id Int,
        |
    ]

    [codesnippet:UserId] is just another name for [codesnippet:Int].
    [codesnippet:User] is a distinct nominal struct type.

    Use a struct when you need a real domain type with its own fields, constructor and methods.
    Use a type alias when you only want a clearer type name.
]

#[section, $markdown:
    ## Aliases Are Not Values

    A type alias can only be used where the compiler expects a type annotation.

    [codeblock, $code("bst"):
        UserId as Int

        id UserId = 42 -- valid
    ]

    A type alias cannot be used as a value, constructor, receiver or copy target.

    [codeblock, $code("bst"):
        UserId as Int

        io(UserId)      -- Error: UserId is a type alias, not a value
        id = UserId(42) -- Error: UserId is not a constructor
        copy_id = copy UserId -- Error: aliases cannot be copied
    ]

    If the alias targets a constructible type, construct the real target type instead.

    [codeblock, $code("bst"):
        Person = |
            name String,
            age Int,
        |

        MainPerson as Person

        person MainPerson = Person("Ada", 36)
    ]
]

#[section, $markdown:
    ## Aliasing Collections and Options

    Aliases can target nested type expressions.

    [codeblock, $code("bst"):
        UserId as Int
        UserIds as {UserId}
        MaybeUserId as UserId?

        ids UserIds = {1, 2, 3}
        selected MaybeUserId = none
    ]

    Aliases are resolved before normal type checking, so nested aliases behave exactly like their final target type.

    [codeblock, $code("bst"):
        Name as String
        Names as {Name}

        names Names = {"Ada", "Grace", "Evelyn"}
    ]

    Empty collection literals still need an explicit element type at the declaration site.

    [codeblock, $code("bst"):
        Names as {String}

        names Names = {} -- valid: the alias provides the collection element type
        bad_names ~= {}  -- Error: empty collection type is ambiguous
    ]
]

#[section, $markdown:
    ## Aliases in Function Signatures

    Type aliases can be used in parameters, returns and local declarations.

    [codeblock, $code("bst"):
        UserId as Int
        Username as String

        format_user |id UserId, name Username| -> String:
            return [: #[id] [name]]
        ;

        id UserId = 7
        name Username = "Bean"

        label = format_user(id, name)
        io(label)
    ]

    Because aliases are transparent, passing an [codesnippet:Int] where [codesnippet:UserId] is expected is valid.

    [codeblock, $code("bst"):
        UserId as Int

        print_id |id UserId|:
            io(id)
        ;

        raw_id Int = 1
        print_id(raw_id) -- valid
    ]

    If you need the compiler to reject raw [codesnippet:Int] values in a user-id position,
    use a struct instead of an alias.
]

#[section, $markdown:
    ## Exporting and Importing Aliases

    Use [codesnippet:#] to export a type alias from another file.

    [codeblock, $code("bst"):
        -- types.bst
        # UserId as Int
        # Username as String
    ]

    Import the alias by symbol path.

    [codeblock, $code("bst"):
        -- #page.bst
        import @types/UserId
        import @types/Username

        id UserId = 1
        name Username = "Ada"
    ]

    Imported aliases can also be renamed locally with [codesnippet:as].

    [codeblock, $code("bst"):
        import @types/UserId as Id

        id Id = 1
        io(id)
    ]

    Import aliases are file-local. Renaming an imported type alias does not change the exported name in the source file.
]

#[section, $markdown:
    ## Aliasing Structs and Choices

    Aliases can target user-defined structs and choices.

    [codeblock, $code("bst"):
        Person = |
            name String,
            age Int,
        |

        MainPerson as Person

        person MainPerson = Person("Alice", 30)
        io(person.name)
    ]

    The alias does not create a second struct identity.
    [codesnippet:MainPerson] and [codesnippet:Person] are the same type to the compiler.

    Aliases can also target choices.

    [codeblock, $code("bst"):
        Status ::
            Ready,
            Loading,
            Failed String,
        ;

        LoadStatus as Status

        status LoadStatus = Status::Ready

        if status is:
            case Ready => io("ready")
            else => io("not ready")
        ;
    ]
]

#[section, $markdown:
    ## External Types

    Imported external package types can be aliased too.

    [codeblock, $code("bst"):
        import @std/io/IO

        StdIO as IO
    ]

    This is mainly useful when an external type name is too general or when a local page wants a clearer name for documentation.
]

#[section, $markdown:
    ## Cycles Are Invalid

    Type aliases must eventually resolve to a real type.

    Recursive aliases are rejected.

    [codeblock, $code("bst"):
        A as B
        B as A -- Error: circular dependency
    ]

    Self-referential aliases are also invalid.

    [codeblock, $code("bst"):
        UserId as UserId -- Error
    ]

    Use a real struct or another explicit runtime representation when recursive data is needed.
]

#[section, $markdown:
    ## When To Use Type Aliases

    Use a type alias for clarity:

    [codeblock, $code("bst"):
        UserId as Int
        HtmlFragment as String
        CssClass as String
        RouteParts as {String}
    ]

    Use a struct when you need a real data type:

    [codeblock, $code("bst"):
        UserId = |
            value Int,
        |

        user_id UserId = UserId(42)
    ]

    A good rule:

    - Alias: better name for an existing type.
    - Struct: new domain type with its own identity.
    - Choice: one of several named variants.
]