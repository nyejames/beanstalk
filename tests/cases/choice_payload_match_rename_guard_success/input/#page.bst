Status ::
    Ready,
    Loading |
        progress Int,
    |,
;

status = Status::Loading(42)

if status is:
    case Loading(progress as p) if p is 42 => io("forty-two")
    case Ready => io("ready")
    else => io("other")
;
