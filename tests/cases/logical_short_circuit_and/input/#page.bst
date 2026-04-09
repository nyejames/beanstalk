rhs_and || -> Bool:
    io("rhs-and-called")
    return true
;

lhs = false
value = lhs and rhs_and()

if value:
    io("and-true")
;

io("and-done")
