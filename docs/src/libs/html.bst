-- This is a library of basic HTML components and styles
-- This will not contain specific pallets or themes,
-- just basic HTML components and styles
Basic = |
    body String = [$markdown:<body>[..]</body>],
    page String = [$markdown:<section>[..]</section>],

    -- Basic formatting
    center String = [:<span align="center">[..]<span/>],
    underline String = [:<u>[..]</u>],
    strikethrough String = [:<s>[..]</s>],
|

-- Export a constant instance of the basic library
-- Using all of its default values
# basic = Basic()