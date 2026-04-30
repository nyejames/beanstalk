#Response :: Success, Err | message String |;

a = Response::Success
b = Response::Err("bad")

if a is b:
    io("same")
else
    io("different")
;
