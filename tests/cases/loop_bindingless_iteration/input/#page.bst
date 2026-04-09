-- Binding-less iteration loops are valid for both collection and range forms.
items = {2, 4, 6}
item_count ~= 0

loop items:
    item_count = item_count + 1
;

range_count ~= 0
loop 0 to 3:
    range_count = range_count + 1
;

io([: item_count = [item_count], range_count = [range_count] ])
