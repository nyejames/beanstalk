-- Test structs and collection literals in JavaScript backend.
-- Struct field reads/writes are exercised here; collection indexing remains covered by lower-level JS tests.

Point = |
    x Int,
    y Int,
|

Person = |
    name String,
    age Int,
|

-- Test struct construction
p1 = Point(10, 20)
io([: Point: ([p1.x], [p1.y])])

-- Test struct with strings
person = Person("Alice", 30)
io([: Person: [person.name], age [person.age]])

-- Test field assignment
p2 ~= Point(0, 0)
p2.x = 5
p2.y = 10
io([: Updated point: ([p2.x], [p2.y])])

-- Test empty collection literal
empty = {}

-- Test collection literals with elements
numbers = {1, 2, 3, 4, 5}

-- Test collection of strings
names = {"Alice", "Bob", "Charlie"}

-- Test nested collections
matrix = {{1, 2}, {3, 4}}

io("Struct and collection tests passed")
