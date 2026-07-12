import @html { canvas, Canvas, get_canvas }

#[canvas:
  Canvas helper import is reached at runtime.
  320
  180
]

draw || -> String, Error!:
    drawing ~= get_canvas("game_canvas")!
    ~drawing.set_fill_style("#202020")
    ~drawing.fill_rect(0.0, 0.0, 10.0, 10.0)
    return "canvas_context_ready"
;

status = draw() catch:
    then "canvas_context_unavailable"
;

io.line([: [status]])
