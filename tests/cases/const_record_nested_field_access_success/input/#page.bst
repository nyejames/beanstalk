Swatch = |
    label String = "red",
|

Palette = |
    primary Swatch = Swatch(),
|

palette #= Palette()

io.line([: [palette.primary.label]])
