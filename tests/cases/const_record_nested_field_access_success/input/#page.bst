Swatch = |
    label String = "red",
|

Palette = |
    primary Swatch = Swatch(),
|

palette #= Palette()

io(palette.primary.label)
