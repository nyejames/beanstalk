-- Test that struct field mutation routes through the JS computed-place layer.
-- Validates that __bs_field and __bs_write are used for field assignment.

Point = |
    x Int,
    y Int,
|

p = Point(1, 2)
p.x = 10
p.y = 20
io([: Point: ([p.x], [p.y])])
