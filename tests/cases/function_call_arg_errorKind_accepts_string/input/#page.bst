-- Validates expected/actual ordering in user function argument validation.
-- ErrorKind is one-way compatible: expected=ErrorKind accepts actual=String,
-- but expected=String does NOT accept actual=ErrorKind.
-- If the compatibility call were reversed, this would incorrectly fail.

log_error |message ErrorKind|:
    io("called")
;

log_error("something went wrong")
