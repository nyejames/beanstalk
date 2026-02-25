-- Test string operations in JavaScript backend
-- Validates string literals, concatenation, and escaping

main ||:
    -- Test basic strings
    str1 = "Hello"
    str2 = "World"
    io(str1 + " " + str2)
    
    -- Test string concatenation
    greeting = "Hello, " + "World!"
    io(greeting)
    
    -- Test strings with special characters
    newline = "Line 1\nLine 2"
    io(newline)
    
    tab = "Column1\tColumn2"
    io(tab)
    
    quote = "She said \"Hello\""
    io(quote)
    
    -- Test empty string
    empty = ""
    io("Empty string: '" + empty + "'")
    
    -- Test character literals
    ch1 = 'a'
    ch2 = 'Z'
    ch3 = '😊'
    io([: Characters: ch1, ch2, ch3])
    
    io("String operation tests passed")
;
