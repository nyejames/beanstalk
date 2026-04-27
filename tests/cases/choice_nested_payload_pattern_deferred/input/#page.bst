Response :: Success, Err | message String |;

e = Response::Err("bad")
if e is:
    case Err(message(x)) => io(x)
;
