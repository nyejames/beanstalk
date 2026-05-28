Status :: Ready, Busy;

current Status = Status::Ready

if current is:
    Status::Unknown => io("unknown")
    else => io("fallback")
;
