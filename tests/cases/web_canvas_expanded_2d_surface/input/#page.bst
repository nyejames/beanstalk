import @web/canvas

run |id String| -> String, Error!:
    canvas_ref ~= canvas.get_canvas(id)!
    ~canvas_ref.set_canvas_size(320, 180)

    ctx ~= canvas.context_2d(canvas_ref)!
    ~ctx.save()
    ~ctx.set_fill_style("#112233")
    ~ctx.set_stroke_style("white")
    ~ctx.set_line_width(2.0)
    ~ctx.set_line_cap("round")
    ~ctx.set_line_join("round")
    ~ctx.set_global_alpha(0.95)
    ~ctx.set_image_smoothing_enabled(true)
    ~ctx.clear_rect(0.0, 0.0, 320.0, 180.0)
    ~ctx.fill_rect(4.0, 4.0, 80.0, 32.0)
    ~ctx.stroke_rect(4.0, 4.0, 80.0, 32.0)

    ~ctx.begin_path()
    ~ctx.move_to(12.0, 70.0)
    ~ctx.line_to(52.0, 100.0)
    ~ctx.quadratic_curve_to(76.0, 120.0, 96.0, 80.0)
    ~ctx.arc(96.0, 80.0, 12.0, 0.0, 6.28)!
    ~ctx.close_path()
    ~ctx.stroke()

    gradient ~= ctx.create_linear_gradient(0.0, 0.0, 120.0, 0.0)!
    ~gradient.add_color_stop(0.0, "red")!
    ~gradient.add_color_stop(1.0, "blue")!
    ~ctx.set_fill_gradient(gradient)
    ~ctx.fill_rect(120.0, 8.0, 80.0, 32.0)

    ~ctx.translate(10.0, 10.0)
    ~ctx.rotate(0.1)
    ~ctx.scale(1.0, 1.0)
    ~ctx.reset_transform()

    ~ctx.set_font("16px sans-serif")
    ~ctx.set_text_align("left")
    ~ctx.set_text_baseline("top")
    ~ctx.set_direction("ltr")
    ~ctx.set_letter_spacing("0px")
    ~ctx.set_word_spacing("0px")
    ~ctx.set_font_kerning("normal")
    ~ctx.set_text_rendering("geometricPrecision")
    ~ctx.set_lang("en")
    ~ctx.fill_text("Canvas", 10.0, 130.0)
    metrics = ctx.measure_text("Canvas")
    width = canvas.text_width(metrics)

    image_data ~= ctx.create_image_data(2, 2)!
    ~image_data.image_data_set_pixel(0, 0, 255, 0, 0, 255)!
    red = canvas.image_data_get_red(image_data, 0, 0)!
    image_width = canvas.image_data_width(image_data)
    ~ctx.put_image_data(image_data, 210.0, 8.0)!

    exported = canvas.to_data_url_type(canvas_ref, "image/png")!
    context_lost = ctx.is_context_lost()
    ~ctx.restore()

    return "expanded_canvas_ok"
;

result = run("game") catch:
    then "expanded_canvas_error"
;
io(result)
