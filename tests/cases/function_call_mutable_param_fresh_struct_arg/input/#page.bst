Item = |
    label String,
|

mutate |value ~Item| -> String:
    return value.label
;

label = mutate(Item("x"))
[:function_call_mutable_param_fresh_struct_arg label=[label]]
