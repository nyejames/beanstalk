-- Helper that creates circular dependency
#import @main

export do_something || -> Void:
    ["Helper doing something"]
;

["Helper loaded"]