-- This is a library of basic HTML components and styles
Basic = |
    body String = #[:<body>[..]</body>],

    -- Colors
    red String = #[:<span style="color: red;">[..]</span>],
    green String = #[:<span style="color: green;">[..]</span>],
    blue String = #[:<span style="color: blue;">[..]</span>],
|

-- Export a constant instance of the basic library
-- Using all of its default values
#basic = Basic()