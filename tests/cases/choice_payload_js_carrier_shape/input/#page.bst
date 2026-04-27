#Response :: Err | message String |, Success;

e = Response::Err("bad")
if e is:
    case Success => io("ok")
    case Err => io("err")
;
