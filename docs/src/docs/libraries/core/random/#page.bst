import @html {center}
import @styles/docs {navbar, title, section, codeblock, codesnippet, theme_head}

page_title #= "Core Random"
page_description #= "Random number helpers in the core library."
page_head #= theme_head

#[navbar]
#[title, center: CORE RANDOM]

#[section, $markdown:
    [codesnippet:@core/random] provides basic random number helpers for the HTML JavaScript path.

    [codesnippet:random_float()] returns a [codesnippet:Float].
    [codesnippet:random_int(min, max)] returns an [codesnippet:Int] in the inclusive range from [codesnippet:min] to [codesnippet:max].
    If [codesnippet:min] is greater than [codesnippet:max], the bounds are swapped.

    [codeblock, $code("bst"):
        import @core/random {random_float, random_int}

        value = random_float()
        die_roll = random_int(1, 6)

        io([:roll=[die_roll] value=[value]])
    ]

    Seeded random is deferred.
]
