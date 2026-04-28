Response ::
    Success,
    Err |
        message String,
    |,
;

response = Response::Err("bad")

if response is:
    case Err(message as message) => io(message)
    case Success => io("ok")
;
