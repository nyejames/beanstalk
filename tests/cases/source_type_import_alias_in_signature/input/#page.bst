import @types/Person as Human

describe |person Human| -> String:
    return person.name
;

ana = Person(name = "Ana")
io(describe(ana))
