-- General capture pattern must not shadow an existing visible binding.

helper ||:
    io("helper")
;

value ~= 1
if value is:
    case helper => io(helper)
    else => io("other")
;
