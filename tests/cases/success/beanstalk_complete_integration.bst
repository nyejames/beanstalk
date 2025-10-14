-- Test file: Complete Beanstalk language integration
-- Tests implicit borrowing, mutability syntax, and scope semantics together

-- Initialize variables with different semantics
immutable_data = "constant value"
mutable_counter ~= 0
mutable_accumulator ~= 100

-- Implicit borrowing of immutable data
ref1 = immutable_data    -- Shared borrow
ref2 = immutable_data    -- Another shared borrow (allowed)
ref3 = ref1              -- Borrow of a borrow

-- Mutable operations within scopes
if mutable_counter is 0:
    -- Mutate variables within scope
    mutable_counter = mutable_counter + 1
    mutable_accumulator = mutable_accumulator * 2
    
    -- Create references within scope
    local_ref = ref1
    local_counter_ref ~= mutable_counter  -- Mutable access
;

-- Nested scopes with mixed semantics
condition1 = mutable_counter > 0
condition2 = mutable_accumulator > 150

if condition1:
    final_result ~= 0

    if condition2:
        -- Deep nesting with borrowing and mutation
        temp_ref = ref2
        mutable_counter = mutable_counter + 10
        final_result = mutable_accumulator + mutable_counter
    else
        -- Alternative path
        temp_ref = ref3
        final_result = mutable_accumulator
    ;
    
    -- Scope-local operations
    scope_result = final_result + 5
else
    -- Outer else branch
    scope_result = 0
;

-- Final operations after scopes
end_ref = immutable_data
end_counter = mutable_counter