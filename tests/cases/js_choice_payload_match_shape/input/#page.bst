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

r = Response::Pending(3, message = "waiting")
if r is:
    case Success => io("done")
    case Err(message) => io(message)
    case Pending(retry_count, message) => io(["[retry_count] [message]"])
;
