#Response :: Err | message String |, Success;

e = Response::Err("bad")
if e is:
    case Success => io("ok")
    case Err(message) => io("err")
;
