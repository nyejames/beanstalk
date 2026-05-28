import @helper { Counter as C, tick }

run || -> String:
    c = C(0)
    result = c.tick()
    return "source_type_alias_visibility_ok"
;

io(run())
