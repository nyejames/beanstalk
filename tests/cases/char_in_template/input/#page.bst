-- Test Char interpolated in a template string.
c = 'Z'
message ~= [: The letter is [c]]
io(message)
