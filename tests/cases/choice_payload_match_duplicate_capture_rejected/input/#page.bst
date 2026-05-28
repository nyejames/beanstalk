Response ::
    Success,
    Pending |
        retry_count Int,
        message String,
    |,
;

r = Response::Pending(3, "waiting")

if r is:
    Pending(retry_count, retry_count) => io("ok")
    Success => io("ok")
;
