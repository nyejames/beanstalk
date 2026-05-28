Box type T = |
    value T,
|

single type T |value T| -> {T}:
    return {value}
;

wrap_collection type T |value T| -> Box of {T}:
    return Box(single(value))
;

values {Int} = single(1)
values_with_capacity {Int 64} = single(2)
values_without_capacity {Int} = values_with_capacity
boxed_values Box of {String} = wrap_collection("leaf")

io([: generic_constructed_types_success [values.length()] [values_without_capacity.length()]])
