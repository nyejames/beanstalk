-- Broken module with a syntax error inside a function body.
-- The syntax error is in a variable declaration: the type is followed by
-- an unexpected token instead of '=' which triggers the Syntax error.
#outer_fn ||:
    broken_var String |unexpected|
;
