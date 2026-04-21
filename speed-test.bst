-- speed-test.bst
-- Heavy frontend benchmark:
-- - lots of compile-time constant folding
-- - many nested const templates
-- - slot / insert / children wrapper composition
-- - markdown + css formatting directives
-- - structs + receiver methods
-- - collections, loops, nested loops, conditions
-- - runtime template construction in function bodies

# page_title = "Beanstalk Frontend Speed Test"

# accent = "hsl(273, 92%, 63%)"
# accent_2 = "hsl(196, 91%, 58%)"
# accent_3 = "hsl(22, 96%, 60%)"
# border = "hsla(0, 0%, 100%, 0.10)"
# bg_0 = "hsl(240, 18%, 8%)"
# bg_1 = "hsl(240, 18%, 11%)"
# bg_2 = "hsl(240, 18%, 15%)"
# text_main = "hsl(0, 0%, 96%)"
# text_muted = "hsl(0, 0%, 72%)"

-- Heavy arithmetic constant folding clusters
# fold_01 = ((1 + 2 + 3 + 4 + 5 + 6 + 7 + 8 + 9 + 10) * 17) + (91 * 37) - (11 * 12) + (7 * 7 * 7)
# fold_02 = ((12 + 13 + 14 + 15 + 16 + 17 + 18 + 19) * 29) + (101 * 43) - (9 * 8 * 7) + (33 * 21)
# fold_03 = ((21 + 22 + 23 + 24 + 25 + 26 + 27 + 28) * 13) + (73 * 59) - (19 * 17) + (44 * 12)
# fold_04 = ((31 + 32 + 33 + 34 + 35 + 36 + 37 + 38) * 11) + (83 * 61) - (27 * 15) + (48 * 18)
# fold_05 = ((41 + 42 + 43 + 44 + 45 + 46 + 47 + 48) * 19) + (97 * 71) - (39 * 12) + (54 * 23)
# fold_06 = ((51 + 52 + 53 + 54 + 55 + 56 + 57 + 58) * 23) + (89 * 67) - (24 * 24) + (61 * 27)
# fold_07 = ((61 + 62 + 63 + 64 + 65 + 66 + 67 + 68) * 31) + (79 * 53) - (14 * 13) + (72 * 31)
# fold_08 = ((71 + 72 + 73 + 74 + 75 + 76 + 77 + 78) * 37) + (109 * 41) - (18 * 16) + (84 * 35)
# fold_09 = ((81 + 82 + 83 + 84 + 85 + 86 + 87 + 88) * 41) + (127 * 29) - (17 * 17) + (95 * 39)
# fold_10 = ((91 + 92 + 93 + 94 + 95 + 96 + 97 + 98) * 43) + (131 * 31) - (21 * 19) + (108 * 42)
# fold_11 = ((3 * 7 * 11) + (5 * 13 * 17) + (19 * 23)) * 9
# fold_12 = ((29 * 31) + (37 * 41) + (43 * 47) + (53 * 59)) - 777

# digest_a = fold_01 + fold_02 + fold_03 + fold_04
# digest_b = fold_05 + fold_06 + fold_07 + fold_08
# digest_c = fold_09 + fold_10 + fold_11 + fold_12
# grand_digest = digest_a + digest_b + digest_c

# page_shell = [:
<!doctype html>
<html>
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>[$slot]</title>
    <style>
        [$slot("style")]
    </style>
</head>
<body>
    [$slot("body")]
</body>
</html>
]

# global_styles = [$insert("style"), $css:
    :root {
        --accent: [accent];
        --accent-2: [accent_2];
        --accent-3: [accent_3];
        --border: [border];
        --bg-0: [bg_0];
        --bg-1: [bg_1];
        --bg-2: [bg_2];
        --text-main: [text_main];
        --text-muted: [text_muted];
    }

    * {
        box-sizing: border-box;
    }

    body {
        margin: 0;
        padding: 0;
        font-family: Inter, system-ui, sans-serif;
        background: linear-gradient(180deg, var(--bg-0), var(--bg-1));
        color: var(--text-main);
    }

    .page {
        width: min(1200px, calc(100% - 3rem));
        margin: 0 auto;
        padding: 2rem 0 5rem 0;
    }

    .section {
        margin-bottom: 1.25rem;
    }

    .card {
        border: 1px solid var(--border);
        background: linear-gradient(180deg, var(--bg-1), var(--bg-2));
        border-radius: 18px;
        padding: 1rem 1.1rem;
        box-shadow: 0 14px 48px rgba(0, 0, 0, 0.24);
    }

    .card-head {
        margin-bottom: 0.7rem;
    }

    .card-foot {
        margin-top: 0.9rem;
        color: var(--text-muted);
        font-size: 0.92rem;
    }

    .heading {
        margin: 0;
        font-size: 1.15rem;
        font-weight: 800;
        letter-spacing: 0.04rem;
    }

    .hero-title {
        margin: 0 0 0.5rem 0;
        font-size: 2rem;
        font-weight: 900;
    }

    .hero-note {
        color: var(--text-muted);
        line-height: 1.55;
    }

    .tight-list {
        margin: 0.7rem 0 0 1rem;
        padding: 0;
    }

    .tight-list li {
        margin: 0.2rem 0;
    }

    .runtime-grid {
        display: grid;
        grid-template-columns: repeat(3, minmax(0, 1fr));
        gap: 0.8rem;
    }

    .runtime-card {
        border: 1px solid var(--border);
        border-radius: 14px;
        padding: 0.9rem;
        background: rgba(255, 255, 255, 0.03);
    }

    .runtime-card.even {
        outline: 1px solid hsla(273, 92%, 63%, 0.28);
    }

    .runtime-card.odd {
        outline: 1px solid hsla(196, 91%, 58%, 0.28);
    }

    .runtime-table {
        display: grid;
        gap: 0.35rem;
    }

    .runtime-row {
        display: grid;
        grid-template-columns: 100px 1fr 120px;
        gap: 0.8rem;
        padding: 0.45rem 0.6rem;
        border-radius: 10px;
        border: 1px solid var(--border);
        background: rgba(255, 255, 255, 0.025);
    }

    .runtime-row.even {
        background: rgba(255, 255, 255, 0.03);
    }

    .runtime-row.odd {
        background: rgba(255, 255, 255, 0.05);
    }

    .matrix {
        display: grid;
        gap: 0.35rem;
    }

    .matrix-row {
        display: grid;
        grid-template-columns: repeat(18, minmax(0, 1fr));
        gap: 0.25rem;
    }

    .cell {
        display: block;
        text-align: center;
        font-size: 0.8rem;
        padding: 0.35rem 0.2rem;
        border: 1px solid var(--border);
        border-radius: 8px;
        background: rgba(255, 255, 255, 0.03);
    }

    .muted {
        color: var(--text-muted);
    }

    .hero-accent {
        color: var(--accent);
    }

    .hero-accent-2 {
        color: var(--accent-2);
    }

    .hero-accent-3 {
        color: var(--accent-3);
    }
]

# page = [$children([:<section class="section">[$slot]</section>]):
    <main class="page">
        [$slot]
    </main>
]

# card = [:
<article class="card">
    <div class="card-head">
        [$slot("head")]
    </div>
    <div class="card-body">
        [$slot]
    </div>
    <div class="card-foot">
        [$slot("foot")]
    </div>
</article>
]

# section_title = [:
<h2 class="heading" style="[$slot("style")]">
    [$slot]
</h2>
]

# accent_title = [$insert("style"):
color: [accent];
]

# accent_title_2 = [$insert("style"):
color: [accent_2];
]

# accent_title_3 = [$insert("style"):
color: [accent_3];
]

# bullet_list = [$children([:<li>[$slot]</li>]):
<ul class="tight-list">
    [$slot]
</ul>
]

# hero_head = [$insert("head"):
    [section_title, accent_title:
        Frontend benchmark overview
    ]
]

# hero_foot = [$insert("foot"):
    digest [grand_digest]
]

# hero_card = [card, hero_head, hero_foot:
    [:
        <h1 class="hero-title">
            Beanstalk <span class="hero-accent">speed test</span>
        </h1>
        <div class="hero-note">
            This file deliberately mixes heavy constant folding, many nested templates,
            slots, wrappers, directives, loops, collections, structs, and runtime template
            construction. It is intended to make AST and template work show up clearly in
            detailed timers.
        </div>
    ]
]

# bench_a_head = [$insert("head"):
    [section_title, accent_title:
        Compile-time block A
    ]
]

# bench_a_foot = [$insert("foot"):
    A checksum = [fold_01] / [fold_02] / [fold_03] / [fold_04]
]

# bench_b_head = [$insert("head"):
    [section_title, accent_title_2:
        Compile-time block B
    ]
]

# bench_b_foot = [$insert("foot"):
    B checksum = [fold_05] / [fold_06] / [fold_07] / [fold_08]
]

# bench_c_head = [$insert("head"):
    [section_title, accent_title_3:
        Compile-time block C
    ]
]

# bench_c_foot = [$insert("foot"):
    C checksum = [fold_09] / [fold_10] / [fold_11] / [fold_12]
]

# bench_d_head = [$insert("head"):
    [section_title, accent_title:
        Compile-time wrapper stress
    ]
]

# bench_d_foot = [$insert("foot"):
    digest_a [digest_a] / digest_b [digest_b] / digest_c [digest_c]
]

# compile_card_a = [card, bench_a_head, bench_a_foot:
    [$markdown:
        ### Fold cluster A

        This block is fully compile-time.

        It combines:
        - many arithmetic constants
        - markdown parsing
        - nested wrapper composition
        - named slot insertion
        - direct template interpolation

        Derived values:
        - fold_01 = [fold_01]
        - fold_02 = [fold_02]
        - fold_03 = [fold_03]
        - fold_04 = [fold_04]
    ]

    [bullet_list:
        [: compile-time section A1 ]
        [: compile-time section A2 ]
        [: compile-time section A3 ]
        [: checksum [digest_a] ]
    ]
]

# compile_card_b = [card, bench_b_head, bench_b_foot:
    [$markdown:
        ### Fold cluster B

        More compile-time work with different constants.

        These values are intentionally chained into additional digests:
        - fold_05 = [fold_05]
        - fold_06 = [fold_06]
        - fold_07 = [fold_07]
        - fold_08 = [fold_08]
    ]

    [bullet_list:
        [: markdown body + wrapper list ]
        [: slot insertion + named foot ]
        [: template interpolation of big constants ]
        [: checksum [digest_b] ]
    ]
]

# compile_card_c = [card, bench_c_head, bench_c_foot:
    [$markdown:
        ### Fold cluster C

        This adds another constant-heavy group to keep type resolution,
        constant resolution, and template folding busy.

        Derived values:
        - fold_09 = [fold_09]
        - fold_10 = [fold_10]
        - fold_11 = [fold_11]
        - fold_12 = [fold_12]
    ]

    [bullet_list:
        [: compile-time block C1 ]
        [: compile-time block C2 ]
        [: compile-time block C3 ]
        [: checksum [digest_c] ]
    ]
]

# compile_card_d = [card, bench_d_head, bench_d_foot:
    [$markdown:
        ### Wrapper and slot composition

        This card exists mostly to keep wrapper composition and slot handling active.

        Grand digest:
        - [grand_digest]

        Sub digests:
        - A [digest_a]
        - B [digest_b]
        - C [digest_c]
    ]

    [bullet_list:
        [: wrapper composition ]
        [: slot definitions ]
        [: named slot insertions ]
        [: markdown + html + css directive paths ]
        [: grand digest [grand_digest] ]
    ]
]

BenchmarkBlock = |
    title String,
    body String,
    seed Int,
|

render |this BenchmarkBlock, index Int| -> String:
    tone ~= "even"

    if index % 2 is not 0:
        tone ~= "odd"
    ;

    composite = this.seed * (index + 3) + (index * index) + 17

    return [:
        <article class="runtime-card [tone]">
            <h3>[this.title]</h3>
            <p class="muted">[this.body]</p>
            <div>seed [this.seed]</div>
            <div>index [index]</div>
            <div>composite [composite]</div>
        </article>
    ]
;

build_runtime_grid || -> String:
    items ~= {
        BenchmarkBlock("Runtime alpha", "Exercises collections, loops, receiver methods, and template construction.", 17),
        BenchmarkBlock("Runtime beta", "Builds up runtime strings by repeatedly wrapping previous content.", 23),
        BenchmarkBlock("Runtime gamma", "Adds more method-call and interpolation work.", 31),
        BenchmarkBlock("Runtime delta", "Uses the same receiver path but different data.", 43),
        BenchmarkBlock("Runtime epsilon", "Keeps AST parsing busy in function bodies.", 59),
        BenchmarkBlock("Runtime zeta", "Good for comparing node emission and template normalization.", 71),
        BenchmarkBlock("Runtime eta", "Another block to keep collection iteration non-trivial.", 83),
        BenchmarkBlock("Runtime theta", "Large enough to make the collection loop visible.", 97),
        BenchmarkBlock("Runtime iota", "Additional body parsing and runtime template churn.", 109),
    }

    grid ~= [:
        <div class="runtime-grid">
    ]

    loop items |item, index|:
        grid ~= [:
            [grid]
            [item.render(index)]
        ]
    ;

    grid ~= [:
        [grid]
        </div>
    ]

    return grid
;

build_runtime_rows || -> String:
    rows ~= [:
        <div class="runtime-table">
    ]

    loop 0 to 80 |row|:
        metric = ((row + 3) * (row + 7) * (row + 11)) - (row * row) + 19
        residue = metric % 97

        row_class ~= "even"
        if row % 2 is not 0:
            row_class ~= "odd"
        ;

        rows ~= [:
            [rows]
            <div class="runtime-row [row_class]">
                <span>row [row]</span>
                <span>metric [metric]</span>
                <span>residue [residue]</span>
            </div>
        ]
    ;

    rows ~= [:
        [rows]
        </div>
    ]

    return rows
;

build_runtime_matrix || -> String:
    matrix ~= [:
        <div class="matrix">
    ]

    loop 0 to 18 |outer|:
        row_template ~= [:
            <div class="matrix-row">
        ]

        loop 0 to 18 |inner|:
            value = ((outer + 1) * (inner + 2)) + (outer * inner) + 13

            row_template ~= [:
                [row_template]
                <span class="cell">[value]</span>
            ]
        ;

        row_template ~= [:
            [row_template]
            </div>
        ]

        matrix ~= [:
            [matrix]
            [row_template]
        ]
    ;

    matrix ~= [:
        [matrix]
        </div>
    ]

    return matrix
;

build_runtime_summary || -> String:
    summary ~= [:
        <div class="card">
            <div class="card-head">
                [section_title, accent_title_2:
                    Runtime summary
                ]
            </div>
            <div class="card-body">
    ]

    loop 0 to 24 |index|:
        value = ((index + 5) * (index + 9)) + (index * 3) + 41

        summary ~= [:
            [summary]
            <div>summary line [index] = [value]</div>
        ]
    ;

    summary ~= [:
            </div>
            <div class="card-foot">
                runtime summary generated with numeric loops and repeated template rebuilding
            </div>
        </div>
    ]

    return summary
;

runtime_grid = build_runtime_grid()
runtime_rows = build_runtime_rows()
runtime_matrix = build_runtime_matrix()
runtime_summary = build_runtime_summary()

body_insert = [$insert("body"):
    [page:
        [hero_card]
        [compile_card_a]
        [compile_card_b]
        [compile_card_c]
        [compile_card_d]
        [runtime_summary]

        [card, [$insert("head"):
            [section_title, accent_title_3:
                Runtime grid output
            ]
        ], [$insert("foot"):
            built from a collection of structs and a receiver method call in a loop
        ]:
            [runtime_grid]
        ]

        [card, [$insert("head"):
            [section_title, accent_title:
                Runtime rows output
            ]
        ], [$insert("foot"):
            built from a numeric loop with repeated runtime template rebuilding
        ]:
            [runtime_rows]
        ]

        [card, [$insert("head"):
            [section_title, accent_title_2:
                Runtime matrix output
            ]
        ], [$insert("foot"):
            built from nested loops for extra AST body parsing and template work
        ]:
            [runtime_matrix]
        ]
    ]
]

[page_shell, global_styles, body_insert:
    [page_title]
]