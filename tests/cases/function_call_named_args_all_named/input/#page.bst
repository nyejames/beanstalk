greet |name String, greeting String| -> String:
    return greeting + " " + name
;

result = greet(name = "world", greeting = "hello")
[:function_call_named_args_all_named result=[result]]
