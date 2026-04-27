Response ::
    Success,
    Err |
        message String,
    |,
;

r = Response::Err("bad")

if r is:
    case Err(message) => io(message)
    case Err(other) => io(other)
    case Success => io("ok")
;
