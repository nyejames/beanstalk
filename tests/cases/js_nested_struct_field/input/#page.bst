-- Test that nested struct field access composes __bs_field calls.
-- Validates the computed-place layer for nested struct paths.

Inner = |
    value Int,
|

Outer = |
    inner Inner,
|

wrapper = Outer(Inner(42))
io(wrapper.inner.value)
