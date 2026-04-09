rhs_or || -> Bool:
    io("rhs-or-called")
    return false
;

lhs = true
value = lhs or rhs_or()

if value:
    io("or-true")
;

io("or-done")
