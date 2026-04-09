-- Validate conditional/range loop semantics through rendered totals.

counter ~= 0
loop counter < 3:
    counter = counter + 1
;

sum ~= 0
loop 0 to 5 |i|:
    sum = sum + i
;

inclusive_sum ~= 0
loop 0 upto 5 |i|:
    inclusive_sum = inclusive_sum + i
;

[:loop_conditional_and_range counter=[counter] sum_exclusive=[sum] sum_inclusive=[inclusive_sum]]
