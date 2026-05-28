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
    Success => io("ok")
    Err(message) => io(message)
    Pending(retry_count, message) => io(message)
;
