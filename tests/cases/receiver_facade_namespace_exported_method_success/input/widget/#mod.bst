export Counter = |
    count Int,
|

export make_counter || -> Counter:
    return Counter(41)
;

export tick |this Counter| -> Int:
    return this.count + 1
;
