-- Test missing import error handling
#import @nonexistent_module

print("This should not compile")
nonexistent_module.some_function()