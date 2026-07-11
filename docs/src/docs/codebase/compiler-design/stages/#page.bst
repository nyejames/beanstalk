import @html {table, th, td}
import @styles/docs {navbar, section, codebase_theme_head}

page_title #= "Pipeline stages"
page_description #= "A map of the eight compiler stages, their ownership and their detailed references."
page_head #= codebase_theme_head

#[navbar]

#[section, $md:
    @../ (Compiler design) / Pipeline stages

    # Pipeline stages

    The frontend prepares and orders declarations, resolves semantics, validates memory safety and hands compiled modules to the backend. Each stage owns one clean transformation.
]

#[section, $md:
    Each page below explains the shape a stage receives, the shape it produces and the decisions it owns. Follow the pipeline forward when a change crosses a boundary.

    [table:
        [th:
            [$md: **Stage**]
            [$md: **Ownership**]
            [$md: **Page**]
        ]

        [td:
            [$md: Stage 0: Project structure]
            [$md: Finds config, module roots, reachable files and builder-supported source assets.]
            [$md: @./project-structure (Project structure)]
        ]

        [td:
            [$md: Stage 1: Tokenization]
            [$md: Converts source text into located tokens and owns lexical recognition.]
            [$md: @./tokenization (Tokenization)]
        ]

        [td:
            [$md: Stage 2: Header parsing]
            [$md: Discovers top-level declarations, dependency edges and import surfaces.]
            [$md: @./header-parsing (Header parsing)]
        ]

        [td:
            [$md: Stage 3: Dependency sorting]
            [$md: Orders declarations topologically and reports dependency cycles.]
            [$md: @./dependency-sorting (Dependency sorting)]
        ]

        [td:
            [$md: Stage 4: AST construction]
            [$md: Resolves declarations, checks bodies, folds constants and prepares HIR input.]
            [$md: @./ast-construction (AST construction)]
        ]

        [td:
            [$md: Stage 5: HIR generation]
            [$md: Lowers typed AST into backend-facing semantic IR with explicit control flow.]
            [$md: @./hir-generation (HIR generation)]
        ]

        [td:
            [$md: Stage 6: Borrow validation]
            [$md: Validates exclusivity and access, producing ownership side-table facts.]
            [$md: @./borrow-validation (Borrow validation)]
        ]

        [td:
            [$md: Stage 7: Backend lowering]
            [$md: Assembles and emits backend artifacts through project builders and lowerers.]
            [$md: @./backend-lowering (Backend lowering)]
        ]
    ]
]
