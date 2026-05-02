Box type T = |
    value T,
|

IntBox as Box of Int

alias_value IntBox = Box(1)
raw Box of Int = alias_value

[: concrete generic alias]
