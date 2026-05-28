import @web/canvas { fill_rect }

run || -> String:
    return "should_fail"
;

io(run())
