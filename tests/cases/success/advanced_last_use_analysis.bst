-- Test file: Advanced last-use analysis (should pass)
-- Tests precise last-use detection in complex scenarios

-- Test 1: Last use in conditional branches
data1 = "branch test"
condition = true

if condition:
    -- Use data1 in this branch - should be last use in this path
    branch_result = data1
else
    -- Alternative path doesn't use data1
    branch_result = "alternative"
;

-- After conditional, mutable access should work if last use was detected
data1 = "modified"

-- Test 2: Last use with multiple references
data2 = "multi ref test"
ref1 = data2
ref2 = data2
ref3 = data2

-- Use references in sequence - last one should be detected as last use
temp1 = ref1
temp2 = ref2
temp3 = ref3  -- This should be the last use

-- After last use of all references, mutable access should work
data2 = "changed"

-- Test 3: Last use with function-like patterns
data3 = "function test"
param_ref = data3

-- Simulate function call pattern
result = param_ref  -- Last use of param_ref

-- After function-like usage, original should be available for mutable access
data3 = "function result"