Response ::
    Success,
    Err |
        message String,
    |,
;

r = Response::Err("bad")

if r is:
    case Err(message, extra) => io("ok")
    case Success => io("ok")
;
