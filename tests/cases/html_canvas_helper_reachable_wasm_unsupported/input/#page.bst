import @html { get_canvas_context }

draw || -> String, Error!:
    context = get_canvas_context()!
    return "canvas_context_ready"
;

status = draw() catch:
    then "canvas_context_unavailable"
;

io(status)

