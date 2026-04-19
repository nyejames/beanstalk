-- Prove error.push_trace() is emitted through final output.
-- Uses a multi-return function and named handler to access err and call the helper.

can_error |name String| -> String, Float, Error!:
    return! Error("Other", "test.trace", "original message")
;

trace_error |name String| -> String, Float:
    value, score = can_error(name) err! "fallback", 0.0:
        frame = StackFrame(function = "trace_error")
        traced = err.push_trace(frame)
        summary = [:error_helper_push_trace_runtime_contract msg=[traced.message]]
        io(summary)
    ;
    return value, score
;

msg, score = trace_error("test")
io(msg)
