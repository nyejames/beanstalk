-- Test missing import error handling
#import @nonexistent_module

["This should not compile"]
nonexistent_module.some_function()