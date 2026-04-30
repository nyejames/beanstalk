-- Nested payload patterns should be rejected with a clear deferred diagnostic.

Response ::
    Success,
    Err |
        message String,
    |,
;

r = Response::Err("bad")

if r is:
    case Err(message(nested)) => io(nested)
    else => io("other")
;
