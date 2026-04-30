#Response :: Success, Err | message String |;

e = Response::Err("bad")
if e is Response::Err("bad"):
    io("ok")
;
