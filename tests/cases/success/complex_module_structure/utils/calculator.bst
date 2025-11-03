-- Calculator utility module
export compute |a Int, b Int| -> Int:
    result = a + b
    ["Computing: " + a + " + " + b + " = " + result]
    return result
;

export multiply |a Int, b Int| -> Int:
    result = a * b
    ["Multiplying: " + a + " * " + b + " = " + result]
    return result
;

-- Calculator module initialization
["Calculator module initialized"]