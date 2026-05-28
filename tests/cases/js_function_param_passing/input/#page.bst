-- Test that function parameters are normalized through __bs_param_binding
-- and that call arguments use the reference ABI (__bs_binding wrapping).

double |n Int| -> Int:
    return n * 2
;

identity |value Int| -> Int:
    return value
;

x = 5
result1 = double(x)
result2 = identity(42)
io(result1)
io(result2)
