#Response :: Err | message String |, Success;

e = Response::Err("bad")
other = e
if other is:
    case Success => io("ok")
    case Err(message) => io("err")
;
