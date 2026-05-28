-- Validates that a mutable optional variable can be reassigned to `none`.
-- The mutation site must preserve the Option(_) parse context so the `none`
-- literal can resolve its inner type.

name ~String? = none
name = none

io("ok")
