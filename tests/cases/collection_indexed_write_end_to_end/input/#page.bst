-- Tests collection indexed write syntax (get/assign) through the full pipeline.

values ~= {10, 20, 30}
values.get(1) = 99
first = values.get(0) ! 0
io(first)
