#Status :: Ready, Busy;

current ~= Status::Ready
current = Status::Busy
label ~= "unset"
if current is:
    case Ready => label = "ready";
    case Busy => label = "busy";
;
[:choice_assignment_flow result=[label]]
