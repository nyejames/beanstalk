#Status :: Ready, Busy;

status Status = Status::Ready
if status is:
    case Ready if true => io("ready")
    case Busy => io("busy")
;
