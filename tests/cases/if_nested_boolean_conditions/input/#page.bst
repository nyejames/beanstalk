outer = true
inner = false

if outer:
    if not inner:
        io("nested true")
    else
        io("nested false")
    ;
else
    io("outer false")
;
