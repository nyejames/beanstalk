describe |prefix String = "item", subject String| -> String:
    return prefix + ":" + subject
;

result = describe(subject = "apple")
[:function_call_named_args_default_skip result=[result]]
