Response ::
    Success,
    Err |
        message String,
    |,
;

r = Response::Err("bad")

if r is:
    case Err(text as message) => io(text)
    case Success => io("ok")
;
