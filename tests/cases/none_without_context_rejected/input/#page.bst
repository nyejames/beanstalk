-- Validates that `none` is rejected when there is no explicit optional type
-- context. A declaration without a type annotation cannot resolve the inner
-- type of `none`.

name = none

io("should not reach here")
