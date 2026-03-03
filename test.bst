func_test |str String, str2 String| -> String:
    return str
;

func_test("test", "second argument",)

-- Newline inside function call arguments
io(
    func_test("Hello, world!", "second argument")
)