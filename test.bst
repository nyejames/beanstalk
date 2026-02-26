-- Simple test of HTML project builder
io("Hello from JavaScript!")

number = 42

#[: Here is a compile time template inserted into the HTML. ]

[: This should all become some HTML in the HTML build system from the runtime. With this number: [number]]