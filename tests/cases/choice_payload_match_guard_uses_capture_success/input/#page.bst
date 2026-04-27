Response ::
    Success,
    Err |
        message String,
    |,
;

r = Response::Err("bad")

if r is:
    case Err(message) if message is not "" => io(message)
    case Success => io("ok")
    else => io("else")
;
