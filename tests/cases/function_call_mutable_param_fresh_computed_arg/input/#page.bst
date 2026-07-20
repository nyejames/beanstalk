mutate |value ~Int| -> Int:
    value = value + 1
    return value
;

out = mutate(1 + 2)
[:function_call_mutable_param_fresh_computed_arg out=[out]]
