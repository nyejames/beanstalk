#Response :: Success, Err | message String |;

if Response::Err(message = "bad") is Response::Err(message = "bad"):
    io("equal")
;

if Response::Err(message = "bad") is Response::Err(message = "good"):
    io("different")
else
    io("not equal")
;
