Status :: Ready, Busy;

status = Status::Ready
if status is:
    case Ready => io("ready")
    case _ => io("other")
;
