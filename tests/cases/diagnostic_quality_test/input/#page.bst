-- Test diagnostic message quality for various error types

-- Syntax error
["unclosed string

-- Type error  
number = 42
io(number + "string")

-- Undefined variable
io(undefined_variable)

-- Host function error
io(123)  -- Wrong argument type