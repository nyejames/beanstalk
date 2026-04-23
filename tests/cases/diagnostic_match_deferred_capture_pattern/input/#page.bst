#Status :: Ready, Busy;

current Status = Status::Ready

#handle |status Status|:
    if status is:
        case Ready |payload| => io("ready")
        else => io("other")
    ;
;

handle(current)
