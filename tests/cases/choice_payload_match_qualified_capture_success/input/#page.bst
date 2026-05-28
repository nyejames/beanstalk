Response ::
    Success,
    Err |
        message String,
    |,
;

r = Response::Err("bad")

if r is:
    Response::Err(message) => io(message)
    Response::Success => io("ok")
;
