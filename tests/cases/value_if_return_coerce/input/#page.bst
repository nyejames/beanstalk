get_float |condition Bool| -> Float:
    return if condition then 1 else 2
;

value = get_float(true)
io("value if return coerce test")
