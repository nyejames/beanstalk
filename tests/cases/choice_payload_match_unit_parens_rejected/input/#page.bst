Response ::
    Success,
    Err |
        message String,
    |,
;

r = Response::Success

if r is:
    case Success() => io("ok")
    case Err(message) => io(message)
;
