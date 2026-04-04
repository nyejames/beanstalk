import @lib/html {format, style}
import @styles/docs {navbar, title, section, palette, table, codeblock, theme_head}

# page_title = "Pattern Matching"
# page_description = "Pattern matching in Beanstalk."
# page_head = theme_head

#[navbar]
#[title, style.center: PATTERN MATCHING]

#[section, $markdown:

    # Pattern matching
    Pattern matching is a powerful tool for control flow in Beanstalk. 
    It provides the next level of expressiveness beyond simple if statements. 
    When you need to check more than one condition, this is what you use.
    
    It allows you to match on the value of an expression and execute different code based on the pattern that matches.
    Pattern matching is also exhaustive, meaning that the compiler will check that all possible cases are handled, which can help prevent bugs.

    [codeblock, $code("bst"):
        -- Pattern matching
        thing = 69

        if thing is:
            case 0 => io("zero")
            case < 0 => io("negative")
            case > 1 => io("big number")
            case not 5 => io("not 5")
            else => io("only one")
        ;
        
        -- Matching choices
        if Catagory is:
        case A =>
            io("Category A")
        case B =>
            io("Category B")
        case C |message| =>
            io([:Category C with message: [message]])
        ;
    ]
]