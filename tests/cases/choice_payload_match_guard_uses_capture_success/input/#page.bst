Response ::
    Success,
    Err |
        message String,
    |,
;

r = Response::Err("bad")

if r is:
    Err(message) if message is not "" => io(message)
    Success => io("ok")
    else => io("else")
;
