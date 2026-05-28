can_error |fail Bool| -> String, Int, Error!:
    if fail:
        return! Error("failed")
    ;

    return "ok", 1
;

local_recover || -> String, Int:
    name, count = can_error(true) catch:
        then "local", 10
    ;
    return name, count
;

propagate_recover || -> String, Int, Error!:
    name, count = can_error(true)!
    return name, count
;

local_name, local_count = local_recover()
prop_name, prop_count = propagate_recover() catch:
    then "prop", 20
;
io(local_name)
io(local_count)
io(prop_name)
io(prop_count)
