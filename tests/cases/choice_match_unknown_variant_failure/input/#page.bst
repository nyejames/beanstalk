#Status :: Ready, Busy;

current Status = Status::Ready

if current is:
    case Unknown => io("unknown")
    else => io("fallback")
;
