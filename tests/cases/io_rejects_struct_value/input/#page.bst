-- TODO(display): Replace this failure fixture once io() accepts Display-style struct values.
-- AST currently rejects struct values passed to io().

Point = |
    x Int,
    y Int,
|

point = Point(1, 2)
io(point)
