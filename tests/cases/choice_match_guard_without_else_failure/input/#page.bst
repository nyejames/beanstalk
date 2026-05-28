Status :: Ready, Busy;

status Status = Status::Ready
if status is:
    Ready if true => io("ready")
    Busy => io("busy")
;
