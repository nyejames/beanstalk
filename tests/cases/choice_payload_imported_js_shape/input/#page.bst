import @types/Response

e = Response::Err("bad")
if e is:
    case Success => io("ok")
    case Err(message) => io("err")
;
