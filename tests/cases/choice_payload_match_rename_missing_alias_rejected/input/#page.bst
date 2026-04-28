Response ::
    Success,
    Err |
        message String,
    |,
;

response = Response::Err("bad")

if response is:
    case Err(message as) => io("bad")
    case Success => io("ok")
;
