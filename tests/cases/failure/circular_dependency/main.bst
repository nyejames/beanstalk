-- Test circular dependency detection
#import @helper

print("Main starting")
helper.do_something()