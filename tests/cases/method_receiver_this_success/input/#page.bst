Vector2 = |
    x Float,
    y Float,
|

reset |this ~Vector2|:
    this.x = 0.0
    this.y = 0.0
;

vec ~= Vector2(12.0, 87.0)
~vec.reset()

[: [vec.x], [vec.y]]
