-- Prove error.with_location() is emitted through final output.
-- Uses a multi-return function and named handler to access err and call the helper.

can_error |name String| -> String, Float, Error!:
    return! Error("Other", "test.location", "original message")
;

annotate_error |name String| -> String, Float:
    value, score = can_error(name) err! "fallback", 0.0:
        loc = ErrorLocation(file = "test.bst", line = 42, column = 1)
        annotated = err.with_location(loc)
        io(annotated.message)
    ;
    return value, score
;

msg, score = annotate_error("test")
io(msg)
