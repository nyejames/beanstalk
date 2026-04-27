#Response :: Err | message String |, Pending | retry_count Int, message String |, Success;

e = Response::Err("bad")
p = Response::Pending(3, "waiting")
io("ok")
