Inner = |
    value Int,
|

Outer = |
    inner Inner,
|

wrapper ~= Outer(Inner(1))
wrapper.inner.value = 5
io(wrapper.inner.value)
