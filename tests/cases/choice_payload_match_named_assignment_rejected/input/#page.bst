Response ::
    Success,
    Err |
        message String,
    |,
;

r = Response::Err("bad")

if r is:
    Err(message = text) => io(text)
    Success => io("ok")
;
