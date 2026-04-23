#Status :: Ready, Busy;

status Status = Status::Ready
label ~= "unset"

if status is:
    case Ready => label = "ready"
    case Busy => label = "busy"
;

suffix ~= "-done"
result = label + suffix
[:choice_match_wildcard_merge_continuation result=[result]]
