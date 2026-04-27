Response ::
    Success,
    Err |
        message String,
    |,
;

r = Response::Err("bad")

if r is:
    case Response::Err(message) => io(message)
    case Response::Success => io("ok")
;
