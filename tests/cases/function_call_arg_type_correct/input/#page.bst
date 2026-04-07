-- Validates that user function calls accept correctly-typed arguments.

takes_string |value String|:
    io(value)
;

takes_string("hello")
