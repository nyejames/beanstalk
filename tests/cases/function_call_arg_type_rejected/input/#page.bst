-- Validates that passing a wrong-type argument to a user function produces a clean error.

takes_string |value String|:
    io(value)
;

takes_string(1)
