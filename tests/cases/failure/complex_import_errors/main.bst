-- Test complex import error scenarios
#import @nonexistent/module
#import @utils/broken_module

print("This should not compile")
nonexistent.function()
broken_module.function()