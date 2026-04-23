#Status :: Ready, Busy, Offline;
#Phase :: Start, Run, End;

status Status = Status::Busy
phase Phase = Phase::Run

result ~= "unset"

if status is:
    case Busy => result = "busy";
    case Ready => result = "ready";
    case Offline => result = "offline";
;

if phase is:
    case Run => result = result + "-run";
    case Start => result = result + "-start";
    case End => result = result + "-end";
;

[:choice_nested_match_flow result=[result]]
