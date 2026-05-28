Response ::
    Success,
    Err |
        message String,
    |,
;

r = Response::Success

if r is:
    Success() => io("ok")
    Err(message) => io(message)
;
