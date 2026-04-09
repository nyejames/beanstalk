-- Validate descending inference and explicit step semantics.

count_desc ~= 0
loop 5 to 0 |i|:
    count_desc = count_desc + 1
;

sum_even_desc ~= 0
loop 10 upto 0 by 2 |i|:
    sum_even_desc = sum_even_desc + i
;

sum_even_desc_negative ~= 0
loop 10 upto 0 by -2 |i|:
    sum_even_desc_negative = sum_even_desc_negative + i
;

[:loop_range_direction_and_step count_desc=[count_desc] sum_even_desc=[sum_even_desc] sum_even_desc_negative=[sum_even_desc_negative]]
