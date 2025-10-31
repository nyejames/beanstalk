-- Helper that creates circular dependency
#import @main

export do_something || -> Void:
    print("Helper doing something")
;

print("Helper loaded")