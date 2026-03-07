Basic = |
    document String = [:
        <head>
            [$slot]
        </head>
        <body>[$markdown: [$slot]]</body>
    ],
    page String = [$markdown:<section>[$slot]</section>],

    -- Basic formatting
    center String = [:<span align="center">[$slot]<span/>],
    underline String = [:<u>[$slot]</u>],
    strikethrough String = [:<s>[$slot]</s>],
|

-- Export a constant instance of the basic library
-- Using all of its default values
# basic = Basic()


#[basic.page: Hello world]