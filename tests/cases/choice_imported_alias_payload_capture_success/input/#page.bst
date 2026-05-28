import @choices/Response { Response as Reply }

e = Reply::Err("captured")

if e is:
    Reply::Success => io("ok")
    Reply::Err(message) => io(message)
;
