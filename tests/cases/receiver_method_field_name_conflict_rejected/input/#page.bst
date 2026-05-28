Counter = |
    count Int,
|

-- "count" is already a field on Counter
count |this Counter| -> Int:
    return this.count
;

c = Counter(0)
io(c.count())
