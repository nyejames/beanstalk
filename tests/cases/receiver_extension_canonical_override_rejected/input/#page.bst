import @helper { Point }

describe |this Point| -> String:
    return "extension"
;

p = Point(1, 2)
io(p.describe())
