Point = |
    x Int,
    y Int,
|

load_index |items {Int}, idx Int| -> Int, Error!:
    return items.get(idx)!
;

point = Point(1, 2)
values ~= {point.x, point.y, 3}
values.set(2, 8)
selected = load_index(~values, 1) ! 0

io(selected)
io(values.length())
