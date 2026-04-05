increment |value ~Int| -> Int:
    value = value + 1
    return value
;

x ~= 10
result = increment(x)
io(result)
