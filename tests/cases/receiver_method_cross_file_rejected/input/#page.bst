import @helper/Point

-- describe is declared here, but Point is defined in helper.bst — violates same-file rule
describe |this Point| -> String:
    return [: ([this.x], [this.y])]
;

p = Point(1, 2)
io(p.describe())
