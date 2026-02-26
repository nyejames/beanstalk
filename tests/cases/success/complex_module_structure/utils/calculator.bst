-- Calculator utility module
#compute |a Int, b Int| -> Int:
    result = a + b
    io([: Computing: a + b = result])
    return result
;

#multiply |a Int, b Int| -> Int:
    result = a * b
    io([: Multiplying: a * b = result])
    return result
;

-- Calculator module initialization
io("Calculator module initialized")