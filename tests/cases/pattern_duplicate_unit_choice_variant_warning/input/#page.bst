#Status :: Ready, Busy, Offline;

current Status = Status::Busy
result ~= "unset"

if current is:
    case Ready => result = "ready"
    case Busy => result = "busy"
    case Ready => result = "ready again"
    case Offline => result = "offline"
;

[:pattern_duplicate_unit_choice_variant_warning result=[result]]
