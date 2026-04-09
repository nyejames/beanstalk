Person = |
    name String,
    age Int,
|

person ~= Person("Alice", 30)

name_ref = person.name
age_ref = person.age

io([: [name_ref] [age_ref]])
person.age = person.age + 1
io(person.age)
