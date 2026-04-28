Response ::
    Success,
    Err |
        message String,
    |,
;

response = Response::Err("bad")

if response is:
    case Err(message as error_message) => io(message)
    case Success => io("ok")
;
