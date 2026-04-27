Response ::
    Success,
    Pending |
        retry_count Int,
        message String,
    |,
;

r = Response::Pending(3, "waiting")

if r is:
    case Pending(retry_count, retry_count) => io("ok")
    case Success => io("ok")
;
