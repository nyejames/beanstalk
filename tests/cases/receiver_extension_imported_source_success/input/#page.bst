import @helper { Point }

describe |this Point| -> String:
    return [: ([this.x], [this.y])]
;

p = Point(1, 2)
[:receiver_extension_imported_source_success value=[p.describe()]]
