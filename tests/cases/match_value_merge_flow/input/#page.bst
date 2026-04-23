value ~= 2
picked ~= 0

if value is:
    case 1 => picked = 10
    case 2 => picked = 20
    else => picked = 30
;

result = picked + 5
[:match_value_merge_flow result=[result]]
