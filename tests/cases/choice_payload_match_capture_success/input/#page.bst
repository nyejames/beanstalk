Response ::
    Success,
    Err |
        message String,
    |,
    Pending |
        retry_count Int,
        message String,
    |,
;

r = Response::Err("bad")

if r is:
    case Success => io("ok")
    case Err(message) => io(message)
    case Pending(retry_count, message) => io(message)
;
