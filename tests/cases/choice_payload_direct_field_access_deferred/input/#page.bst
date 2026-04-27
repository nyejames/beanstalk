Response :: Success, Err | message String |;

e = Response::Err("bad")
io(e.message)
