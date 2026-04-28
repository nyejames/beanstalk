Response ::
    Success,
    Err |
        message String,
    |,
;

error_message = "outer"
response = Response::Err("bad")

if response is:
    case Err(message as error_message) => io(error_message)
    case Success => io("ok")
;
