-- Test file: Lifetime inference with complex control flow (should pass)
-- Tests that the fixed lifetime inference handles complex control flow patterns correctly

-- Initialize variables
condition = true
data = "test"

-- Complex control flow with borrowing
if condition:
    -- Create local reference within scope
    local_ref = data
    temp_result = local_ref
    
    -- Modify data after local reference is done
    data = "modified in if"
else
    -- Alternative path
    data = "modified in else"
;

-- After control flow, data should be usable
final_result = data