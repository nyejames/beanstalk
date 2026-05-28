import @web/canvas

run |id String| -> String, Error!:
    canvas_ref = canvas.get_canvas(id)!
    ctx ~= canvas.context_2d(canvas_ref)!
    ~ctx.set_fill_style("red")
    ~ctx.fill_rect(0.0, 0.0, 100.0, 100.0)
    return "canvas_ok"
;

result = run("game") catch:
    then "error"
;
io(result)
