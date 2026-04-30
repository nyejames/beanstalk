#Response :: Success, Err | message String |;

if Response::Err("bad") is Response::Err("bad"):
    io("equal")
;

if Response::Err("bad") is Response::Err("good"):
    io("different")
else
    io("not equal")
;
