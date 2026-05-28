Response ::
    Success,
    Err |
        message String,
    |,
;

message = "outer"
r = Response::Err("bad")

if r is:
    Err(message) => io(message)
    Success => io("ok")
;
