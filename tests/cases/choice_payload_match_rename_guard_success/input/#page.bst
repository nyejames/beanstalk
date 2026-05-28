Status ::
    Ready,
    Loading |
        progress Int,
    |,
;

status = Status::Loading(42)

if status is:
    Loading(progress as p) if p is 42 => io("forty-two")
    Ready => io("ready")
    else => io("other")
;
