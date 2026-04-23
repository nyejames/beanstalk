#Status :: Ready, Busy, Offline;

get_status || -> Status:
    return Status::Busy
;

current Status = get_status()
label ~= "unset"
if current is:
    case Ready => label = "ready"
    case Busy => label = "busy"
    case Offline => label = "offline"
;
[:choice_function_return_flow result=[label]]
