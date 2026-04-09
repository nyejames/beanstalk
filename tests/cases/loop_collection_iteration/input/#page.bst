-- Collection iteration with pipe bindings
items = {1, 2, 3, 4}
sum ~= 0

loop items item:
    sum = sum + item
;

io([: collection_sum = [sum] ])
