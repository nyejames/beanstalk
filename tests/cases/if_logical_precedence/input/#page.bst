a = true
b = false
c = false

if not a and b:
    io("unexpected not/and")
;

if a or b and c:
    io("or-after-and")
;

if (a or b) and c:
    io("unexpected grouped")
;

io("if precedence done")
