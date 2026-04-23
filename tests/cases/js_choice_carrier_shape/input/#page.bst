#Status :: Ready, Busy, Offline;
current Status = Status::Ready
result ~= "ok"
if current is:
    case Ready => result = "ready";
    case Busy => result = "busy";
    case Offline => result = "offline";
;
io(result)
