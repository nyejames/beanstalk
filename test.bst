-- Simple test of HTML project builder
io("Hello from JavaScript!")

number = 12

-- Test recursive block emission with control flow

add |x Int, y Int| -> Int:
    return x + y
;

result = add(5, 3)
io(result)

-- Test if-else
if result > 5:
    io("Greater than 5")
else
    io("Not greater than 5")
;

-- Test loop
loop i in 0 to 3:
    io(i)
;


#[: <h1>Here is a compile time template inserted into the HTML.</h1> ]

[: This should all become some HTML in the HTML build system from the runtime. With this number: [number]]