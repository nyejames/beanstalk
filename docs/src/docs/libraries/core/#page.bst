import @html {center}
import @styles/docs {navbar, title, section, theme_head}

page_title #= "Core Libraries"
page_description #= "Overview of Beanstalk core libraries and their current HTML/JS support."
page_head #= theme_head

#[navbar]
#[title, center: CORE LIBRARIES]

#[section, $markdown:
    Core libraries are small, typed packages provided by a builder.
    They are not ordinary source files in your project, but they are imported with the same `import` syntax.

    Most core libraries require explicit imports.
    The prelude keeps only the smallest always-available surface bare, such as `io`.

    Builder support can vary.
    The @../../progress (implementation matrix) is the source of truth for current package and backend support.
    The HTML builder currently supports optional core packages on the JavaScript path.
]

#[section, $markdown:
    ## Current pages

    - @./prelude (Prelude)
    - @./io (IO)
    - @./collections (Collections)
    - @./math (Math)
    - @./text (Text)
    - @./random (Random)
    - @./time (Time)
]
