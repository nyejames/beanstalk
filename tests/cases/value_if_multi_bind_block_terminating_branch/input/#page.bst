get_values |condition Bool| -> Int, String:
    first, second = if condition:
        return 9, "early"
    else
        then 0, "late"
    ;
    return first, second
;

a, b = get_values(false)
io("terminating")
io(a)
io(b)
