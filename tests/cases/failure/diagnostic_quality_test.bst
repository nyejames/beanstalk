-- Test diagnostic message quality for various error types

-- Syntax error
print("unclosed string

-- Type error  
number = 42
print(number + "string")

-- Undefined variable
print(undefined_variable)

-- Host function error
print(123)  -- Wrong argument type