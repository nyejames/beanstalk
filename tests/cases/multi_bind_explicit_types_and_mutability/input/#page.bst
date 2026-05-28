triple || -> Int, String, Float:
    return 1, "two", 3.0
;

a, b String, c ~Float = triple()
c = c + 1.0
io(a)
io(b)
io(c)
