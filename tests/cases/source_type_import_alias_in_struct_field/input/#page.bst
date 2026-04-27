import @types/Person as Human

Group = |
    owner Human,
|

g = Group(owner = Human(name = "Ana"))
io(g.owner.name)
