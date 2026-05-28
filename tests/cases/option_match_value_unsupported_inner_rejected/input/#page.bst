User = | name String |
maybe_user User? = none

if maybe_user is:
    User("Ana") => io("same")
    else => io("other")
;
