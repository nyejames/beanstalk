#Response :: Success, Err | message String |;

a = Response::Err("bad")
b = Response::Err("good")

if a is b:
    io("same")
else
    io("different")
;
