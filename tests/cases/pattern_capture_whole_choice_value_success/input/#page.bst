Status :: Ready, Busy;

status = Status::Busy
result ~= ""

if status is:
    case Ready => result = "ready"
    case captured => result = "captured"
    else => result = "else"
;

[:pattern_capture_whole_choice_value_success result=[result]]
