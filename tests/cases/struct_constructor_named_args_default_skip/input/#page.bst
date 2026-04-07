Config = |
    height Int = 100,
    width Int,
|

-- Skip 'height' (uses default 100), provide only 'width' by name
c = Config(width = 80)
io([: [c.width]])
