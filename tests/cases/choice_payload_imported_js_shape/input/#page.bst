import @types { Response }

e = Response::Err("bad")
if e is:
    Success => io("ok")
    Err(message) => io("err")
;
