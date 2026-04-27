import @types/Person as Human

Group = |
    owner Human,
|

g = Group(owner = Person(name = "Ana"))
io(g.owner.name)
