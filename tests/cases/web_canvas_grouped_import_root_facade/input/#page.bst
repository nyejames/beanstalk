import @web/canvas { get_canvas, context_2d, Canvas2d }

uses_grouped_canvas_imports |context Canvas2d| -> String, Error!:
    return "grouped_canvas_ok"
;

io.line("grouped_canvas_imports_resolved")
