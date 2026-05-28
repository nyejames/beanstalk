pair |condition Bool| -> Float, String:
    return if condition then 1, "yes" else 2, "no"
;

a, b = pair(true)
io("value if return multi coerce test")
