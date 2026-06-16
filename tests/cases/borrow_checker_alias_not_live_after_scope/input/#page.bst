value ~= 10

if true:
    alias = value
    io.line([: [alias]])
;

-- Reassignment after the branch — alias is gone, this must succeed
value = 20
io.line([: [value]])
