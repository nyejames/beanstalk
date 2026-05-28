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
    Success => io("done")
    Err(message) => io(message)
    Pending(retry_count, message) => io(["[retry_count] [message]"])
;
