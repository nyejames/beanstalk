-- Test complex import error scenarios
#import @nonexistent/module
#import @utils/broken_module

["This should not compile"]
nonexistent.function()
broken_module.function()