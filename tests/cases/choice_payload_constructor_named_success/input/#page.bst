#Response :: Err | message String |, Pending | retry_count Int, message String |, Success;

e = Response::Err(message = "bad")
p = Response::Pending(retry_count = 3, message = "waiting")
io("ok")
