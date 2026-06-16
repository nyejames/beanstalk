Response :: Success, Err | message String |;

a = Response::Success
b = Response::Err("bad")

if a is b:
    io.line("same")
else
    io.line("different")
;
