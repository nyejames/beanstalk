#Status :: Ready, Busy;

current Status = Status::Ready

if current is:
    case Status::Unknown => io("unknown")
    else => io("fallback")
;
