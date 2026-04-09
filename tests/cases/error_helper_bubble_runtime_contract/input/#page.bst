-- Prove error.bubble() is emitted through final output.
-- Uses a multi-return function to propagate an error with bubble semantics.

can_error |name String| -> String, Int, Error!:
    return! Error("Other", "test.bubble", "original message")
;

bubble_error |name String| -> String, Int, Error!:
    value, number = can_error(name) err!:
        return! err.bubble()
    ;
    return value, number
;

result, count = bubble_error("test") ! "bubbled_to_fallback", 0
io(result)
io(count)
