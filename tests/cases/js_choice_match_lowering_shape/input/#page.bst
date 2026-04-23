#Status :: Ready, Busy;
current Status = Status::Ready
result ~= "unset"
if current is:
    case Ready => result = "ready";
    case Busy => result = "busy";
;
io(result)
