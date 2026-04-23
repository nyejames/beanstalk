Point = |
    x Int,
|

set_x |this ~Point, new_x Int|:
    this.x = new_x
;

p ~= Point(0)
~p.set_x(10)
io(p.x)
~p.set_x(20)
io(p.x)
