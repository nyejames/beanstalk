#Status :: Ready, Busy, Offline;

current Status = Status::Busy
match_result ~= "unset"

if current is:
    case Ready => match_result = "ready"
    case Status::Busy => match_result = "busy"
    case Offline => match_result = "offline"
;

[:choice_match_exhaustive_success result=[match_result]]
