pair |condition Bool| -> Int, String:
    return if condition then 1, "yes" else 0, "no"
;

a, b = pair(true)
io([:value if return multi inline first=[a] second=[b]])
