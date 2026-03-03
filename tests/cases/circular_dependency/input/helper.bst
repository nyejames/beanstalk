-- Helper that creates circular dependency
#import @main

export do_something ||:
    io("Helper doing something")
;

io("Helper loaded")
