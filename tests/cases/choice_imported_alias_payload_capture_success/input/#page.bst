import @choices/Response/Response as Reply

e = Reply::Err("captured")

if e is:
    case Reply::Success => io("ok")
    case Reply::Err(message) => io(message)
;
