parse_user |name String| -> String, Float, Error!:
    if name is "":
        return! Error("Parse", "parse.user.empty", "name is empty")
    ;

    return name, 1.5
;

render_user |name String| -> String:
    user, score = parse_user(name) err! "guest", 0.0:
        io(err.code)
    ;

    return [:[user]-[score]]
;

output = render_user("")
io(output)
