can_error || -> Int, Error!:
    return! Error("bad")
;

recover || -> Int:
    return can_error() catch:
        then 0
        io("late")
    ;
;

io(recover())
io("then tail after value production is unreachable")
