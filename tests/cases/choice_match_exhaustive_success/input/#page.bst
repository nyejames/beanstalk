#Status :: Ready, Busy, Offline;

current Status = Status::Busy

if current is:
    case Ready => io("ready");
    case Status::Busy => io("busy");
    case Offline => io("offline");
;
