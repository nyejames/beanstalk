-- Conditional + range loop integration coverage
counter ~= 0
loop counter < 3:
    io([: conditional counter = [counter] ])
    counter = counter + 1
;

sum ~= 0
loop 0 to 5 |i|:
    sum = sum + i
;
io([: sum_exclusive = [sum] ])

inclusive_sum ~= 0
loop 0 upto 5 |i|:
    inclusive_sum = inclusive_sum + i
;
io([: sum_inclusive = [inclusive_sum] ])
