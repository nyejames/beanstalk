import @lib/html/{doc, format}
-- Some basic testing styles for the documentation website

# header = [:
    [doc.prelude, doc.en]
    [doc.head, $html:
        <meta charset="UTF-8">
        <meta name="viewport" content="width=device-width, initial-scale=1">
        <link rel="icon" href="[$slot("favicon")]">
        [doc.title: Beanstalk Documentation]
        [doc.style:
            [$slot("css")]
        ]
    ]
]

# body = [:
    [doc.body:
        [$slot]
    ]
]

# section = [$html:
    <section class="container">[$slot]</section>
]

# navbar = [$html:
    <nav class="bst-nav-1" aria-label="Primary">
        <a
            href="[/@]"
            style="
                display: flex;
                align-items: center;
                gap: 0.6rem;
                padding: 0.9rem 0;
                color: light-dark(var(--bst-text-lightmode), var(--bst-text-darkmode));
                text-decoration: none;
                font-weight: 700;
                letter-spacing: 0.04rem;
            "
        >
            <span
                aria-hidden="true"
                style="
                    font-family: var(--bst-font-family-emoji);
                    font-size: 1.4rem;
                    line-height: 1;
                "
            >🌱</span>

            <span>Beanstalk</span>
        </a>

        <div
            style="
                display: flex;
                align-items: center;
                gap: 1rem;
            "
        >
            <a
                href="/beanstalk/docs"
                style="
                    color: light-dark(var(--bst-text-lightmode), var(--bst-text-darkmode));
                    text-decoration: none;
                    font-weight: 500;
                    padding: 0.35rem 0;
                "
            >
                Docs
            </a>

            <a
                href="https://github.com/nyejames/beanstalk"
                target="_blank"
                rel="noreferrer"
                aria-label="Beanstalk GitHub repository"
                style="
                    display: inline-flex;
                    align-items: center;
                    justify-content: center;
                    color: light-dark(var(--bst-text-lightmode), var(--bst-text-darkmode));
                    padding: 0.35rem 0;
                "
            >
                <svg
                    xmlns="http://www.w3.org/2000/svg"
                    viewBox="0 0 16 16"
                    width="22"
                    height="22"
                    fill="currentColor"
                    aria-hidden="true"
                >
                    <path d="M8 0C3.58 0 0 3.58 0 8c0 3.54 2.29 6.53 5.47 7.59.4.07.55-.17.55-.38
                    0-.19-.01-.82-.01-1.49-2.01.37-2.53-.49-2.69-.94-.09-.23-.48-.94-.82-1.13-.28-.15-.68-.52
                    -.01-.53.63-.01 1.08.58 1.23.82.72 1.21 1.87.87 2.33.66.07-.52.28-.87.51-1.07
                    -1.78-.2-3.64-.89-3.64-3.95 0-.87.31-1.59.82-2.15-.08-.2-.36-1.02.08-2.12 0 0 .67-.21
                    2.2.82a7.65 7.65 0 0 1 4 0c1.53-1.04 2.2-.82 2.2-.82.44 1.1.16 1.92.08 2.12.51.56.82
                    1.27.82 2.15 0 3.07-1.87 3.75-3.65 3.95.29.25.54.73.54 1.48 0 1.07-.01 1.93-.01 2.2
                    0 .21.15.46.55.38A8.01 8.01 0 0 0 16 8c0-4.42-3.58-8-8-8Z"/>
                </svg>
            </a>
        </div>
    </nav>
]

# footer = [$html:
    <div></div>
]

# table = [format.table:
    [$insert("style"):border-collapse: collapse; border: 1px solid; padding: 1em;]
    [$slot]
]

# row = [$html:
    <tr style="border-bottom: 1px dotted grey;">[$reset, $children([$html:<td style="padding: 0.2em 0.5em;">[$slot]</td>]):[$slot]]</tr>
]

# header_row = [$html:
    <tr style="background-color: hsla(107, 100%, 36%, 0.23);">
        [$reset, $children([$html:
            <th style="border: 1px solid; padding: 0.5em; text-align: left;">[$slot]</th>
        ]):[$slot]]
    </tr>
]

# codeblock = [$html:
    <pre style="background-color: hsla(0, 0%, 42%, 0.12); padding-left: 1em; border-radius: 5px; max-width: 100%; white-space: pre; overflow-x: auto;">
        [$slot]
    </pre>
]

# title = [$html: <h1 style="font-size: 2em;[$slot("style")]">[$slot]</h1>]

Palette = |
    -- Colors for the documentation website
    red String = [$html:<span style="color: hsl(0, 73%, 55%);">[$slot]</span>],
    green String = [$html:<span style="color: green;">[$slot]</span>],
    blue String = [$html:<span style="color: blue;">[$slot]</span>],
|

# palette = Palette()
