Response ::
    Success,
    Err |
        message String,
    |,
;

r = Response::Err("bad")

if r is:
    case Err(message = text) => io(text)
    case Success => io("ok")
;
