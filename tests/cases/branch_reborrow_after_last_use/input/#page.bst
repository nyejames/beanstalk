-- Test case: a mutable alias can be recreated after the previous mutable alias reaches its last use
-- This used to model inconsistent move outcomes, but under alias-by-default semantics it is valid.

data = "shared_data"
some_condition = true

first_ref ~= data

if some_condition:
    io.line([: [first_ref]])
else
    io.line([: [first_ref]])
    second_ref ~= data
    io.line([: [second_ref]])
;
