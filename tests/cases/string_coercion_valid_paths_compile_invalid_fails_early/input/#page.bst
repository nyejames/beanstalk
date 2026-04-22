-- Regression: valid scalar/template paths compile, invalid struct fails early

-- Valid String
io("hello")

-- Valid Int
io(42)

-- Valid Float
io(3.14)

-- Valid Bool
io(true)

-- Valid Char (via template)
c = 'A'
[:char_value [c]]

-- Valid template passed to io
message ~= [:template_test]
io(message)

-- Invalid struct should fail while the above compiled fine
Point = |
    x Int,
    y Int,
|

point = Point(1, 2)
io(point)
