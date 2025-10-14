-- Test file: Function calls with borrow checker
-- Tests requirement 7.3: borrow checker integration with function calls

-- Simple function definition
-- This takes a mutable integer and returns an immutable integer
-- The immutable integer is a new copy
simple_function |value ~Int| -> Int:
    result = value + 1
    return result
;

-- Function that takes a string parameter
-- String without the ~ symbol is a string slice or immutable string
-- In this case a new owned immutable string is created by wrapping the string slice in a template
string_function |text String| -> String:
    return [text]
;

-- Test basic function calls
number = 42
result = simple_function(number)

-- Test function calls with immutable strings
message = "hello"
echo = string_function(message)

-- Test multiple function calls
value1 = simple_function(10)
value2 = simple_function(value1)