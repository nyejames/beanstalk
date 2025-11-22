-- Test function with return
simple_function |value String| -> String:
    io(value)
    return value
;

simple_function("beans")
