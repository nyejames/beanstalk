mutate |values ~{Int}| -> Int:
    ~values.push(4)
    return values.length()
;

len = mutate({1, 2, 3})
[:function_call_mutable_param_fresh_collection_arg len=[len]]
