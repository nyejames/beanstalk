import @styles/docs {navbar, section, codebase_theme_head, codeblock}

page_title #= "Memory management"
page_description #= "GC semantics, shared access, borrow validation, ownership and backend memory lowering."
page_head #= codebase_theme_head

#[navbar]

#[section, $md:
    @../ (Codebase) / Memory management

    # Memory management

    Beanstalk keeps the source model small. Reads are shared, copies are explicit and mutation has to announce itself.

    Borrow validation enforces exclusive mutation and move safety. 
    
    A GC semantic baseline keeps execution practical while ownership analysis leaves room for future allocation and deterministic-drop optimisations.
]

#[section, $md:
    ## Shared access vs copy

    Existing values use shared, read-only access by default. Binding a new name to an existing value doesn't copy it. Use `copy` when you need an independent value.

    [codeblock, $code("bst"):
        items ~= {"a", "b"}
        shared_items = items

        change(~items)       -- invalid: shared_items is used again
        inspect(shared_items)
    ]

    [codeblock, $code("bst"):
        items ~= {"a", "b"}
        copied_items = copy items

        change(~items)
        inspect(copied_items)
    ]
]

#[section, $md:
    ## Exclusive access

    `~place` requests temporary mutation-capable access to an existing mutable place. It isn't a move marker or a type identity. Fresh rvalues satisfy mutable parameters without source `~`.

    [codeblock, $code("bst"):
        change({"fresh"})
    ]
]

#[section, $md:
    ## How the compiler layers it

    [codeblock, $code("text"):
        source access rules
            -> AST access-mode and copy validation
            -> HIR places, hidden locals and explicit control flow
            -> borrow validation and ownership facts
            -> GC or ownership-aware backend lowering
    ]
]

#[section, $md:
    ## Detailed pages

    - @./access-and-aliasing (Access and aliasing): shared reads, explicit copies and `~` access
    - @./borrow-validation (Borrow validation): roots, joins, move safety and facts
    - @./ownership-and-drops (Ownership and drops): inferred transfer, unified ABI and conditional destruction
    - @./runtime-and-backend-lowering (Runtime and backend lowering): GC baseline and ownership-aware backend paths
]

#[section, $md:
    ## Current support

    See @../../progress (Progress matrix) for what is implemented today.
]
