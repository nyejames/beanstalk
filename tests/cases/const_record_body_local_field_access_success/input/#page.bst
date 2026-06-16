Palette = |
    red String = "red",
    green String = "green",
|

show_red || -> String:
    palette #= Palette()
    return palette.red
;

io.line([: [show_red()]])
