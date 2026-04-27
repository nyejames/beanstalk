Response ::
    Success,
    Err |
        message String,
    |,
;

message = "outer"
r = Response::Err("bad")

if r is:
    case Err(message) => io(message)
    case Success => io("ok")
;
