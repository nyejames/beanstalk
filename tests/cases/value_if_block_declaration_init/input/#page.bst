choose |condition Bool| -> Int:
    result = if condition:
        then 30
    else
        then 40
    ;
    return result
;

value = choose(true)
io([:value if block declaration init result=[value]])
