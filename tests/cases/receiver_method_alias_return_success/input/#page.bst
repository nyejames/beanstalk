Point = |
    x Int,
    y Int,
|

self_ref |this Point| -> this:
    return this
;

p ~= Point(1, 2)
q ~= p.self_ref()
q.x = 99
io(q.x)
