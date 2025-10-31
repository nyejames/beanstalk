-- Calculator utility module
export compute |a Int, b Int| -> Int:
    result = a + b
    print("Computing: " + a + " + " + b + " = " + result)
    return result
;

export multiply |a Int, b Int| -> Int:
    result = a * b
    print("Multiplying: " + a + " * " + b + " = " + result)
    return result
;

-- Calculator module initialization
print("Calculator module initialized")