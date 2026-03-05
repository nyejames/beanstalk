Basic = |
    document String = [:
        <head>
            [..]
        </head>
        <body>[$markdown: [..]]</body>
    ],
    page String = [$markdown:<section>[..]</section>],

    -- Basic formatting
    center String = [:<span align="center">[..]<span/>],
    underline String = [:<u>[..]</u>],
    strikethrough String = [:<s>[..]</s>],
|

-- Export a constant instance of the basic library
-- Using all of its default values
# basic = Basic()


#[basic.page: Hello world]