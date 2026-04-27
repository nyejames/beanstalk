Response :: Success, Err | message String |;

e = Response::Err("bad")
if e is:
    case Err(text as message) => io(text)
;
