-- Exercises all five collection built-in methods through the full pipeline.

mutate_and_read || -> Int:
    values ~= {10, 20, 30}
    ~values.push(40)
    ~values.set(0, 99)
    ~values.remove(1)
    first = values.get(0) ! 0
    return values.length()
;

total = mutate_and_read()
io(total)