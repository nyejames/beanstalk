Status :: Ready, Busy;

status = Status::Ready
if status is:
    Ready => io("ready")
    _ => io("other")
;
