import @core/time {Duration, mark_now, elapsed_since}

-- This function references a JS-only @core/time call but is never reached from start.
-- HTML-Wasm should succeed because the call is not syntactically reachable.
unused_wrapper || -> String:
    m = mark_now()
    e = elapsed_since(m)
    return [:unused delta=[e.as_milliseconds()]]
;

[:core_time_unused_wasm_wrapper_success]
