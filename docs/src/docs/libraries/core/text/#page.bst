import @html {center}
import @styles/docs {navbar, title, section, codeblock, codesnippet, theme_head}

page_title #= "Core Text"
page_description #= "Text helper functions for String values."
page_head #= theme_head

#[navbar]
#[title, center: CORE TEXT]

#[section, $markdown:
    [codesnippet:@core/text] provides helper functions for [codesnippet:String] values.

    Current helpers:
    - [codesnippet:length]
    - [codesnippet:is_empty]
    - [codesnippet:contains]
    - [codesnippet:starts_with]
    - [codesnippet:ends_with]

    [codeblock, $code("bst"):
        import @core/text {length, contains}

        name = "Beanstalk"
        count = length(name)
        has_stalk = contains(name, "stalk")

        io([:count=[count] has_stalk=[has_stalk]])
    ]

    On the current HTML JavaScript path, text length follows JavaScript string length behavior.
]
