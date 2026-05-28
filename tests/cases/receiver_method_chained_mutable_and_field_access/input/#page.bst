Box = |
    x Int,
|

shift |this ~Box|:
    this.x = this.x + 1
;

b ~= Box(7)
~b.shift()
io(b.x)
