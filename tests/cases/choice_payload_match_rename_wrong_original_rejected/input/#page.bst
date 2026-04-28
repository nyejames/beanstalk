Response ::
    Success,
    Err |
        message String,
    |,
;

response = Response::Err("bad")

if response is:
    case Err(text as error_message) => io(error_message)
    case Success => io("ok")
;
