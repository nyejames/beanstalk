-- Basic HTML components and utility formatters for HTML project builds.
-- Provided by the HTML project builder as a builder source library.
import @web/canvas {
    Canvas2d,
    get_canvas as raw_get_canvas,
    context_2d as raw_context_2d,
    set_fill_style as raw_set_fill_style,
    fill_rect as raw_fill_rect,
    clear_rect as raw_clear_rect,
    stroke_rect as raw_stroke_rect,
}

-- Basic tags
export p #String = [$html:<p style="[$slot("style")]">[$slot]</p>]
export h1 #String = [$html:<h1 style="[$slot("style")]">[$slot]</h1>]
export h2 #String = [$html:<h2 style="[$slot("style")]">[$slot]</h2>]
export h3 #String = [$html:<h3 style="[$slot("style")]">[$slot]</h3>]
export h4 #String = [$html:<h4 style="[$slot("style")]">[$slot]</h4>]
export h5 #String = [$html:<h5 style="[$slot("style")]">[$slot]</h5>]
export h6 #String = [$html:<h6 style="[$slot("style")]">[$slot]</h6>]

export img #String = [$html:<img src="[$slot(1)]" style="[$slot("style")]" alt="[$slot]">]
export small #String = [$html:<small style="[$slot("style")]">[$slot]</small>]
export large #String = [$html:<large style="[$slot("style")]">[$slot]</large>]

-- Formatting helpers
export underline #String = [$html:<u style="[$slot("style")]">[$slot]</u>]
export strikethrough #String = [$html:<s style="[$slot("style")]">[$slot]</s>]
export table #String = [$html:
  <table style="[$slot("style")]">
    [$slot]
  </table>
]

-- Utility style inserts
export center #String = [$insert("style"):text-align: center;]
export right #String = [$insert("style"):text-align: right;]
export left #String = [$insert("style"):text-align: left;]
export smaller #String = [$insert("style"):font-size: smaller;]
export medium #String = [$insert("style"):font-size: medium;]
export larger #String = [$insert("style"):font-size: larger;]

-- Basic Canvas component
export CANVAS_ID #= "bst_builtin_canvas"
export canvas #String = [$html:<canvas id="[CANVAS_ID]" width="[$slot(2)]" height="[$slot(3)]" style="[$slot("style")]"></canvas>]

-- Get the basic canvas element and 2D context for drawing (opaque JS interop)
export get_canvas_context || -> Canvas2d, Error!:
    canvas_element = raw_get_canvas(CANVAS_ID)!
    return raw_context_2d(canvas_element)!
;

-- Source-owned wrapper that provides method-style canvas drawing ergonomics.
export Canvas = |
    context Canvas2d,
|

export get_canvas || -> Canvas, Error!:
    context = get_canvas_context()!
    return Canvas(context)
;

set_fill_style |this ~Canvas, color String|:
    raw_set_fill_style(~this.context, color)
;

fill_rect |this ~Canvas, x Float, y Float, width Float, height Float|:
    raw_fill_rect(~this.context, x, y, width, height)
;

clear_rect |this ~Canvas, x Float, y Float, width Float, height Float|:
    raw_clear_rect(~this.context, x, y, width, height)
;

stroke_rect |this ~Canvas, x Float, y Float, width Float, height Float|:
    raw_stroke_rect(~this.context, x, y, width, height)
;
