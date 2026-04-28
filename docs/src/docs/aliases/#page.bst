import @lib/html {style}
import @styles/docs {navbar, title, section, codeblock, codesnippet, theme_head}

# page_title = "Aliases"
# page_description = "Type aliases, import aliases, and local renaming with as in Beanstalk."
# page_head = theme_head

#[navbar]
#[title, style.center: ALIASES]

#[section, $markdown:
    Aliases give a local name to something that already exists.

    Beanstalk uses [codesnippet:as] for three narrow, deliberate renaming surfaces:

    - Type alias declarations.
    - Import aliases.
    - Choice payload capture aliases in match arms.

    [codeblock, $code("bst"):
        UserId as Int

        import @models/User as ModelUser

        case Err(message as error_message) => io(error_message)
    ]

    Aliases are useful when a name has a domain-specific meaning, when a type annotation is long,
    or when two imported libraries export symbols with the same name.

    They are not a general shadowing feature. An alias introduces a visible file-local name and must not
    collide with any other visible name in that file.
]

#[section, $markdown:
    ## Type Aliases

    Type aliases give another name to an existing type.

    [codeblock, $code("bst"):
        UserId as Int
        Names as {String}
        MaybeName as String?
    ]

    The syntax is:

    [codeblock, $code("bst"):
        AliasName as ExistingType
    ]

    Alias names should use UpperCamelCase, the same as structs, choices, and other type names.

    Type aliases are compile-time-only. They do not create runtime values and they do not emit code.
]

#[section, $markdown:
    ## Transparent Type Aliases

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

    Use a struct when you need a real domain type with its own fields, constructor, and methods.
    Use a type alias when you only want a clearer type name.
]

#[section, $markdown:
    ## Type Aliases Are Not Values

    A type alias can only be used where the compiler expects a type annotation.

    [codeblock, $code("bst"):
        UserId as Int

        id UserId = 42 -- valid
    ]

    A type alias cannot be used as a value, constructor, receiver, or copy target.

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

    Type aliases can be used in parameters, returns, and local declarations.

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
    ## Import Aliases

    Import aliases rename an imported symbol for the current file.

    [codeblock, $code("bst"):
        import @models/User as ModelUser
        import @admin/User as AdminUser

        model_user ModelUser = ModelUser("Ada")
        admin_user AdminUser = AdminUser("Grace")
    ]

    The imported declaration keeps its original exported name in its source file.
    The alias only changes the name visible in the importing file.

    Import aliases work for all explicitly imported symbols:

    - Functions.
    - Constants.
    - Structs.
    - Choices.
    - Type aliases.
    - External package functions, constants, and types.

    [codeblock, $code("bst"):
        import @std/math/sin as sine
        import @std/math/PI as CIRCLE_PI

        value = sine(CIRCLE_PI)
    ]

    Import aliases are file-local. They are not exported and they do not rename the source declaration for other files.
]

#[section, $markdown:
    ## Resolving Import Name Conflicts

    Import aliases are most useful when two libraries export the same symbol name.

    [codeblock, $code("bst"):
        import @blog/render as render_blog
        import @docs/render as render_docs

        blog_html = render_blog()
        docs_html = render_docs()
    ]

    Without aliases, both imports would try to introduce [codesnippet:render] into the same file.
    That is a name collision.

    [codeblock, $code("bst"):
        import @blog/render
        import @docs/render -- Error: render is already visible in this file
    ]

    The same rule applies across value and type names. Even if the compiler can internally tell that one symbol is a type
    and another is a value, Beanstalk rejects duplicate visible spellings for readability.

    [codeblock, $code("bst"):
        import @models/User
        import @formatters/User -- Error: User is already visible in this file
    ]

    Rename one or both imports so the file remains unambiguous.

    [codeblock, $code("bst"):
        import @models/User as ModelUser
        import @formatters/User as format_user

        user ModelUser = ModelUser("Ada")
        label = format_user(user)
    ]
]

#[section, $markdown:
    ## Grouped Import Aliases

    Grouped imports can rename individual imported entries.

    [codeblock, $code("bst"):
        import @components {
            Button as PrimaryButton,
            Card as ArticleCard,
            render as render_component,
        }
    ]

    Nested grouped paths can also use aliases.

    [codeblock, $code("bst"):
        import @docs {
            pages/home/render as render_home,
            pages/about/render as render_about,
        }

        home = render_home()
        about = render_about()
    ]

    The alias applies only to the final imported symbol binding.
    The canonical target still resolves to the full imported path.

    [codeblock, $code("bst"):
        import @docs {
            pages/home/render as render_home,
        }

        -- The local name is render_home.
        -- The imported target is still @docs/pages/home/render.
    ]

    A grouped import cannot use one trailing alias for the whole group.
    Each entry must be renamed separately.

    [codeblock, $code("bst"):
        import @components {Button, Card} as Ui -- Error

        import @components {
            Button as UiButton,
            Card as UiCard,
        }
    ]
]

#[section, $markdown:
    ## Import Aliases and Type Aliases Together

    Type aliases can target imported aliases.

    [codeblock, $code("bst"):
        import @models/User as ExternalUser

        CurrentUser as ExternalUser

        user CurrentUser = ExternalUser("Ada")
    ]

    This is useful when one file wants a local import name, but also wants a domain-specific type alias for annotations.

    Imported type aliases can also be renamed directly.

    [codeblock, $code("bst"):
        -- types.bst
        # UserId as Int
    ]

    [codeblock, $code("bst"):
        -- #page.bst
        import @types/UserId as AccountId

        id AccountId = 42
    ]
]

#[section, $markdown:
    ## Alias Collision Rules

    Aliases must not collide with any visible name in the same file.

    This includes:

    - Same-file declarations.
    - Other imports.
    - Import aliases.
    - Type aliases.
    - Builtin names.
    - Prelude names.

    [codeblock, $code("bst"):
        render ||:
            return "local"
        ;

        import @other/render as render -- Error: render is already declared in this file
    ]

    Aliases do not shadow existing names. Pick a more specific name instead.

    [codeblock, $code("bst"):
        import @other/render as render_other
    ]

    Re-importing the same symbol with a different alias should be avoided unless the file genuinely needs both names.
    Prefer one local spelling per imported symbol.
]

#[section, $markdown:
    ## Alias Naming Warnings

    Import aliases should keep the same broad naming convention as the imported symbol.

    If the imported symbol starts with a capital letter, the alias should also start with a capital letter.
    If the imported symbol starts with a lowercase letter, the alias should also start with a lowercase letter.

    [codeblock, $code("bst"):
        import @models/User as ModelUser      -- good
        import @models/User as model_user     -- warning

        import @formatters/render as render_page -- good
        import @formatters/render as RenderPage  -- warning
    ]

    This warning is intentionally simple. It does not try to fully distinguish [codesnippet:camelCase],
    [codesnippet:snake_case], or other naming styles.

    The goal is only to catch obvious type/value naming drift.
]

#[section, $markdown:
    ## Choice Payload Capture Aliases

    Choice payload capture aliases rename a matched payload field inside one match arm.

    [codeblock, $code("bst"):
        Response ::
            Err | message String |,
            Success | value String |,
        ;

        response = Response::Err("Missing title")

        if response is:
            case Err(message as error_message) => io(error_message)
            case Success(value as body) => io(body)
        ;
    ]

    The name before [codesnippet:as] is the declared payload field name.
    The name after [codesnippet:as] is the local capture name visible inside that arm.

    [codeblock, $code("bst"):
        case Err(message as error_message) => io(error_message)
    ]

    This keeps the pattern tied to the declared payload shape while still letting the arm choose a clearer local name.

    Without [codesnippet:as], the capture name is the field name.

    [codeblock, $code("bst"):
        case Err(message) => io(message)
    ]

    Capture aliases are useful when a field name would collide with an existing variable or when the local meaning is clearer
    than the generic payload field name.

    [codeblock, $code("bst"):
        message = "outer message"

        if response is:
            case Err(message as error_message) => io(error_message)
            else => io(message)
        ;
    ]

    Choice payload capture aliases are arm-local. They do not rename the variant field itself and they do not leak into other arms.
]

#[section, $markdown:
    ## What as Does Not Do

    [codesnippet:as] is not a cast operator and it is not a general renaming expression.

    These forms are invalid:

    [codeblock, $code("bst"):
        value = input as Int        -- Error: use explicit casts such as Int(input)
        renamed as existing_value   -- Error: this is not a value alias
        function_call() as result   -- Error: expression aliases are not supported
    ]

    Use [codesnippet:as] only for supported alias declarations, import aliases, and Choice payload capture aliases.

    This keeps renaming predictable and avoids adding multiple ways to express the same idea.
]

#[section, $markdown:
    ## When To Use Aliases

    Use a type alias for clearer annotations:

    [codeblock, $code("bst"):
        UserId as Int
        HtmlFragment as String
        CssClass as String
        RouteParts as {String}
    ]

    Use an import alias when an imported name is too generic or conflicts with another visible name:

    [codeblock, $code("bst"):
        import @blog/render as render_blog
        import @docs/render as render_docs
    ]

    Use a Choice payload capture alias when the field name is correct on the type but awkward in one arm:

    [codeblock, $code("bst"):
        case Failed(message as failure_message) => io(failure_message)
    ]

    A good rule:

    - Type alias: better name for an existing type.
    - Import alias: better local name for an imported symbol.
    - Choice payload capture alias: better arm-local name for an extracted field.
    - Struct: new domain type with its own identity.
    - Choice: one of several named variants.
]
