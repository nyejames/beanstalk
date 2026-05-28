pair |condition Bool| -> Int, String:
    return if condition:
        then 1, "yes"
    else
        then 0, "no"
    ;
;

a, b = pair(false)
io("value if return multi block test")
