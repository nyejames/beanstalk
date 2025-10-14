-- Test file: Beanstalk scope semantics with : and ; delimiters
-- Tests that scope delimiters map correctly to WASM block structures

-- Simple conditional scope
condition = true
if condition:
    result = 42
else
    result = 0
;

-- Nested scopes
outer_condition = true
inner_condition = false

if outer_condition:
    nested_result ~= 3
    if inner_condition:
        nested_result = 1
    else
        nested_result = 2
    ;
    final_result = nested_result
else
    final_result = 0
;

-- Function-like scope (when functions are implemented)
-- test_function |x Int| -> Int:
--     if x > 0:
--         return x * 2
--     else
--         return 0
--     ;
-- ;

-- Variable scoping within blocks
global_var = 100
if true:
    local_var = 50
    scoped_result = global_var + local_var
;

-- Error handling scope (when implemented)
-- risky_operation() !err:
--     return err
-- ;