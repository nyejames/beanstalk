Counter = |
    count Int,
|

increment |this ~Counter|:
    this.count = this.count + 1
;

c ~= Counter(5)
~c.increment()
io(c.count)
