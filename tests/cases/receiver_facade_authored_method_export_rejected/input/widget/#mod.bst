export Counter = |
    count Int,
|

export tick |this Counter| -> Int:
    return this.count + 1
;
