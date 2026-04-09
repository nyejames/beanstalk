-- Bare item/index bindings for collection and range loops
items = {2, 4, 6}
sum ~= 0

loop items item, index:
    sum = sum + item + index
;

count ~= 0
loop 0 to 5 i:
    count = count + i
;

io([: bare_sum = [sum], bare_count = [count] ])
