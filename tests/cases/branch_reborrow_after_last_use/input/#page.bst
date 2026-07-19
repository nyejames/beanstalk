-- Test case: a mutable alias can be recreated after the previous mutable alias reaches its last use
-- This used to model inconsistent move outcomes, but under alias-by-default semantics it is valid.

data = "shared_data"
some_condition = true

first_ref ~= data

if some_condition:
    io.line([: branch_reborrow_after_last_use executed-first-reference=[first_ref]])
else
    io.line([: branch_reborrow_after_last_use alternate-first-reference=[first_ref]])
    second_ref ~= data
    io.line([: branch_reborrow_after_last_use alternate-second-reference=[second_ref]])
;
