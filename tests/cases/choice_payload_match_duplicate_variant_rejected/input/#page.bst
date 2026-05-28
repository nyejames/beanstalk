Response ::
    Success,
    Err |
        message String,
    |,
;

r = Response::Err("bad")

if r is:
    Err(message) => io(message)
    Err(other) => io(other)
    Success => io("ok")
;
