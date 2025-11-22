-- Test circular dependency detection
#import @helper

io("Main starting")
helper.do_something()