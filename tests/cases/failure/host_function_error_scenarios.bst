-- Test various host function error scenarios
print()  -- Missing argument
print("arg1", "arg2")  -- Too many arguments
unknown_host_function("test")  -- Unknown host function