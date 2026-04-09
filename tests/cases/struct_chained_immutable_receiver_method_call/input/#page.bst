Counter = |
    count Int,
|

bumped |this Counter| -> Counter:
    return Counter(this.count + 1)
;

c = Counter(1)
io(c.bumped().bumped().count)
