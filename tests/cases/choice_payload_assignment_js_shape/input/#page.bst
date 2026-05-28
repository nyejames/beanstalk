Response :: Err | message String |, Success;

e = Response::Err("bad")
other = e
if other is:
    Success => io("ok")
    Err(message) => io("err")
;
