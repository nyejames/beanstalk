-- Test that the dispatcher handles compound control flow correctly.
-- Combines a conditional loop with branching inside the loop body.

sum ~= 0
counter ~= 0
loop counter < 5:
    if counter > 2:
        sum = sum + 100
    else
        sum = sum + 1
    ;
    counter = counter + 1
;
io(sum)
