pair_opt || -> String?, Int?:
    return none, none
;

name String?, count Int? = pair_opt()
io(name)
io(count)
