-- Collection iteration with item + index bindings
items = {10, 20, 30}
weighted ~= 0

loop items item, index:
    weighted = weighted + item + index
;

io([: weighted = [weighted] ])
