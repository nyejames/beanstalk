mutate |value ~String| -> String:
    return value
;

out = mutate([:content])
[:function_call_mutable_param_fresh_template_arg out=[out]]
