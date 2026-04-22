-- AST rejects struct value passed to io()

Point = |
    x Int,
    y Int,
|

point = Point(1, 2)
io(point)
