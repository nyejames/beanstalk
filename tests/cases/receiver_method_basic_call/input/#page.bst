Point = |
    x Int,
    y Int,
|

describe |this Point| -> String:
    return [: ([this.x], [this.y])]
;

p = Point(3, 4)
io(p.describe())
