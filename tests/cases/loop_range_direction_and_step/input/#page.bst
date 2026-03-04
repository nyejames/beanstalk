-- Descending inference and explicit step normalization
count_desc ~= 0
loop i in 5 to 0:
    count_desc = count_desc + 1
;
io([: descending_count = [count_desc] ])

sum_even_desc ~= 0
loop i in 10 upto 0 by 2:
    sum_even_desc = sum_even_desc + i
;
io([: descending_even_sum = [sum_even_desc] ])

sum_even_desc_negative ~= 0
loop i in 10 upto 0 by -2:
    sum_even_desc_negative = sum_even_desc_negative + i
;
io([: descending_even_sum_negative = [sum_even_desc_negative] ])
