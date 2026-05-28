Response ::
    Success,
    Err |
        message String,
    |,
;

response = Response::Err("bad")

if response is:
    Err(message as 1) => io("bad")
    Success => io("ok")
;
