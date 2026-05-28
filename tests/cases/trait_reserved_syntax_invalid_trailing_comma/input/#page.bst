-- Test that trailing commas in trait method returns are rejected
Parser must:
    parse |input String| -> Result, String,
;
