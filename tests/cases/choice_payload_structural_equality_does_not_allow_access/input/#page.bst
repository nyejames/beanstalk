Response :: Success, Err | message String |;

a = Response::Err("bad")
b = Response::Err("bad")

if a is b:
    io("equal")
;
